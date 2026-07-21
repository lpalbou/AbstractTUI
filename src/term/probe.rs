//! Active capability probing: the sans-IO [`ActiveProbe`] state machine
//! and the cell-pixel-size refresh helper.
//!
//! OWNER: KERNEL. Query formats and citations: `docs/design/term-input.md`
//! §2.2; startup sequencing recipe (who runs the probe and when): §2.3.
//!
//! Protocol: a batch of feature queries followed by a **DA1 sentinel** —
//! terminals answer input FIFO, so once DA1 answers, everything that was
//! going to answer has answered. The prober never blocks (the driver loop
//! with the deadline is `input::probe_active`) and is therefore trivially
//! safe against mute terminals. It must never run at a `dumb` terminal
//! (RT1-6b) — the driver checks `Capabilities::dumb` before writing a
//! single byte.

use super::caps::{Capabilities, CapsReply, WrapKind};
use super::verbs::tmux_wrap;
use super::Terminal;
use crate::base::PixelSize;

/// Unique id for the kitty-graphics probe so its reply cannot be confused
/// with any application image traffic that carries ids of its own.
const KITTY_GFX_PROBE_ID: &str = "4242";
/// Distinct id for the tmux-WRAPPED kitty probe: a reply carrying it is
/// proof the payload crossed tmux to the outer terminal and the answer
/// crossed back — i.e. `allow-passthrough` is enabled.
const KITTY_GFX_WRAPPED_ID: &str = "4343";

/// Extra wait after tmux's own DA1 sentinel for wrapped replies: they pay
/// an additional round trip through the outer terminal, so FIFO ordering
/// against the DIRECT sentinel does not hold for them. Passthrough-off
/// sessions (the tmux default) simply spend this window once at startup —
/// the probe runs concurrently with first paint (§2.3), so it is not
/// user-visible latency.
pub const TMUX_GRACE: std::time::Duration = std::time::Duration::from_millis(150);

/// Sans-IO active capability prober: hands out query bytes, folds
/// [`CapsReply`]s into a [`Capabilities`], and knows when the DA1
/// sentinel (plus tmux grace) means the probe is complete. The IO loop
/// lives in `input::probe_active`.
#[derive(Debug, Default)]
pub struct ActiveProbe {
    done: bool,
    saw_any: bool,
    /// Probing THROUGH tmux: direct queries are answered by tmux itself,
    /// wrapped queries by the outer terminal (iff allow-passthrough).
    tmux: bool,
    /// tmux's own DA1 answered; wrapped replies may still be in flight.
    sentinel_seen: bool,
    /// Wrapped kitty-graphics reply arrived (passthrough + kitty proven).
    wrapped_kitty_seen: bool,
    /// XTVERSION replies seen. Under tmux the FIRST is tmux's own (FIFO:
    /// the direct query precedes the wrapped one), the SECOND is the
    /// outer terminal answering through passthrough.
    xtversion_seen: u8,
}

impl ActiveProbe {
    /// A direct-only prober (no tmux wrapping); prefer
    /// [`ActiveProbe::for_caps`], which reads the environment facts.
    pub fn new() -> Self {
        Self::default()
    }

    /// A prober aware of the environment: under tmux it expects the
    /// wrapped-query section of [`Self::full_query_bytes`] and interprets
    /// duplicate replies accordingly.
    pub fn for_caps(caps: &Capabilities) -> Self {
        ActiveProbe {
            tmux: caps.in_tmux,
            ..Self::default()
        }
    }

    fn direct_queries(b: &mut Vec<u8>) {
        // kitty keyboard: report current progressive-enhancement flags.
        b.extend_from_slice(b"\x1b[?u");
        // DECRQM for synchronized output (2026) and SGR-Pixels mouse
        // (1016) — active evidence, no env folklore for either.
        b.extend_from_slice(b"\x1b[?2026$p");
        b.extend_from_slice(b"\x1b[?1016$p");
        // XTVERSION: terminal name + version.
        b.extend_from_slice(b"\x1b[>0q");
        // XTSMGRAPHICS: read (Pa=1 color registers, Pi=1 read, Pv=0).
        b.extend_from_slice(b"\x1b[?1;1;0S");
        // XTWINOPS 16: one cell's size in pixels (reply: CSI 6;h;w t).
        b.extend_from_slice(b"\x1b[16t");
        // kitty graphics: query action (a=q) with a 1x1 RGB payload and a
        // unique id; neither stored nor displayed by the terminal.
        b.extend_from_slice(
            format!("\x1b_Gi={KITTY_GFX_PROBE_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\").as_bytes(),
        );
    }

    /// The passthrough verification section (tmux only): the same shapes,
    /// wrapped for the OUTER terminal. Any reply proves passthrough; the
    /// XTVERSION reply additionally names the outer terminal (the iTerm2/
    /// WezTerm image path has no query form of its own), and the kitty
    /// reply proves the outer speaks kitty graphics.
    fn wrapped_queries(b: &mut Vec<u8>) {
        b.extend_from_slice(&tmux_wrap(b"\x1b[>0q"));
        b.extend_from_slice(&tmux_wrap(
            format!("\x1b_Gi={KITTY_GFX_WRAPPED_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\").as_bytes(),
        ));
    }

    /// The classic direct-only batch (kept for callers that built against
    /// it; misses passthrough verification under tmux).
    pub fn query_bytes() -> Vec<u8> {
        let mut b = Vec::with_capacity(160);
        Self::direct_queries(&mut b);
        b.extend_from_slice(b"\x1b[c"); // DA1 sentinel ends the probe
        b
    }

    /// The full batch for THIS prober: direct queries, then (under tmux)
    /// the wrapped passthrough section, then the DA1 sentinel — last so
    /// FIFO still guarantees every earlier answer precedes it. tmux
    /// answers DA1 itself, so the sentinel works identically inside tmux.
    pub fn full_query_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(224);
        Self::direct_queries(&mut b);
        if self.tmux {
            Self::wrapped_queries(&mut b);
        }
        b.extend_from_slice(b"\x1b[c");
        b
    }

    /// Fold one reply into `caps`. Returns `true` when the probe is
    /// complete: the DA1 sentinel arrived and (under tmux) no wrapped
    /// reply is still worth waiting for — see [`Self::awaiting_wrapped`].
    pub fn on_reply(&mut self, reply: &CapsReply, caps: &mut Capabilities) -> bool {
        self.saw_any = true;
        match reply {
            CapsReply::KittyKeyboard { .. } => {
                // Any reply at all — flags included or zero — proves the
                // protocol exists (the query itself is part of it).
                caps.kitty_keyboard = true;
            }
            CapsReply::DecMode { mode: 2026, status } => {
                // 1 = set, 2 = reset: both mean "recognized". 0 and 4 mean
                // unusable — direct evidence, allowed to lower an env guess.
                caps.sync_output_2026 = matches!(status, 1 | 2);
            }
            CapsReply::DecMode { mode: 1016, status } => {
                caps.sgr_pixel_mouse = matches!(status, 1 | 2);
            }
            CapsReply::DecMode { .. } => {}
            CapsReply::XtVersion { text } => {
                self.xtversion_seen = self.xtversion_seen.saturating_add(1);
                if !self.tmux {
                    caps.term_version = Some(text.clone());
                } else if self.xtversion_seen == 1 {
                    // FIFO: the direct query precedes the wrapped one, so
                    // the first reply is tmux introducing itself — better
                    // evidence than the env-derived version string.
                    caps.tmux_version = Some(text.clone());
                } else {
                    // The outer terminal answered THROUGH tmux: proof of
                    // allow-passthrough, plus the outer's identity — the
                    // only detection path for the iTerm2 image protocol
                    // (it has no query form of its own). Names, not
                    // guesses: only known image-capable outers flip bits.
                    caps.term_version = Some(text.clone());
                    caps.graphics_wrap = Some(WrapKind::Tmux);
                    let lower = text.to_ascii_lowercase();
                    if lower.contains("iterm") || lower.contains("wezterm") {
                        caps.iterm2_images = true;
                    }
                }
            }
            CapsReply::KittyGraphics { raw } => {
                let s = String::from_utf8_lossy(raw);
                let ok = s.contains("OK");
                // Reply must echo the right probe id and say OK; an error
                // reply (EINVAL...) still proves the protocol is spoken,
                // but not usably enough to advertise.
                if ok && s.contains(&format!("i={KITTY_GFX_PROBE_ID}")) {
                    caps.kitty_graphics = true;
                }
                if ok && self.tmux && s.contains(&format!("i={KITTY_GFX_WRAPPED_ID}")) {
                    // The wrapped probe round-tripped: allow-passthrough
                    // is on AND the outer terminal speaks kitty graphics.
                    caps.kitty_graphics = true;
                    caps.graphics_wrap = Some(WrapKind::Tmux);
                    self.wrapped_kitty_seen = true;
                }
            }
            CapsReply::XtSmGraphics {
                item: 1,
                status: 0,
                value,
            } => {
                // Color-register count. Terminals answer huge values for
                // "effectively unlimited"; clamp into the u16 the sixel
                // encoder can actually address.
                caps.sixel_max_registers = Some((*value).min(u16::MAX as u32) as u16);
            }
            CapsReply::XtSmGraphics { .. } => {}
            CapsReply::WindowOp { op: 6, a, b } => {
                // Cell size report: HEIGHT then WIDTH (xterm XTWINOPS).
                // Only accept sane values — a confused terminal reporting
                // zero must not overwrite an ioctl-derived size.
                let (h, w) = (*a, *b);
                if (1..=512).contains(&w) && (1..=512).contains(&h) {
                    caps.cell_pixel_size = Some(PixelSize::new(w as u16, h as u16));
                }
            }
            CapsReply::WindowOp { .. } => {}
            CapsReply::PrimaryDa { params } => {
                // DA1 attribute 4 = sixel graphics (vt100.net DA1 tables).
                // Under tmux this is TMUX's own DA1: a sixel attribute
                // there means tmux itself re-encodes sixel (built with
                // --enable-sixel) — usable WITHOUT wrapping, so the flag
                // stands on its own.
                if params.contains(&4) {
                    caps.sixel = true;
                }
                self.sentinel_seen = true;
                self.done = !self.awaiting_wrapped();
            }
            CapsReply::XtGetTcap { .. } | CapsReply::CursorPos { .. } | CapsReply::Osc { .. } => {}
        }
        if self.sentinel_seen && !self.awaiting_wrapped() {
            self.done = true;
        }
        self.done
    }

    /// True once the probe is complete (sentinel + no wrapped stragglers
    /// worth waiting for).
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// The DA1 sentinel arrived (tmux's own, when probing through tmux).
    pub fn sentinel_passed(&self) -> bool {
        self.sentinel_seen
    }

    /// Under tmux, wrapped replies cross to the outer terminal and back —
    /// an extra round trip the DIRECT sentinel does not bound. True while
    /// any wrapped answer could still usefully arrive; the driver grants
    /// [`TMUX_GRACE`] past the sentinel for them (passthrough-off
    /// sessions, the tmux default, never answer and simply spend the
    /// grace once).
    pub fn awaiting_wrapped(&self) -> bool {
        self.tmux && !(self.wrapped_kitty_seen && self.xtversion_seen >= 2)
    }

    /// Whether any reply arrived at all (diagnostic: distinguishes a mute
    /// terminal from a terminal that answered only the sentinel).
    pub fn saw_any_reply(&self) -> bool {
        self.saw_any
    }
}

/// Re-read the platform's cell pixel geometry into `caps` — call after a
/// resize, where cell metrics may change (font zoom resizes report the
/// same cell COUNT with different pixels). Platform-only (no wire round
/// trip): when the platform cannot measure (`None`) an earlier `CSI 16 t`
/// result is kept, because "cannot measure locally" is not evidence the
/// old wire answer went stale. Returns the value now in effect.
pub fn refresh_cell_pixel_size(
    term: &mut dyn Terminal,
    caps: &mut Capabilities,
) -> Option<PixelSize> {
    if let Some(px) = term.cell_pixel_size() {
        caps.cell_pixel_size = Some(px);
    }
    caps.cell_pixel_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_folds_replies_and_ends_on_da1() {
        let mut caps = Capabilities::default();
        let mut probe = ActiveProbe::new();
        assert!(!probe.on_reply(&CapsReply::KittyKeyboard { flags: 0 }, &mut caps));
        assert!(caps.kitty_keyboard);
        assert!(!probe.on_reply(
            &CapsReply::DecMode {
                mode: 2026,
                status: 2
            },
            &mut caps
        ));
        assert!(caps.sync_output_2026);
        assert!(!probe.on_reply(
            &CapsReply::XtVersion {
                text: "kitty 0.38.1".into()
            },
            &mut caps
        ));
        assert_eq!(caps.term_version.as_deref(), Some("kitty 0.38.1"));
        assert!(!probe.on_reply(
            &CapsReply::KittyGraphics {
                raw: b"Gi=4242;OK".to_vec()
            },
            &mut caps
        ));
        assert!(caps.kitty_graphics);
        assert!(!probe.on_reply(
            &CapsReply::XtSmGraphics {
                item: 1,
                status: 0,
                value: 1024
            },
            &mut caps
        ));
        assert_eq!(caps.sixel_max_registers, Some(1024));
        assert!(!probe.on_reply(&CapsReply::WindowOp { op: 6, a: 18, b: 9 }, &mut caps));
        assert_eq!(caps.cell_pixel_size, Some(PixelSize::new(9, 18)));
        assert!(probe.on_reply(
            &CapsReply::PrimaryDa {
                params: vec![62, 4, 22]
            },
            &mut caps
        ));
        assert!(caps.sixel && probe.is_done());
    }

    #[test]
    fn probe_lowers_env_optimism_with_evidence() {
        let mut caps = Capabilities {
            sync_output_2026: true,
            ..Capabilities::default()
        };
        let mut probe = ActiveProbe::new();
        // GNOME Terminal answers status 4 (permanently reset): unusable.
        probe.on_reply(
            &CapsReply::DecMode {
                mode: 2026,
                status: 4,
            },
            &mut caps,
        );
        assert!(!caps.sync_output_2026);
    }

    #[test]
    fn probe_rejects_foreign_graphics_reply() {
        let mut caps = Capabilities::default();
        let mut probe = ActiveProbe::new();
        // Wrong id: some other image traffic answered, not our probe.
        probe.on_reply(
            &CapsReply::KittyGraphics {
                raw: b"Gi=31;OK".to_vec(),
            },
            &mut caps,
        );
        assert!(!caps.kitty_graphics);
        // Right id but an error message: protocol spoken, not advertised.
        probe.on_reply(
            &CapsReply::KittyGraphics {
                raw: b"Gi=4242;EINVAL:bad".to_vec(),
            },
            &mut caps,
        );
        assert!(!caps.kitty_graphics);
    }

    #[test]
    fn probe_rejects_insane_replies() {
        let mut caps = Capabilities::default();
        let mut probe = ActiveProbe::new();
        // Failed XTSMGRAPHICS (status 1/2/3) must not set a count.
        probe.on_reply(
            &CapsReply::XtSmGraphics {
                item: 1,
                status: 3,
                value: 0,
            },
            &mut caps,
        );
        assert_eq!(caps.sixel_max_registers, None);
        // Register count saturates into u16.
        probe.on_reply(
            &CapsReply::XtSmGraphics {
                item: 1,
                status: 0,
                value: 1_000_000,
            },
            &mut caps,
        );
        assert_eq!(caps.sixel_max_registers, Some(u16::MAX));
        // Zero / absurd cell sizes are refused.
        probe.on_reply(&CapsReply::WindowOp { op: 6, a: 0, b: 9 }, &mut caps);
        assert_eq!(caps.cell_pixel_size, None);
        probe.on_reply(
            &CapsReply::WindowOp {
                op: 6,
                a: 20_000,
                b: 9,
            },
            &mut caps,
        );
        assert_eq!(caps.cell_pixel_size, None);
        // Non-cell window ops are ignored, not misread.
        probe.on_reply(
            &CapsReply::WindowOp {
                op: 4,
                a: 720,
                b: 1280,
            },
            &mut caps,
        );
        assert_eq!(caps.cell_pixel_size, None);
    }

    #[test]
    fn query_batch_ends_with_da1_sentinel() {
        let q = ActiveProbe::query_bytes();
        assert!(q.ends_with(b"\x1b[c"));
        let s = String::from_utf8(q).unwrap();
        assert!(s.contains("\x1b[?u"));
        assert!(s.contains("\x1b[?2026$p"));
        assert!(s.contains("\x1b[?1016$p"));
        assert!(s.contains("\x1b[>0q"));
        assert!(s.contains("\x1b[?1;1;0S"));
        assert!(s.contains("\x1b[16t"));
        assert!(s.contains("a=q"));
    }

    fn tmux_caps() -> Capabilities {
        Capabilities {
            in_tmux: true,
            needs_tmux_passthrough: true,
            ..Capabilities::default()
        }
    }

    #[test]
    fn tmux_probe_batch_appends_wrapped_section() {
        let plain = ActiveProbe::for_caps(&Capabilities::default());
        assert_eq!(plain.full_query_bytes(), ActiveProbe::query_bytes());

        let probe = ActiveProbe::for_caps(&tmux_caps());
        let q = probe.full_query_bytes();
        assert!(q.ends_with(b"\x1b[c"), "sentinel stays last");
        let s = String::from_utf8_lossy(&q).into_owned();
        assert!(s.contains("\x1bPtmux;"), "wrapped section present");
        // Inside the wrapper every ESC is doubled: the wrapped kitty
        // query's APC introducer must appear as ESC ESC _.
        assert!(s.contains("\x1b\x1b_Gi=4343"), "{s:?}");
        // Direct probe keeps its own id — replies stay distinguishable.
        assert!(s.contains("\x1b_Gi=4242,"));
    }

    #[test]
    fn tmux_passthrough_verified_by_wrapped_replies() {
        let mut caps = tmux_caps();
        let mut probe = ActiveProbe::for_caps(&caps.clone());
        // tmux introduces itself first (direct XTVERSION, FIFO).
        probe.on_reply(
            &CapsReply::XtVersion {
                text: "tmux 3.5a".into(),
            },
            &mut caps,
        );
        assert_eq!(caps.tmux_version.as_deref(), Some("tmux 3.5a"));
        assert_eq!(caps.term_version, None);
        // The wrapped kitty query round-trips: passthrough + kitty proven.
        probe.on_reply(
            &CapsReply::KittyGraphics {
                raw: b"Gi=4343;OK".to_vec(),
            },
            &mut caps,
        );
        assert!(caps.kitty_graphics);
        assert_eq!(caps.graphics_wrap, Some(WrapKind::Tmux));
        // The outer terminal names itself through the wrapper.
        let done_before_sentinel = probe.on_reply(
            &CapsReply::XtVersion {
                text: "kitty 0.38.1".into(),
            },
            &mut caps,
        );
        assert!(!done_before_sentinel, "sentinel still owed");
        assert_eq!(caps.term_version.as_deref(), Some("kitty 0.38.1"));
        // Sentinel arrives with every wrapped answer in: probe completes.
        assert!(probe.on_reply(&CapsReply::PrimaryDa { params: vec![62] }, &mut caps));
        assert!(!probe.awaiting_wrapped());
    }

    #[test]
    fn tmux_passthrough_off_stays_conservative() {
        let mut caps = tmux_caps();
        let mut probe = ActiveProbe::for_caps(&caps.clone());
        probe.on_reply(
            &CapsReply::XtVersion {
                text: "tmux 3.3".into(),
            },
            &mut caps,
        );
        // Only tmux's own DA1 answers: not done — the driver grants the
        // bounded grace, and when nothing arrives the flags stay off.
        assert!(!probe.on_reply(&CapsReply::PrimaryDa { params: vec![62] }, &mut caps));
        assert!(probe.sentinel_passed() && probe.awaiting_wrapped());
        assert!(!caps.kitty_graphics);
        assert_eq!(caps.graphics_wrap, None);
        assert!(!caps.iterm2_images);
    }

    #[test]
    fn tmux_outer_iterm2_unlocks_iterm_images_only() {
        let mut caps = tmux_caps();
        let mut probe = ActiveProbe::for_caps(&caps.clone());
        probe.on_reply(
            &CapsReply::XtVersion {
                text: "tmux 3.4".into(),
            },
            &mut caps,
        );
        // Sentinel first: iTerm2 never answers the wrapped KITTY query,
        // so its XTVERSION reply is the straggler the grace window buys.
        assert!(!probe.on_reply(&CapsReply::PrimaryDa { params: vec![64] }, &mut caps));
        probe.on_reply(
            &CapsReply::XtVersion {
                text: "iTerm2 3.5.10".into(),
            },
            &mut caps,
        );
        assert!(caps.iterm2_images, "outer iTerm2 named through the wrapper");
        assert_eq!(caps.graphics_wrap, Some(WrapKind::Tmux));
        assert!(!caps.kitty_graphics, "no kitty proof, no kitty claim");
        assert_eq!(caps.term_version.as_deref(), Some("iTerm2 3.5.10"));
    }

    #[test]
    fn outside_tmux_wrapped_ids_and_second_versions_are_inert() {
        let mut caps = Capabilities::default();
        let mut probe = ActiveProbe::new();
        // A foreign reply carrying the WRAPPED id outside tmux (some app
        // reusing our id): must not set the wrap bit.
        probe.on_reply(
            &CapsReply::KittyGraphics {
                raw: b"Gi=4343;OK".to_vec(),
            },
            &mut caps,
        );
        assert_eq!(caps.graphics_wrap, None);
        assert!(!caps.kitty_graphics);
        // Two XTVERSION replies outside tmux: last one wins, no wrap.
        probe.on_reply(
            &CapsReply::XtVersion {
                text: "kitty 0.1".into(),
            },
            &mut caps,
        );
        probe.on_reply(
            &CapsReply::XtVersion {
                text: "kitty 0.2".into(),
            },
            &mut caps,
        );
        assert_eq!(caps.term_version.as_deref(), Some("kitty 0.2"));
        assert_eq!(caps.graphics_wrap, None);
    }

    #[test]
    fn decrqm_1016_folds_pixel_mouse_bit() {
        let mut caps = Capabilities::default();
        let mut probe = ActiveProbe::new();
        probe.on_reply(
            &CapsReply::DecMode {
                mode: 1016,
                status: 2,
            },
            &mut caps,
        );
        assert!(caps.sgr_pixel_mouse);
        // "not recognized" lowers it again — evidence beats optimism.
        probe.on_reply(
            &CapsReply::DecMode {
                mode: 1016,
                status: 0,
            },
            &mut caps,
        );
        assert!(!caps.sgr_pixel_mouse);
    }
}

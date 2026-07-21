//! Terminal capability model: the [`Capabilities`] struct, the passive
//! environment pass, decoded reply frames ([`CapsReply`]), and the
//! consumer-facing views ([`GraphicsCaps`], `render::PresentCaps`).
//!
//! OWNER: KERNEL. The active prober lives in `term::probe`; query formats
//! and citations in `docs/design/term-input.md` §2.
//!
//! The env pass is free, instant and conservative; the active probe raises
//! or *lowers* fields with direct evidence (a DECRPM "not recognized"
//! beats any env guess).

use crate::base::PixelSize;
use crate::render::present::{ColorDepth, PresentCaps};

/// What the terminal can do. Booleans default to `false` except
/// `deferred_wrap` (see field doc); the env pass raises what it can prove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// 24-bit SGR color (38;2;r;g;b).
    pub truecolor: bool,
    /// 256-color palette (38;5;n). Implied by `truecolor`.
    pub colors_256: bool,
    /// Kitty keyboard protocol (CSI u progressive enhancement).
    pub kitty_keyboard: bool,
    /// Kitty graphics protocol (APC _G).
    pub kitty_graphics: bool,
    /// iTerm2 inline images (OSC 1337 File=).
    pub iterm2_images: bool,
    /// Sixel raster graphics (DA1 attribute 4).
    pub sixel: bool,
    /// Sixel color registers (XTSMGRAPHICS), when the terminal reported.
    pub sixel_max_registers: Option<u16>,
    /// Graphics payloads must be wrapped before emission (`Some(Tmux)` =
    /// pass through `term::tmux_wrap`). Set ONLY by the active probe when
    /// a wrapped query round-trips (proof that the user enabled
    /// `allow-passthrough`); `None` under tmux means graphics stay off.
    pub graphics_wrap: Option<WrapKind>,
    /// One cell's pixel geometry (platform ioctl or `CSI 16 t` reply).
    pub cell_pixel_size: Option<PixelSize>,
    /// SGR-Pixels mouse reporting (DEC 1016): coordinates arrive in
    /// pixels instead of cells. Detected by DECRQM probe only (no env
    /// folklore); consumers must convert via `EventReader::
    /// enable_pixel_mouse` — raw pixels must never pose as cell coords.
    pub sgr_pixel_mouse: bool,
    /// Synchronized output, DEC private mode 2026.
    pub sync_output_2026: bool,
    /// SGR mouse encoding (DEC 1006).
    pub sgr_mouse: bool,
    /// Bracketed paste (DEC 2004).
    pub bracketed_paste: bool,
    /// Focus in/out reporting (DEC 1004).
    pub focus_events: bool,
    /// OSC 8 hyperlinks.
    pub hyperlinks: bool,
    /// SGR 4:3 curly underline (colon subparams).
    pub undercurl: bool,
    /// SGR 58/59 colored underlines. Today the env evidence set is
    /// identical to `undercurl` (same terminal lineage introduced both);
    /// kept as a separate fact so a future probe can split them.
    pub underline_color: bool,
    /// Writing the last column leaves the cursor pending-wrap instead of
    /// wrapping immediately (xterm heritage). Every terminal in the
    /// supported matrix defers; this bit exists so ONE verified
    /// counterexample can flip the presenter to skip-last-column (RT1-5)
    /// without an engine release. Default TRUE — it is the property of
    /// the VT lineage itself, not an optional feature.
    pub deferred_wrap: bool,
    /// OSC 52 clipboard WRITE is honored (the read form is never emitted
    /// — see `Terminal::clipboard_copy`). Terminals that ignore the frame
    /// copy nothing silently, so callers report success only when this
    /// bit holds.
    pub osc52_copy: bool,
    /// OSC 9 desktop notifications (iTerm2 convention). Prefer
    /// `notify_channel()` over reading this directly.
    pub osc9_notify: bool,
    /// OSC 99 desktop notifications (kitty's protocol; kitty speaks no
    /// OSC 9). Prefer `notify_channel()` over reading this directly.
    pub osc99_notify: bool,
    /// Inside tmux, OSC/APC payloads meant for the OUTER terminal need
    /// `ESC Ptmux; … ESC \` wrapping with doubled ESCs (`term::tmux_wrap`)
    /// AND the user's `allow-passthrough` enabled — which is OFF by
    /// default since tmux 3.3a and invisible from the environment.
    /// Graphics therefore stay disabled under tmux (labeled degradation);
    /// this bit tells a future verified-passthrough path that wrapping
    /// would be required.
    pub needs_tmux_passthrough: bool,
    /// tmux version when identifiable (`TERM_PROGRAM_VERSION`, tmux 3.4+;
    /// older tmux exposes nothing version-shaped). Diagnostic/labeling.
    pub tmux_version: Option<String>,
    /// The session speaks UTF-8 (locale on unix, always on for our
    /// UTF-8-codepage windows session).
    pub unicode_ok: bool,
    /// `NO_COLOR` was set: the user asked for no color, independent of
    /// what the terminal supports (informal spec: no-color.org). The env
    /// pass forces color depth down; themes may want the raw fact.
    pub no_color: bool,
    /// TERM says this is not a terminal worth escaping at (`dumb`/empty).
    /// The active probe MUST be skipped (RT1-6b): emitting query bytes at
    /// a dumb terminal violates the same rule that zeroes everything else.
    pub dumb: bool,
    /// Running inside tmux/screen: env describes the multiplexer, graphics
    /// need passthrough (deferred), active probes answer AS the multiplexer.
    pub in_tmux: bool,
    /// `name version` from XTVERSION, when the terminal reported one.
    pub term_version: Option<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            truecolor: false,
            colors_256: false,
            kitty_keyboard: false,
            kitty_graphics: false,
            iterm2_images: false,
            sixel: false,
            sixel_max_registers: None,
            graphics_wrap: None,
            cell_pixel_size: None,
            sgr_pixel_mouse: false,
            sync_output_2026: false,
            sgr_mouse: false,
            bracketed_paste: false,
            focus_events: false,
            hyperlinks: false,
            undercurl: false,
            underline_color: false,
            deferred_wrap: true, // property of the VT lineage; see field doc
            osc52_copy: false,
            osc9_notify: false,
            osc99_notify: false,
            needs_tmux_passthrough: false,
            tmux_version: None,
            unicode_ok: false,
            no_color: false,
            dumb: false,
            in_tmux: false,
            term_version: None,
        }
    }
}

impl Capabilities {
    /// Passive detection from the process environment.
    ///
    /// Free and instant — run it before first paint; the active probe
    /// (`input::probe_active`) upgrades the result concurrently with the
    /// first frame (design doc §2.3).
    ///
    /// ```
    /// use abstracttui::term::Capabilities;
    ///
    /// let caps = Capabilities::detect_env();
    /// // Feed the presenter and the graphics ladder from the same facts:
    /// let present = caps.present_caps();
    /// let gfx = caps.graphics();
    /// println!("{}", caps.summary_line()); // e.g. "truecolor, kitty-kbd, …"
    /// # let _ = (present, gfx);
    /// ```
    pub fn detect_env() -> Self {
        Self::detect_env_with(&|k| std::env::var(k).ok())
    }

    /// Same, with an injectable lookup so tests never touch process env.
    pub fn detect_env_with(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let get = |k: &str| lookup(k).unwrap_or_default();
        let term = get("TERM").to_ascii_lowercase();
        let term_program = get("TERM_PROGRAM");
        let colorterm = get("COLORTERM").to_ascii_lowercase();

        let mut c = Capabilities::default();

        // A terminal that identifies as dumb (or nothing at all) gets
        // nothing; emitting escapes at a dumb terminal is worse than plain
        // text. The linux console parses CSI but has no mouse/paste/focus.
        let dumb = term.is_empty() && term_program.is_empty() || term == "dumb";
        let linux_console = term == "linux";

        c.in_tmux =
            !get("TMUX").is_empty() || term.starts_with("tmux") || term.starts_with("screen");

        let kitty = !get("KITTY_WINDOW_ID").is_empty() || term == "xterm-kitty";
        let wezterm = term_program == "WezTerm" || !get("WEZTERM_EXECUTABLE").is_empty();
        let ghostty = term_program == "ghostty" || !get("GHOSTTY_RESOURCES_DIR").is_empty();
        let iterm2 = term_program == "iTerm.app" || !get("ITERM_SESSION_ID").is_empty();
        let windows_terminal = !get("WT_SESSION").is_empty();
        let apple_terminal = term_program == "Apple_Terminal";
        let vscode = term_program == "vscode";
        let foot = term == "foot" || term.starts_with("foot-");
        let vte_version: u32 = get("VTE_VERSION").parse().unwrap_or(0);
        let modern = kitty || wezterm || ghostty || iterm2 || windows_terminal || foot || vscode;

        c.truecolor =
            colorterm == "truecolor" || colorterm == "24bit" || term.contains("direct") || modern;
        c.colors_256 = c.truecolor || term.contains("256color") || apple_terminal;

        c.kitty_keyboard = kitty || wezterm || ghostty || foot;
        // WezTerm's kitty-graphics support is partial; its iTerm2 path is
        // complete, so we prefer that there and let the active probe raise
        // kitty_graphics only when the terminal proves it.
        c.kitty_graphics = kitty || ghostty;
        c.iterm2_images = iterm2 || wezterm;
        c.sixel = foot || term.contains("sixel");
        c.sync_output_2026 = modern;
        c.hyperlinks = modern || vte_version >= 5000;
        // SGR 4:3 undercurl: kitty lineage + VTE 0.52+ (52xx) + iTerm2 +
        // Windows Terminal (1.18+; env cannot see versions, accept).
        c.undercurl = kitty
            || wezterm
            || ghostty
            || foot
            || iterm2
            || windows_terminal
            || vte_version >= 5200;
        c.underline_color = c.undercurl; // same lineage, same evidence today
                                         // OSC 52 write: default-on in the kitty/wezterm/ghostty/foot/iterm2
                                         // lineage and Windows Terminal; xterm gates it behind allowWindowOps
                                         // (off by default) and VTE only grew it recently — both stay false
                                         // without evidence. tmux translates OSC 52 itself (set-clipboard
                                         // defaults to "external"), so in_tmux does not clear it.
        c.osc52_copy = kitty || wezterm || ghostty || foot || iterm2 || windows_terminal;
        // Desktop notifications, two dialects: OSC 9 (iTerm2 convention:
        // iTerm2/WezTerm/ghostty) and OSC 99 (kitty's protocol — kitty
        // never adopted OSC 9). ghostty speaks both; it stays on OSC 9 so
        // it can never double-notify. foot's OSC 777 remains deferred
        // until a consumer asks.
        c.osc9_notify = iterm2 || wezterm || ghostty;
        c.osc99_notify = kitty;

        // Near-universal in the modern era; the exceptions are terminals
        // that predate the modern era entirely.
        let interactive = !dumb && !linux_console;
        c.sgr_mouse = interactive;
        c.bracketed_paste = interactive;
        c.focus_events = interactive && !apple_terminal;

        // The user's explicit no-color request outranks terminal ability
        // (informal NO_COLOR spec: any non-empty value counts).
        c.no_color = !get("NO_COLOR").is_empty();
        if c.no_color {
            c.truecolor = false;
            c.colors_256 = false;
        }

        if c.in_tmux {
            // Graphics escapes reach the outer terminal only through
            // passthrough wrapping (`term::tmux_wrap`) AND the user's
            // allow-passthrough setting, which is off by default since
            // tmux 3.3a and undetectable from env. Claiming support would
            // draw garbage on default configs: disabled, labeled, and the
            // passthrough need is recorded for a verified path later.
            c.kitty_graphics = false;
            c.iterm2_images = false;
            c.sixel = false;
            c.needs_tmux_passthrough = true;
            if term_program == "tmux" {
                let v = get("TERM_PROGRAM_VERSION");
                if !v.is_empty() {
                    c.tmux_version = Some(v);
                }
            }
        }

        #[cfg(windows)]
        {
            c.unicode_ok = true; // enter() sets the UTF-8 codepage.
                                 // RT1-12b: classic conhost does not translate mouse into VT
                                 // sequences under ENABLE_VIRTUAL_TERMINAL_INPUT — mouse would
                                 // be silently dead. Claim SGR mouse only inside a terminal
                                 // that identified itself as a modern emulator (Windows
                                 // Terminal, kitty, WezTerm, ghostty, iTerm2, foot, VS Code,
                                 // or anything setting TERM_PROGRAM); bare conhost degrades
                                 // to keyboard-only, honestly.
            c.sgr_mouse = c.sgr_mouse && (modern || !term_program.is_empty());
        }
        #[cfg(not(windows))]
        {
            let locale = [get("LC_ALL"), get("LC_CTYPE"), get("LANG")]
                .into_iter()
                .find(|v| !v.is_empty())
                .unwrap_or_default()
                .to_ascii_lowercase();
            c.unicode_ok = locale.contains("utf-8") || locale.contains("utf8");
        }

        if dumb {
            let unicode_ok = c.unicode_ok;
            let in_tmux = c.in_tmux;
            let no_color = c.no_color;
            let needs_tmux_passthrough = c.needs_tmux_passthrough;
            let tmux_version = c.tmux_version.take();
            c = Capabilities::default();
            c.unicode_ok = unicode_ok;
            c.in_tmux = in_tmux;
            c.no_color = no_color;
            c.needs_tmux_passthrough = needs_tmux_passthrough;
            c.tmux_version = tmux_version;
            c.dumb = true;
        }
        c
    }

    /// The presenter's whole view of this terminal (RENDER request 1,
    /// cycle 1). `NO_COLOR` folds to `Ansi16` — the closest depth
    /// `PresentCaps` can express; the raw `no_color` flag stays available
    /// for theme-level decisions.
    pub fn present_caps(&self) -> PresentCaps {
        PresentCaps::from(self)
    }

    /// Human-readable multi-line report for `--caps`-style debug flags
    /// (DESIGN/REDTEAM consumers). Stable-ish prose, not a wire format:
    /// scripts should read fields, humans read this.
    pub fn summary(&self) -> String {
        fn yn(b: bool) -> &'static str {
            if b {
                "yes"
            } else {
                "no"
            }
        }
        let mut s = String::with_capacity(768);
        let color = if self.no_color {
            "disabled (NO_COLOR)"
        } else if self.truecolor {
            "truecolor (24-bit)"
        } else if self.colors_256 {
            "256-color"
        } else {
            "16-color"
        };
        s.push_str("terminal capabilities\n");
        if let Some(v) = &self.term_version {
            s.push_str(&format!("  terminal        : {v}\n"));
        }
        if self.dumb {
            s.push_str("  TERM            : dumb — escapes suppressed, probe skipped\n");
        }
        if self.in_tmux {
            let ver = self.tmux_version.as_deref().unwrap_or("version unknown");
            s.push_str(&format!("  multiplexer     : tmux ({ver})\n"));
        }
        s.push_str(&format!("  color           : {color}\n"));
        s.push_str(&format!(
            "  unicode         : {}\n",
            if self.unicode_ok {
                "UTF-8"
            } else {
                "uncertain (locale not UTF-8)"
            }
        ));
        s.push_str(&format!(
            "  kitty keyboard  : {}\n",
            yn(self.kitty_keyboard)
        ));
        s.push_str(&format!(
            "  mouse           : SGR {} / pixel units {}\n",
            yn(self.sgr_mouse),
            yn(self.sgr_pixel_mouse)
        ));
        s.push_str(&format!(
            "  paste/focus     : bracketed {} / focus events {}\n",
            yn(self.bracketed_paste),
            yn(self.focus_events)
        ));
        s.push_str(&format!(
            "  sync output     : {} (DEC 2026)\n",
            yn(self.sync_output_2026)
        ));
        let gfx = match (self.kitty_graphics, self.iterm2_images, self.sixel) {
            (false, false, false) => "none (unicode mosaic fallback)".to_string(),
            _ => {
                let mut v = Vec::new();
                if self.kitty_graphics {
                    v.push("kitty");
                }
                if self.iterm2_images {
                    v.push("iTerm2");
                }
                if self.sixel {
                    v.push("sixel");
                }
                let mut t = v.join(" + ");
                if self.graphics_wrap == Some(WrapKind::Tmux) {
                    t.push_str(" (via tmux passthrough)");
                }
                t
            }
        };
        s.push_str(&format!("  graphics        : {gfx}\n"));
        if let Some(r) = self.sixel_max_registers {
            s.push_str(&format!("  sixel registers : {r}\n"));
        }
        if let Some(px) = self.cell_pixel_size {
            s.push_str(&format!("  cell size       : {}x{} px\n", px.w, px.h));
        }
        s.push_str(&format!(
            "  styling         : undercurl {} / underline color {} / hyperlinks {}\n",
            yn(self.undercurl),
            yn(self.underline_color),
            yn(self.hyperlinks)
        ));
        let notify = match self.notify_channel() {
            crate::term::verbs::NotifyChannel::Osc9 => "OSC 9",
            crate::term::verbs::NotifyChannel::Osc99 => "OSC 99 (kitty)",
            crate::term::verbs::NotifyChannel::BellOnly => "bell only",
        };
        s.push_str(&format!(
            "  desktop niceties: clipboard copy {} / notify {}\n",
            yn(self.osc52_copy),
            notify
        ));
        s.push_str(&format!(
            "  deferred wrap   : {} (presenter last-column strategy)\n",
            yn(self.deferred_wrap)
        ));
        s
    }

    /// One-line token summary for `--caps` debug flags and log lines:
    /// `truecolor, kitty-kbd, kitty-gfx, sync, mouse-sgr(+pixels), paste,
    /// focus, tmux(passthrough)`. Tokens appear only when TRUE (absence
    /// is the honest default); [`Self::summary`] is the multi-line
    /// human report with the negatives spelled out.
    pub fn summary_line(&self) -> String {
        let mut t: Vec<String> = Vec::with_capacity(12);
        if self.dumb {
            t.push("dumb".into());
        }
        t.push(
            if self.no_color {
                "no-color"
            } else if self.truecolor {
                "truecolor"
            } else if self.colors_256 {
                "256color"
            } else {
                "16color"
            }
            .into(),
        );
        if self.kitty_keyboard {
            t.push("kitty-kbd".into());
        }
        if self.kitty_graphics {
            t.push("kitty-gfx".into());
        }
        if self.iterm2_images {
            t.push("iterm2-img".into());
        }
        if self.sixel {
            match self.sixel_max_registers {
                Some(r) => t.push(format!("sixel({r})")),
                None => t.push("sixel".into()),
            }
        }
        if self.sync_output_2026 {
            t.push("sync".into());
        }
        if self.sgr_mouse {
            t.push(
                if self.sgr_pixel_mouse {
                    "mouse-sgr(+pixels)"
                } else {
                    "mouse-sgr"
                }
                .into(),
            );
        }
        if self.bracketed_paste {
            t.push("paste".into());
        }
        if self.focus_events {
            t.push("focus".into());
        }
        if self.undercurl {
            t.push("undercurl".into());
        }
        if self.osc52_copy {
            t.push("osc52".into());
        }
        if self.in_tmux {
            t.push(
                if self.graphics_wrap == Some(WrapKind::Tmux) {
                    "tmux(passthrough)"
                } else {
                    "tmux"
                }
                .into(),
            );
        }
        t.join(", ")
    }

    /// Which wire `Terminal::notify` should use for THIS terminal. One
    /// channel, never both: ghostty-class terminals speak both dialects
    /// and would pop two notifications.
    pub fn notify_channel(&self) -> crate::term::verbs::NotifyChannel {
        use crate::term::verbs::NotifyChannel;
        if self.osc99_notify {
            NotifyChannel::Osc99
        } else if self.osc9_notify {
            NotifyChannel::Osc9
        } else {
            NotifyChannel::BellOnly
        }
    }

    /// The graphics ladder's read-only view (GFX3D request 1, cycle 1).
    pub fn graphics(&self) -> GraphicsCaps {
        GraphicsCaps {
            kitty_graphics: self.kitty_graphics,
            iterm2_images: self.iterm2_images,
            sixel: self.sixel,
            sixel_max_registers: self.sixel_max_registers,
            cell_pixel_size: self.cell_pixel_size,
            wrap: self.graphics_wrap,
        }
    }
}

/// How graphics payloads must be wrapped before hitting the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WrapKind {
    /// `term::tmux_wrap`: `ESC Ptmux; <payload, ESC doubled> ESC \`.
    Tmux,
}

/// Conversion requested by RENDER (reviews/cycle1/render-requests.md §1):
/// apps never hand-assemble `PresentCaps`. Intra-crate import note: `term`
/// sits below `render` in the layer map; this impl references render's
/// TYPE without calling into render — the dependency arrow stays
/// "render consumes term" at runtime, and RENDER owns the struct.
impl From<&Capabilities> for PresentCaps {
    fn from(c: &Capabilities) -> PresentCaps {
        let color = if c.truecolor {
            ColorDepth::TrueColor
        } else if c.colors_256 {
            ColorDepth::Xterm256
        } else {
            ColorDepth::Ansi16
        };
        PresentCaps {
            color,
            sync_output_2026: c.sync_output_2026,
            hyperlinks: c.hyperlinks,
            undercurl: c.undercurl,
            underline_color: c.underline_color,
        }
    }
}

/// Everything the gfx protocol ladder needs, in one read-only handful
/// (kernel-owned; `gfx` consumes it — GFX3D cycle-1 request 1).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphicsCaps {
    /// Kitty graphics protocol usable (direct or via verified wrap).
    pub kitty_graphics: bool,
    /// iTerm2 OSC 1337 inline images usable.
    pub iterm2_images: bool,
    /// Sixel usable (DA1 attribute 4; under tmux: tmux's own re-encoder).
    pub sixel: bool,
    /// Sixel color registers when reported (XTSMGRAPHICS).
    pub sixel_max_registers: Option<u16>,
    /// One cell in pixels; scaling images/3D viewports needs it.
    pub cell_pixel_size: Option<PixelSize>,
    /// `Some(Tmux)`: every kitty/iTerm2 payload must go through
    /// `term::tmux_wrap` before `Presenter::external_write`. `None`
    /// under tmux means passthrough is unverified — the env pass already
    /// zeroed the protocol bits, so the ladder lands on mosaic.
    pub wrap: Option<WrapKind>,
}

/// A decoded terminal query reply, produced by `input::Parser` and consumed
/// by `term::probe::ActiveProbe`. Defined here (not in `input`) because
/// `term` sits below `input` in the layer map: input depends on term,
/// never the reverse.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)] // variant docs carry the wire format; the named
                       // fields are the escape sequences' own parameter names
pub enum CapsReply {
    /// `CSI ? flags u` — kitty keyboard protocol current-flags report.
    KittyKeyboard { flags: u32 },
    /// `CSI ? mode ; status $ y` — DECRPM. status: 0 unrecognized, 1 set,
    /// 2 reset, 3 permanently set, 4 permanently reset.
    DecMode { mode: u32, status: u8 },
    /// `CSI ? p1 ; p2 ; … c` — DA1 primary device attributes.
    PrimaryDa { params: Vec<u32> },
    /// `DCS > | text ST` — XTVERSION terminal name/version.
    XtVersion { text: String },
    /// `APC _G … ST` — kitty graphics reply, raw control-data payload
    /// (bounded by the parser). Contains `i=<id>` and `OK` on success.
    KittyGraphics { raw: Vec<u8> },
    /// `DCS 1|0 + r … ST` — XTGETTCAP reply, kept raw for a later cycle.
    XtGetTcap { raw: Vec<u8> },
    /// `CSI row ; col R` — cursor position report.
    CursorPos { row: u32, col: u32 },
    /// `CSI ? item ; status ; value S` — XTSMGRAPHICS report
    /// (item 1 = color registers; status 0 = success).
    XtSmGraphics { item: u32, status: u32, value: u32 },
    /// `CSI op ; a ; b t` — XTWINOPS report (op 6 = cell size in pixels,
    /// HEIGHT then WIDTH; op 4 = text area pixels; op 8 = chars).
    WindowOp { op: u32, a: u32, b: u32 },
    /// An OSC reply (e.g. color queries 10/11), kept raw for a later cycle.
    Osc { raw: Vec<u8> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> Capabilities {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Capabilities::detect_env_with(&move |k| map.get(k).cloned())
    }

    #[test]
    fn kitty_env_detected() {
        let c = env(&[
            ("TERM", "xterm-kitty"),
            ("KITTY_WINDOW_ID", "1"),
            ("LANG", "en_US.UTF-8"),
        ]);
        assert!(c.truecolor && c.colors_256);
        assert!(c.kitty_keyboard && c.kitty_graphics && c.undercurl);
        assert!(c.sync_output_2026 && c.hyperlinks && c.unicode_ok);
        assert!(!c.iterm2_images && !c.in_tmux && !c.dumb);
    }

    #[test]
    fn iterm_and_wezterm_prefer_iterm_images() {
        let it = env(&[("TERM_PROGRAM", "iTerm.app"), ("TERM", "xterm-256color")]);
        assert!(it.iterm2_images && !it.kitty_graphics);
        let wt = env(&[("TERM_PROGRAM", "WezTerm"), ("TERM", "xterm-256color")]);
        assert!(wt.iterm2_images && !wt.kitty_graphics && wt.kitty_keyboard);
    }

    #[test]
    fn tmux_masks_graphics_and_records_passthrough_need() {
        let c = env(&[
            ("TMUX", "/tmp/tmux-1000/default,123,0"),
            ("TERM", "tmux-256color"),
            ("KITTY_WINDOW_ID", "1"), // outer terminal leaks env into tmux
        ]);
        assert!(c.in_tmux && c.needs_tmux_passthrough);
        assert!(!c.kitty_graphics && !c.iterm2_images && !c.sixel);
        assert!(c.colors_256);
        assert_eq!(c.tmux_version, None); // pre-3.4 tmux: no version signal
                                          // OSC 52 survives tmux (tmux itself translates via set-clipboard).
        assert!(c.osc52_copy);

        let c = env(&[
            ("TMUX", "/tmp/tmux-1000/default,123,0"),
            ("TERM", "tmux-256color"),
            ("TERM_PROGRAM", "tmux"),
            ("TERM_PROGRAM_VERSION", "3.4"),
        ]);
        assert_eq!(c.tmux_version.as_deref(), Some("3.4"));
        assert!(c.needs_tmux_passthrough);
    }

    #[test]
    fn clipboard_and_notify_gates() {
        use crate::term::verbs::NotifyChannel;
        let c = env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]);
        assert!(c.osc52_copy);
        assert!(!c.osc9_notify, "kitty speaks OSC 99, not OSC 9");
        assert!(c.osc99_notify);
        assert_eq!(c.notify_channel(), NotifyChannel::Osc99);
        let c = env(&[("TERM_PROGRAM", "WezTerm"), ("TERM", "xterm-256color")]);
        assert!(c.osc52_copy && c.osc9_notify);
        assert_eq!(c.notify_channel(), NotifyChannel::Osc9);
        // ghostty speaks both dialects: exactly ONE channel is chosen.
        let c = env(&[("TERM_PROGRAM", "ghostty"), ("TERM", "xterm-ghostty")]);
        assert_eq!(c.notify_channel(), NotifyChannel::Osc9);
        // Plain xterm: allowWindowOps is off by default — no OSC 52 claim,
        // and notifications degrade to the bell.
        let c = env(&[("TERM", "xterm-256color")]);
        assert!(!c.osc52_copy && !c.osc9_notify && !c.osc99_notify);
        assert_eq!(c.notify_channel(), NotifyChannel::BellOnly);
    }

    #[test]
    fn dumb_terminal_gets_nothing_and_is_flagged() {
        let c = env(&[("TERM", "dumb"), ("LANG", "en_US.UTF-8")]);
        assert!(c.dumb);
        assert_eq!(
            c,
            Capabilities {
                unicode_ok: true,
                dumb: true,
                ..Capabilities::default()
            }
        );
        // Empty environment is equally dumb.
        assert!(env(&[]).dumb);
        // Anything real is not.
        assert!(!env(&[("TERM", "xterm-256color")]).dumb);
    }

    #[test]
    fn linux_console_keeps_color_drops_mouse() {
        let c = env(&[("TERM", "linux")]);
        assert!(!c.sgr_mouse && !c.bracketed_paste && !c.focus_events);
        assert!(!c.truecolor && !c.dumb);
    }

    #[test]
    fn plain_xterm_256color() {
        let c = env(&[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]);
        assert!(c.truecolor && c.colors_256 && c.bracketed_paste);
        // RT1-12b: a bare environment (no terminal-program identity)
        // keeps SGR mouse on unix but honestly drops it on Windows,
        // where classic conhost cannot translate mouse into VT.
        #[cfg(not(windows))]
        assert!(c.sgr_mouse);
        #[cfg(windows)]
        assert!(!c.sgr_mouse, "bare env must not claim mouse on windows");
        assert!(!c.kitty_keyboard && !c.kitty_graphics && !c.sixel);
        assert!(!c.undercurl, "no undercurl evidence for plain xterm");
    }

    #[test]
    fn no_color_forces_depth_down_not_features() {
        let c = env(&[
            ("TERM", "xterm-kitty"),
            ("COLORTERM", "truecolor"),
            ("NO_COLOR", "1"),
        ]);
        assert!(c.no_color);
        assert!(!c.truecolor && !c.colors_256);
        // NO_COLOR is about color, not interaction. (kitty identified
        // itself, so the mouse claim holds on every platform.)
        assert!(c.sgr_mouse && c.bracketed_paste && c.kitty_keyboard);
        assert_eq!(c.present_caps().color, ColorDepth::Ansi16);
    }

    #[test]
    fn deferred_wrap_defaults_true_everywhere() {
        assert!(Capabilities::default().deferred_wrap);
        assert!(env(&[("TERM", "xterm-256color")]).deferred_wrap);
        assert!(env(&[("TERM", "dumb")]).deferred_wrap);
    }

    #[test]
    fn present_caps_conversion() {
        let c = env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]);
        let p = c.present_caps();
        assert_eq!(p.color, ColorDepth::TrueColor);
        assert!(p.sync_output_2026 && p.hyperlinks && p.undercurl);
        assert!(p.underline_color);

        let c = env(&[("TERM", "xterm-256color")]);
        assert_eq!(c.present_caps().color, ColorDepth::Xterm256);
        let c = env(&[("TERM", "vt100")]);
        assert_eq!(c.present_caps().color, ColorDepth::Ansi16);
        // From<&Capabilities> is the same path.
        assert_eq!(PresentCaps::from(&c), c.present_caps());
    }

    #[test]
    fn summary_reads_true_and_stays_honest() {
        let mut c = env(&[
            ("TERM", "xterm-kitty"),
            ("KITTY_WINDOW_ID", "1"),
            ("LANG", "en_US.UTF-8"),
        ]);
        c.cell_pixel_size = Some(PixelSize::new(9, 18));
        c.sixel_max_registers = Some(256);
        c.term_version = Some("kitty 0.38.1".into());
        let s = c.summary();
        assert!(s.contains("terminal        : kitty 0.38.1"), "{s}");
        assert!(s.contains("color           : truecolor"), "{s}");
        assert!(s.contains("kitty keyboard  : yes"), "{s}");
        assert!(s.contains("graphics        : kitty"), "{s}");
        assert!(s.contains("cell size       : 9x18 px"), "{s}");
        assert!(s.contains("notify OSC 99"), "{s}");
        assert!(s.lines().count() >= 10, "multi-line report: {s}");

        // Degradations stay visible, not prettified.
        let c = env(&[("TERM", "dumb"), ("NO_COLOR", "1")]);
        let s = c.summary();
        assert!(s.contains("dumb — escapes suppressed"), "{s}");
        assert!(s.contains("disabled (NO_COLOR)"), "{s}");
        assert!(
            s.contains("graphics        : none (unicode mosaic fallback)"),
            "{s}"
        );
        assert!(s.contains("notify bell only"), "{s}");

        // tmux with verified passthrough labels the route.
        let mut c = env(&[("TMUX", "/tmp/t,1,0"), ("TERM", "tmux-256color")]);
        c.kitty_graphics = true;
        c.graphics_wrap = Some(WrapKind::Tmux);
        c.tmux_version = Some("tmux 3.7b".into());
        let s = c.summary();
        assert!(s.contains("multiplexer     : tmux (tmux 3.7b)"), "{s}");
        assert!(s.contains("kitty (via tmux passthrough)"), "{s}");
    }

    #[test]
    fn summary_line_tokens_track_truth() {
        let mut c = env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]);
        c.sixel = true;
        c.sixel_max_registers = Some(256);
        c.sgr_pixel_mouse = true;
        let line = c.summary_line();
        assert_eq!(
            line,
            "truecolor, kitty-kbd, kitty-gfx, sixel(256), sync, \
             mouse-sgr(+pixels), paste, focus, undercurl, osc52"
        );
        // tmux with verified passthrough labels the route; without it,
        // just the multiplexer fact.
        let mut c = env(&[("TMUX", "/tmp/t,1,0"), ("TERM", "tmux-256color")]);
        assert!(c.summary_line().ends_with(", tmux"), "{}", c.summary_line());
        c.graphics_wrap = Some(WrapKind::Tmux);
        assert!(
            c.summary_line().ends_with(", tmux(passthrough)"),
            "{}",
            c.summary_line()
        );
        // Degradations stay visible as their own tokens.
        let c = env(&[("TERM", "dumb"), ("NO_COLOR", "1")]);
        assert_eq!(c.summary_line(), "dumb, no-color");
    }

    #[test]
    fn graphics_view_mirrors_fields() {
        let mut c = env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]);
        c.sixel_max_registers = Some(256);
        c.cell_pixel_size = Some(PixelSize::new(9, 18));
        let g = c.graphics();
        assert!(g.kitty_graphics && !g.iterm2_images && !g.sixel);
        assert_eq!(g.sixel_max_registers, Some(256));
        assert_eq!(g.cell_pixel_size, Some(PixelSize::new(9, 18)));
    }
}

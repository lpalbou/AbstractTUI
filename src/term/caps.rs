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
///
/// `#[non_exhaustive]` (ADR-0003): new capability fields are the most
/// likely additive change this crate makes, and they must never be a
/// breaking release. Reading fields stays plain access; constructing a
/// custom set goes through [`Capabilities::with`] (struct literals and
/// functional-update syntax are crate-internal only):
///
/// ```compile_fail
/// // E0639 downstream: non_exhaustive forbids literal construction,
/// // functional update included.
/// let caps = abstracttui::term::Capabilities {
///     truecolor: true,
///     ..Default::default()
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Construct a custom capability set: defaults, adjusted in place.
    /// THE downstream constructor (ADR-0003) — the struct is
    /// `#[non_exhaustive]`, so struct literals and `..Default::default()`
    /// updates only compile inside this crate; this keeps the same
    /// ergonomics one field-set at a time and stays source-compatible
    /// when capability fields are added:
    ///
    /// ```
    /// use abstracttui::term::Capabilities;
    ///
    /// let caps = Capabilities::with(|c| {
    ///     c.truecolor = true;
    ///     c.colors_256 = true;
    /// });
    /// assert!(caps.truecolor && !caps.kitty_graphics);
    /// ```
    pub fn with(f: impl FnOnce(&mut Self)) -> Self {
        let mut caps = Self::default();
        f(&mut caps);
        caps
    }

    /// The capability set for a terminal that is not a terminal — a
    /// capture harness, a recorder, an embedding that only wants bytes.
    /// The counterpart to [`detect_env`](Self::detect_env): where that
    /// one reads the environment, this one deliberately does NOT, and
    /// that is its whole value.
    ///
    /// A headless sink has no `TERM`, no `COLORTERM`, no locale to
    /// consult — so running the env pass there does not detect anything
    /// about it, it detects *the developer's shell*, and the resulting
    /// output depends on who ran the suite. Colour is where that bites:
    /// a capture terminal reads back the bytes the presenter actually
    /// emitted, so a shell without `COLORTERM` quantises every asserted
    /// colour through the 256 cube while token-vs-token comparisons stay
    /// green. `Driver` substitutes this set automatically when
    /// [`RunConfig::caps`] is `None` and [`Terminal::is_tty`] is false.
    ///
    /// The set: full colour, UTF-8, nothing else. Full colour because a
    /// byte sink can record any colour and the harness should assert the
    /// colour it wrote; UTF-8 because the sink stores bytes, not a font.
    /// Every terminal-bound feature — kitty keyboard/graphics, sixel,
    /// OSC 52, mouse, paste, focus, synchronized output — stays OFF, so
    /// the emitted stream carries only what the frame needed.
    ///
    /// `NO_COLOR` is deliberately NOT honoured here. It is a request
    /// about a human's screen; a capture buffer has no human in front of
    /// it, and letting it move the bytes would reintroduce exactly the
    /// environment-dependent verdict this constructor exists to remove.
    /// A harness that wants to test the no-colour path asks for it:
    /// `Capabilities::with(|c| c.no_color = true)`.
    ///
    /// ```
    /// use abstracttui::term::Capabilities;
    ///
    /// let caps = Capabilities::headless();
    /// assert!(caps.truecolor && caps.colors_256 && caps.unicode_ok);
    /// assert!(!caps.kitty_graphics && !caps.sgr_mouse && !caps.no_color);
    /// ```
    ///
    /// [`RunConfig::caps`]: crate::app::RunConfig::caps
    /// [`Terminal::is_tty`]: crate::term::Terminal::is_tty
    pub fn headless() -> Self {
        Self::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })
    }

    // Passive env detection lives in a `#[path]` sibling (file-size
    // split): see caps_detect.rs — same impl, different file.
}

#[path = "caps_detect.rs"]
mod detect;

impl Capabilities {
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
///
/// `#[non_exhaustive]` like [`Capabilities`] (ADR-0003): graphics facts
/// grow with terminal protocols. Downstream construction goes through
/// [`GraphicsCaps::with`]; the usual source remains
/// [`Capabilities::graphics`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
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

impl GraphicsCaps {
    /// Construct a custom graphics-capability view: defaults, adjusted
    /// in place — the downstream constructor (ADR-0003), mirroring
    /// [`Capabilities::with`].
    ///
    /// ```
    /// use abstracttui::term::GraphicsCaps;
    ///
    /// let gfx = GraphicsCaps::with(|g| g.kitty_graphics = true);
    /// assert!(gfx.kitty_graphics && !gfx.sixel);
    /// ```
    pub fn with(f: impl FnOnce(&mut Self)) -> Self {
        let mut caps = Self::default();
        f(&mut caps);
        caps
    }
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
#[path = "caps_tests.rs"]
mod tests;

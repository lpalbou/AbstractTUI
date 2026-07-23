//! Capability-model tests (split file, `#[path]`-included as
//! `caps::tests` — env-detection matrices, downlevel folds, and the
//! summary/report honesty checks).
//!
//! OWNER: PROBE.

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
    assert!(wt.iterm2_images && !wt.kitty_graphics);
    // 0293: WezTerm ships enable_kitty_keyboard = false by default, so
    // the CLAIM is probe-gated — env alone must not assert it.
    assert!(
        !wt.kitty_keyboard,
        "WezTerm kitty-keyboard claim must wait for probe evidence"
    );
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
fn vscode_env_claims_xterm_js_facts_only() {
    // VS Code + Cursor + forks set TERM_PROGRAM=vscode (xterm.js).
    // Documented xterm.js facts claimed from env: truecolor, OSC 8
    // hyperlinks (>= 4.3), focus reporting (mode 1004). Everything
    // gated or absent stays off until the active probe proves it.
    let c = env(&[
        ("TERM_PROGRAM", "vscode"),
        ("TERM", "xterm-256color"),
        ("LANG", "en_US.UTF-8"),
    ]);
    assert!(c.truecolor && c.colors_256 && c.unicode_ok);
    assert!(c.hyperlinks, "xterm.js >= 4.3 renders OSC 8 links");
    assert!(c.focus_events, "xterm.js reports focus (DEC 1004)");
    assert!(c.sgr_mouse && c.bracketed_paste);
    assert!(
        !c.osc52_copy,
        "clipboard write is settings-gated in VS Code"
    );
    assert!(
        !c.kitty_graphics && !c.iterm2_images && !c.sixel,
        "no pixel-protocol claims from env for xterm.js"
    );
    assert!(!c.kitty_keyboard, "kitty keyboard needs probe evidence");
    assert!(!c.undercurl, "no undercurl claim without evidence");
    assert!(!c.dumb && !c.in_tmux);
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

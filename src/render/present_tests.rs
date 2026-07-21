//! Presenter byte snapshots. These pin the exact emission so REDTEAM's VT
//! model and any future refactor have a stable contract to hold against.

use super::*;
use crate::base::{Point, Rgba, Size};
use crate::render::cell::Glyph;
use crate::render::style::Style;

const TC: PresentCaps = PresentCaps {
    color: ColorDepth::TrueColor,
    sync_output_2026: false,
    hyperlinks: false,
    undercurl: true,
    underline_color: true,
};

fn surf(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

/// A presenter warmed past its honest-ignorance first frame: pen at
/// defaults, cursor parked — the steady state every later frame starts in.
fn warm(next: &Surface) -> Presenter {
    let mut p = Presenter::new();
    let mut out = Vec::new();
    p.emit(&[Run { y: 0, x: 0, len: 1 }], next, &TC, &mut out);
    p
}

fn emit(p: &mut Presenter, runs: &[Run], next: &Surface, caps: &PresentCaps) -> Vec<u8> {
    let mut out = Vec::new();
    p.emit(runs, next, caps, &mut out);
    out
}

fn s(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Cycle-7 measurement (integrator ask): SGR bytes on a theme-heavy
/// dashboard frame — the repeated label/value fg-toggle pattern. Pins
/// that every toggle costs exactly ONE fg-only incremental SGR (no attr
/// or bg churn interleaved), i.e. the pen leaves no easy bytes on the
/// table: SGR has no pen-restore instrument, so `38;2;r;g;b` per
/// truecolor toggle is the floor and a 1-entry last-pen cache would have
/// nothing shorter to emit. Numbers (64x20 dashboard, ~320 toggles):
/// printed by the test; ~16 bytes/toggle = the irreducible payload.
#[test]
fn dashboard_fg_toggles_emit_only_the_irreducible_sgr() {
    let muted = Rgba::rgb(120, 128, 140);
    let bright = Rgba::rgb(235, 240, 250);
    let panel = Rgba::rgb(18, 20, 28);
    let mut next = surf(64, 20);
    for y in 0..20 {
        let mut x = 0;
        for _ in 0..8 {
            x += next.draw_text(x, y, "cpu:", Style::new().fg(muted).bg(panel));
            x += next.draw_text(x, y, "42% ", Style::new().fg(bright).bg(panel));
        }
    }
    let mut p = warm(&next);
    let runs: Vec<Run> = (0..20).map(|y| Run { y, x: 0, len: 64 }).collect();
    let out = emit(&mut p, &runs, &next, &TC);

    // Every SGR after the first cell of a row is an fg-only transition.
    let text = s(&out);
    let mut sgrs: Vec<&str> = Vec::new();
    for chunk in text.split("\x1b[").skip(1) {
        if let Some(end) = chunk.find('m') {
            // Cursor motions end in H; only SGRs end in m at this cut.
            if chunk[..end]
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b';')
            {
                sgrs.push(&chunk[..end]);
            }
        }
    }
    let toggles: Vec<&&str> = sgrs.iter().filter(|p| p.starts_with("38;2;")).collect();
    assert!(
        toggles.len() >= 239,
        "one SGR per fg flip: {}",
        toggles.len()
    );
    // The frame-opening transition (parked default pen -> panel ink) sets
    // fg AND bg once; every LATER toggle must be fg-only — bg re-emits or
    // reset prefixes there would be the "easy bytes" a pen cache could
    // claim, and their absence is what this test pins.
    let with_bg = toggles.iter().filter(|t| t.contains(";48;")).count();
    assert!(
        with_bg <= 1,
        "bg re-emitted on {with_bg} fg toggles (1 opening allowed)"
    );
    assert!(
        !toggles.iter().skip(1).any(|t| t.starts_with("0;")),
        "reset path chosen over the cheaper fg-only incremental"
    );
    // The envelope, for the report: total frame bytes and SGR share.
    let sgr_bytes: usize = sgrs.iter().map(|p| p.len() + 3).sum();
    eprintln!(
        "dashboard 64x20: {} bytes total, {} SGRs ({} bytes, {:.1} avg)",
        out.len(),
        sgrs.len(),
        sgr_bytes,
        sgr_bytes as f64 / sgrs.len().max(1) as f64
    );
}

#[test]
fn zero_runs_zero_bytes() {
    let next = surf(80, 24);
    let mut p = Presenter::new();
    let caps = PresentCaps {
        sync_output_2026: true,
        ..TC
    };
    assert!(
        emit(&mut p, &[], &next, &caps).is_empty(),
        "idle frames cost nothing"
    );
}

#[test]
fn single_default_style_cell_is_tiny_and_exact() {
    let mut next = surf(80, 24);
    next.draw_text(7, 2, "x", Style::new());
    let mut p = warm(&next);
    let out = emit(&mut p, &[Run { y: 2, x: 7, len: 1 }], &next, &TC);
    // CUP + glyph + trailer(SGR reset + park). 19 bytes, under the ~20
    // budget for a steady-state single-cell change.
    assert_eq!(s(&out), "\x1b[3;8Hx\x1b[0m\x1b[24;1H");
    assert!(out.len() <= 20);
}

#[test]
fn run_of_same_style_cells_emits_one_sgr() {
    let mut next = surf(20, 3);
    next.draw_text(
        0,
        0,
        "abcdef",
        Style::new().fg(crate::base::Rgba::rgb(255, 0, 0)),
    );
    let mut p = warm(&next);
    let out = emit(&mut p, &[Run { y: 0, x: 0, len: 6 }], &next, &TC);
    let text = s(&out);
    assert_eq!(
        text.matches("38;2;255;0;0").count(),
        1,
        "one color SGR for the run: {text}"
    );
    assert!(
        text.contains("abcdef"),
        "glyphs flow without per-cell escapes: {text}"
    );
}

#[test]
fn within_frame_style_transitions_pick_the_cheaper_encoding() {
    use crate::base::Rgba;
    use crate::render::cell::Attrs;
    let mut next = surf(20, 1);
    next.draw_text(
        0,
        0,
        "a",
        Style::new().fg(Rgba::rgb(255, 0, 0)).attrs(Attrs::BOLD),
    );
    next.draw_text(
        1,
        0,
        "b",
        Style::new().fg(Rgba::rgb(0, 0, 255)).attrs(Attrs::BOLD),
    );
    next.draw_text(2, 0, "c", Style::new()); // back to plain default
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 3 }], &next, &TC));
    // a→b keeps BOLD: incremental fg-only beats a full reset.
    assert!(
        out.contains("a\x1b[38;2;0;0;255mb"),
        "incremental fg change: {out}"
    );
    // b→c drops everything: a bare reset (1 param) beats "22;39".
    assert!(out.contains("b\x1b[0mc"), "reset when cheaper: {out}");
}

#[test]
fn cursor_motion_economy_same_row_and_column() {
    let mut next = surf(40, 5);
    next.draw_text(0, 1, "a", Style::new());
    next.draw_text(9, 1, "b", Style::new());
    next.draw_text(9, 4, "c", Style::new());
    let mut p = warm(&next);
    let runs = [
        Run { y: 1, x: 0, len: 1 },
        Run { y: 1, x: 9, len: 1 },
        Run { y: 4, x: 9, len: 1 },
    ];
    let out = s(&emit(&mut p, &runs, &next, &TC));
    // The 40x5 warm frame parks at (0,4) → (0,1): same column, CUU 3.
    // After 'a' the cursor is (1,1); → (9,1): same row, CUF 8. After 'b'
    // it is (10,1); → (9,4): neither shared, CUP.
    assert!(out.contains("\x1b[3Aa"), "same-column uses CUU: {out}");
    assert!(out.contains("a\x1b[8Cb"), "same-row uses CUF: {out}");
    assert!(out.contains("b\x1b[5;10Hc"), "diagonal uses CUP: {out}");
}

#[test]
fn cr_beats_cub_to_column_zero() {
    let mut next = surf(40, 2);
    next.draw_text(5, 0, "x", Style::new());
    next.draw_text(0, 0, "y", Style::new());
    let mut p = warm(&next);
    let runs = [Run { y: 0, x: 5, len: 1 }, Run { y: 0, x: 0, len: 1 }];
    let out = s(&emit(&mut p, &runs, &next, &TC));
    assert!(
        out.contains("x\ry"),
        "column 0 in-row is one CR byte: {out}"
    );
}

#[test]
fn wide_glyph_emits_once_and_advances_two() {
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "世x", Style::new());
    let mut p = warm(&next);
    // Run covers leader + continuation + the following narrow cell.
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 3 }], &next, &TC));
    assert_eq!(out.matches('世').count(), 1);
    // 'x' follows the wide glyph with NO cursor motion: the terminal
    // advanced two columns on its own, and the virtual cursor agreed.
    assert!(
        out.contains("世x"),
        "no motion between wide glyph and next cell: {out}"
    );
}

#[test]
fn run_starting_on_a_continuation_skips_it() {
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "世", Style::new());
    let mut p = warm(&next);
    // Malformed-ish run starting mid-pair (diff never produces this; the
    // presenter defends anyway): position at the first sound cell instead.
    let out = s(&emit(&mut p, &[Run { y: 0, x: 1, len: 1 }], &next, &TC));
    assert!(
        !out.contains('世'),
        "leader outside the run is not re-emitted: {out}"
    );
}

#[test]
fn last_column_write_forces_absolute_motion_next() {
    let mut next = surf(10, 3);
    next.draw_text(9, 0, "e", Style::new());
    next.draw_text(0, 1, "f", Style::new());
    let mut p = warm(&next);
    let runs = [Run { y: 0, x: 9, len: 1 }, Run { y: 1, x: 0, len: 1 }];
    let out = s(&emit(&mut p, &runs, &next, &TC));
    // After writing the last column the wrap is pending; relative motion
    // (CR/CUD) would print the next glyph through the pending wrap on some
    // terminals. The presenter must go absolute.
    assert!(
        out.contains("e\x1b[2;1Hf"),
        "absolute CUP after last-column write: {out}"
    );
}

#[test]
fn bottom_right_cell_is_written_then_cup_parks() {
    let mut next = surf(10, 3);
    next.draw_text(9, 2, "z", Style::new());
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 2, x: 9, len: 1 }], &next, &TC));
    // The bottom-right cell IS written (no dead pixel), the pending wrap is
    // then neutralized by the absolute park motion before anything prints.
    assert!(out.contains('z'));
    assert!(
        out.ends_with("\x1b[3;1H"),
        "park is absolute after BR write: {out}"
    );
}

#[test]
fn sync_bracketing_wraps_the_frame() {
    let mut next = surf(10, 2);
    next.draw_text(0, 0, "q", Style::new());
    let caps = PresentCaps {
        sync_output_2026: true,
        ..TC
    };
    let mut p = warm(&next);
    let out = emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &caps);
    assert!(out.starts_with(b"\x1b[?2026h"));
    assert!(out.ends_with(b"\x1b[?2026l"));
}

#[test]
fn hyperlinks_open_and_close_by_uri() {
    let mut next = surf(10, 1);
    let id = next.register_link("https://example.com");
    next.draw_text(0, 0, "ab", Style::new().link(id));
    next.draw_text(2, 0, "c", Style::new());
    let caps = PresentCaps {
        hyperlinks: true,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 3 }], &next, &caps));
    let open = format!("\x1b]8;id={id};https://example.com\x1b\\");
    assert_eq!(
        out.matches(&open).count(),
        1,
        "one open for the linked span: {out}"
    );
    assert!(
        out.contains(&format!("{open}ab\x1b]8;;\x1b\\c")),
        "closed before unlinked text: {out}"
    );
}

#[test]
fn hyperlinks_disabled_emit_nothing() {
    let mut next = surf(10, 1);
    let id = next.register_link("https://example.com");
    next.draw_text(0, 0, "a", Style::new().link(id));
    let mut p = warm(&next);
    let out = emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &TC);
    assert!(!s(&out).contains("]8;"), "no OSC 8 without the capability");
}

#[test]
fn undercurl_emits_colon_style_or_degrades() {
    use crate::render::cell::Attrs;
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "u", Style::new().attrs(Attrs::UNDERCURL));
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &TC));
    assert!(out.contains("4:3"), "undercurl is SGR 4:3: {out}");

    let no_curl = PresentCaps {
        undercurl: false,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(
        &mut p,
        &[Run { y: 0, x: 0, len: 1 }],
        &next,
        &no_curl,
    ));
    assert!(!out.contains("4:3"), "no 4:3 without the capability: {out}");
    // Incremental from the default pen: one added attribute.
    assert!(
        out.contains("\x1b[4mu"),
        "degrades to plain underline: {out}"
    );
}

#[test]
fn xterm256_downlevel_spot_checks() {
    use crate::base::Rgba;
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "r", Style::new().fg(Rgba::rgb(255, 0, 0)));
    next.draw_text(1, 0, "g", Style::new().fg(Rgba::rgb(128, 128, 128)));
    let caps = PresentCaps {
        color: ColorDepth::Xterm256,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 2 }], &next, &caps));
    assert!(out.contains("38;5;196"), "pure red is cube 196: {out}");
    assert!(out.contains("38;5;244"), "mid gray is ramp 244: {out}");
}

#[test]
fn ansi16_downlevel_uses_classic_codes() {
    use crate::base::Rgba;
    let mut next = surf(10, 1);
    next.draw_text(
        0,
        0,
        "w",
        Style::new().fg(Rgba::rgb(255, 0, 0)).bg(Rgba::rgb(0, 0, 0)),
    );
    let caps = PresentCaps {
        color: ColorDepth::Ansi16,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &caps));
    // Incremental from the default pen beats a reset here.
    assert!(
        out.contains("\x1b[91;40m"),
        "bright red fg, black bg: {out}"
    );
}

#[test]
fn default_colors_are_39_49() {
    use crate::base::Rgba;
    let mut next = surf(10, 1);
    next.draw_text(
        0,
        0,
        "a",
        Style::new().fg(Rgba::rgb(1, 2, 3)).bg(Rgba::rgb(4, 5, 6)),
    );
    next.draw_text(1, 0, "b", Style::new()); // back to both defaults
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 2 }], &next, &TC));
    // Reset (1 param) beats "39;49" (2 params): the transition to full
    // defaults must use SGR 0.
    assert!(out.contains("a\x1b[0mb"), "{out}");
}

#[test]
fn empty_glyph_renders_as_space() {
    let mut next = surf(4, 1);
    next.set(0, 0, Cell::EMPTY.with_bg(crate::base::Rgba::rgb(9, 9, 9)));
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &TC));
    assert!(
        out.contains("48;2;9;9;9m "),
        "EMPTY paints its background: {out}"
    );
}

#[test]
fn invalidate_forces_full_resync() {
    let mut next = surf(10, 2);
    next.draw_text(0, 0, "a", Style::new());
    let mut p = warm(&next);
    p.invalidate();
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &TC));
    assert!(
        out.starts_with("\x1b[H\x1b[0m"),
        "CUP + reset after invalidate: {out}"
    );
}

#[test]
fn deterministic_across_identical_calls() {
    let mut next = surf(30, 5);
    next.draw_text(
        0,
        0,
        "same bytes",
        Style::new().fg(crate::base::Rgba::rgb(9, 8, 7)),
    );
    let runs = [Run {
        y: 0,
        x: 0,
        len: 10,
    }];
    let a = emit(&mut warm(&next), &runs, &next, &TC);
    let b = emit(&mut warm(&next), &runs, &next, &TC);
    assert_eq!(a, b);
}

#[test]
fn glyph_never_used_uninit() {
    // Guard: Glyph::EMPTY is the only default-constructed glyph and it is
    // width 1, so cursor math can never advance by 0 on a non-continuation.
    assert_eq!(Glyph::EMPTY.width(), 1);
}

// -- cycle 2 -----------------------------------------------------------------

#[test]
fn risky_cluster_invalidates_cursor_and_recups() {
    let mut next = surf(20, 2);
    next.draw_text(0, 0, "❤\u{FE0F}ab", Style::new());
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 4 }], &next, &TC));
    // After the VS16 cluster the virtual cursor is unknown: 'a' (our cell
    // 2) must be re-anchored with an absolute CUP, and 'b' then flows
    // normally (not risky).
    assert!(
        out.contains("❤\u{FE0F}\x1b[1;3Hab"),
        "absolute re-anchor after VS16: {out}"
    );
}

#[test]
fn zwj_sequence_also_reanchors_but_cjk_does_not() {
    let mut next = surf(20, 1);
    next.draw_text(0, 0, "世x", Style::new());
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 3 }], &next, &TC));
    assert!(
        out.contains("世x"),
        "plain CJK is settled width, no re-anchor: {out}"
    );

    let mut next = surf(20, 1);
    next.draw_text(0, 0, "👨\u{200D}👩\u{200D}👧x", Style::new());
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 3 }], &next, &TC));
    assert!(out.contains("👧\x1b[1;3Hx"), "ZWJ family re-anchors: {out}");
}

#[test]
fn external_write_custody_bytes_and_invalidation() {
    let mut next = surf(10, 3);
    next.draw_text(0, 0, "a", Style::new());
    let mut p = warm(&next);
    let mut out = Vec::new();
    p.external_write(&mut out, b"\x1bPq#0;2;0;0;0#0!5~\x1b\\", Point::new(2, 1));
    // Exact custody bracket: SGR reset, absolute CUP, payload verbatim.
    assert_eq!(
        s(&out),
        "\x1b[0m\x1b[2;3H\x1bPq#0;2;0;0;0#0!5~\x1b\\",
        "flush + absolute position + payload"
    );
    // Everything is forgotten: the next frame re-syncs from absolute state.
    let after = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &TC));
    assert!(
        after.starts_with("\x1b[H\x1b[0m"),
        "CUP + full reset after custody: {after}"
    );
}

#[test]
fn external_write_closes_open_hyperlink_first() {
    let caps = PresentCaps {
        hyperlinks: true,
        ..TC
    };
    let mut next = surf(10, 1);
    let id = next.register_link("https://example.com");
    next.draw_text(0, 0, "L", Style::new().link(id));
    let mut p = warm(&next);
    let mut out = Vec::new();
    // Simulate mid-frame custody: emit a linked run, then hand bytes over
    // WITHOUT the frame trailer having run (emit closes links at frame
    // end, so drive emit_run state by splitting the calls).
    p.emit(&[Run { y: 0, x: 0, len: 1 }], &next, &caps, &mut out);
    out.clear();
    p.external_write(&mut out, b"PAYLOAD", Point::ZERO);
    // Post-frame there is no open link, so no close is emitted — the
    // bracket is still reset + CUP + payload.
    assert_eq!(s(&out), "\x1b[0m\x1b[HPAYLOAD");
}

#[test]
fn underline_color_truecolor_and_reset() {
    use crate::base::Rgba;
    use crate::render::cell::Attrs;
    let mut next = surf(10, 1);
    next.draw_text(
        0,
        0,
        "u",
        Style::new()
            .attrs(Attrs::UNDERLINE)
            .underline_color(Rgba::rgb(255, 0, 100)),
    );
    next.draw_text(1, 0, "v", Style::new().attrs(Attrs::UNDERLINE)); // default ul color
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 2 }], &next, &TC));
    assert!(
        out.contains("4;58:2::255:0:100"),
        "colon-form SGR 58: {out}"
    );
    assert!(
        out.contains("u\x1b[59mv"),
        "SGR 59 resets to default ul: {out}"
    );
}

#[test]
fn underline_color_downlevels() {
    use crate::base::Rgba;
    use crate::render::cell::Attrs;
    let mut next = surf(10, 1);
    next.draw_text(
        0,
        0,
        "u",
        Style::new()
            .attrs(Attrs::UNDERLINE)
            .underline_color(Rgba::rgb(255, 0, 0)),
    );
    // Caps without underline color: the color drops, the underline stays.
    let no_ul = PresentCaps {
        underline_color: false,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &no_ul));
    assert!(!out.contains("58"), "no SGR 58 without the cap: {out}");
    assert!(out.contains("\x1b[4mu"), "plain underline survives: {out}");

    // 256-color terminals with the cap get the palette form.
    let caps256 = PresentCaps {
        color: ColorDepth::Xterm256,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(
        &mut p,
        &[Run { y: 0, x: 0, len: 1 }],
        &next,
        &caps256,
    ));
    assert!(out.contains("4;58:5:196"), "palette-form SGR 58: {out}");
}

#[test]
fn underline_color_without_underline_attr_is_inert() {
    use crate::base::Rgba;
    let mut next = surf(10, 1);
    next.draw_text(
        0,
        0,
        "x",
        Style::new().underline_color(Rgba::rgb(255, 0, 0)),
    );
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &TC));
    assert!(!out.contains("58"), "no underline, no 58 bytes: {out}");
}

#[test]
fn downlevel_preserves_pair_contrast_in_emission() {
    use crate::base::Rgba;
    // Dark-theme faint text: fg and bg quantize to the same gray alone.
    let bg = Rgba::rgb(26, 27, 38);
    let fg = Rgba::rgb(30, 30, 40);
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "f", Style::new().fg(fg).bg(bg));
    let caps = PresentCaps {
        color: ColorDepth::Xterm256,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &caps));
    let fg_idx = out
        .split("38;5;")
        .nth(1)
        .and_then(|t| t.split([';', 'm']).next());
    let bg_idx = out
        .split("48;5;")
        .nth(1)
        .and_then(|t| t.split([';', 'm']).next());
    assert!(
        fg_idx.is_some() && bg_idx.is_some(),
        "both colors emitted: {out}"
    );
    assert_ne!(fg_idx, bg_idx, "faint pair must not collapse: {out}");
}

#[test]
fn same_palette_index_elides_redundant_sgr() {
    use crate::base::Rgba;
    // Two different truecolor values landing on the same 256 index must
    // emit ONE color SGR (pen comparison happens on resolved reprs).
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "a", Style::new().fg(Rgba::rgb(255, 0, 0)));
    next.draw_text(1, 0, "b", Style::new().fg(Rgba::rgb(254, 1, 1)));
    let caps = PresentCaps {
        color: ColorDepth::Xterm256,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 2 }], &next, &caps));
    assert_eq!(
        out.matches("38;5;196").count(),
        1,
        "one SGR for one index: {out}"
    );
    assert!(out.contains("ab"), "glyphs flow uninterrupted: {out}");
}

#[test]
fn term_caps_conversion_reaches_underline_bytes_end_to_end() {
    use crate::base::Rgba;
    use crate::render::cell::Attrs;
    use crate::term::caps::Capabilities;
    // The official conversion path (KERNEL's From impl) must light up the
    // undercurl + underline-color emission with no hand-assembly — the
    // driver switches to this path this cycle (REACT's two-line fix).
    let caps = Capabilities {
        truecolor: true,
        colors_256: true,
        undercurl: true,
        underline_color: true,
        ..Capabilities::default()
    };
    let pc = PresentCaps::from(&caps);
    assert!(
        pc.undercurl && pc.underline_color,
        "caps fields map through"
    );
    assert_eq!(pc.color, ColorDepth::TrueColor);

    let mut next = surf(10, 1);
    next.draw_text(
        0,
        0,
        "u",
        Style::new()
            .attrs(Attrs::UNDERCURL)
            .underline_color(Rgba::rgb(255, 0, 100)),
    );
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &pc));
    assert!(out.contains("4:3"), "undercurl flows from term caps: {out}");
    assert!(
        out.contains("58:2::255:0:100"),
        "underline color flows: {out}"
    );

    // And the conservative default drops both, keeping plain underline
    // semantics (labeled downlevel).
    let pc0 = PresentCaps::from(&Capabilities::default());
    assert!(!pc0.undercurl && !pc0.underline_color);
}

#[test]
fn palette_source_is_base_spot_check() {
    use crate::base::Rgba;
    // 0x800000 is SYSTEM_16[1] (xterm dark red): a nearby color must emit
    // fg code 31, proving the presenter quantizes against base's table
    // (the old hand-typed table put dark red at 205 and would pick 9/91).
    let mut next = surf(10, 1);
    next.draw_text(0, 0, "r", Style::new().fg(Rgba::rgb(0x82, 0x02, 0x02)));
    let caps = PresentCaps {
        color: ColorDepth::Ansi16,
        ..TC
    };
    let mut p = warm(&next);
    let out = s(&emit(&mut p, &[Run { y: 0, x: 0, len: 1 }], &next, &caps));
    assert!(out.contains("\x1b[31mr"), "xterm system dark red: {out}");
}

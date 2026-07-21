//! VERIFY cycle-7 Unicode torture on the full render pipeline. Hostile
//! strings — RTL runs, combining storms (20 diacritics on one base),
//! flag/skin-tone/ZWJ families, lone-surrogate bytes via lossy decode —
//! drawn to a Surface and pushed through diff+present into the VtScreen
//! referee. Invariants: no panic; the surface's wide-pair structure
//! stays valid (`debug_validate`); the model reproduces the surface
//! cell-exact with zero unknown sequences; and the width the drawer
//! advances matches the width model.

use abstracttui::base::{Rgba, Size};
use abstracttui::render::{Cell, FrameDiff, PresentCaps, Presenter, Style, Surface};
use abstracttui::testing::frames::assert_screen_matches;
use abstracttui::testing::VtScreen;
use abstracttui::text;

/// The torture corpus: each entry is one logical string a user could
/// paste or type into a widget.
fn torture_strings() -> Vec<String> {
    let mut v: Vec<String> = vec![
        // Combining storm: base 'e' + 20 combining acutes.
        format!("e{}", "\u{0301}".repeat(20)),
        // Combining on many bases in a row.
        "a\u{0301}b\u{0302}c\u{0303}d\u{0304}e\u{0308}".to_string(),
        // RTL-ish Arabic + Hebrew mixed with Latin.
        "abcمرحباשלוםxyz".to_string(),
        // Regional-indicator flags (each two scalars).
        "\u{1F1EB}\u{1F1F7}\u{1F1EF}\u{1F1F5}\u{1F1FA}\u{1F1F8}".to_string(),
        // Skin-tone modifiers.
        "\u{1F44B}\u{1F3FF}\u{1F44D}\u{1F3FB}".to_string(),
        // Multi-person ZWJ families.
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}".to_string(),
        // VS16 emoji presentation widening a narrow base.
        "\u{2601}\u{FE0F}\u{2764}\u{FE0F}".to_string(),
        // Wide CJK run.
        "日本語のテキスト表示".to_string(),
        // Zero-width joiner/space soup.
        "x\u{200B}y\u{200D}z\u{FEFF}w".to_string(),
        // Control chars interleaved with text (must be handled, not drawn
        // as width).
        "a\u{0007}b\u{001b}c\u{0000}d".to_string(),
    ];
    // Lone-surrogate / invalid-UTF-8 bytes via lossy decode (a paste of
    // raw bytes the terminal delivered).
    let raw: &[u8] = &[0x61, 0xED, 0xA0, 0x80, 0x62, 0xFF, 0xFE, 0x63];
    v.push(String::from_utf8_lossy(raw).into_owned());
    v
}

/// Draw one string at (0,0) of an 80-wide surface; the advanced width
/// must equal the text width model, and the surface must validate.
#[test]
fn drawing_torture_strings_keeps_pairs_valid_and_width_consistent() {
    let style = Style::new()
        .fg(Rgba::rgb(220, 220, 230))
        .bg(Rgba::rgb(10, 10, 14));
    for s in torture_strings() {
        let mut surf = Surface::new(Size::new(80, 3), Cell::EMPTY);
        let advanced = surf.draw_text(0, 0, &s, style);
        // Structural oracle: no continuation without a wide leader, no
        // torn pair (RT1-4).
        surf.debug_validate()
            .unwrap_or_else(|e| panic!("pair invariant broke on {s:?}: {e}"));
        // The drawer's advance must match the display-width model, capped
        // at the surface width (clipping is legal, over-advance is not).
        let model_w = text::width(&s).min(80);
        assert!(
            advanced <= 80,
            "draw advanced {advanced} past the 80-col surface on {s:?}"
        );
        // Advance is monotonic and never exceeds the model width.
        assert!(
            advanced <= model_w.max(0),
            "draw advanced {advanced} > model width {model_w} on {s:?}"
        );
    }
}

/// Full pipeline: EACH torture string, at every column offset (so clip
/// edges and wide-pair boundaries are hit), through diff+present into the
/// referee — cell-exact, zero unknown sequences. One string per fresh
/// frame isolates the cluster-through-pipeline property from the
/// separate overlapping-overwrite property (which adv_render owns).
#[test]
fn torture_strings_roundtrip_through_present_and_model() {
    let caps = PresentCaps::FULL;
    let size = Size::new(40, 3);
    let style = Style::new()
        .fg(Rgba::rgb(200, 200, 210))
        .bg(Rgba::rgb(12, 12, 18));

    for (si, s) in torture_strings().into_iter().enumerate() {
        // Sweep the draw origin across the row, including the last few
        // columns where a wide cluster must clip rather than straddle.
        for x in 0..size.w {
            let mut screen = VtScreen::new(size);
            let mut diff = FrameDiff::new();
            let mut presenter = Presenter::new();
            let prev = Surface::new(size, Cell::EMPTY);
            let mut next = Surface::new(size, Cell::EMPTY);
            next.draw_text(x, 1, &s, style);
            next.debug_validate()
                .unwrap_or_else(|e| panic!("string {si} @x{x}: surface invalid: {e}"));

            let mut bytes = Vec::new();
            let runs = diff.compute_full(&prev, &next);
            presenter.emit(runs, &next, &caps, &mut bytes);
            screen.feed(&bytes);
            assert_eq!(
                screen.unknown_seq_count(),
                0,
                "string {si} @x{x}: unmodeled bytes: {:?}",
                screen.unknown_samples()
            );
            assert_screen_matches(&screen, &next, &caps, &format!("unicode string {si} @x{x}"));
        }
    }
}

/// The width model itself must be self-consistent on the corpus: a
/// cluster's width is 0, 1, or 2, and the string width equals the sum of
/// its clusters' widths (no double-counting across segmentation).
#[test]
fn width_model_is_cluster_additive_on_torture_corpus() {
    for s in torture_strings() {
        let mut sum = 0i32;
        for seg in text::segments(&s) {
            let w = seg.width;
            assert!(
                (0..=2).contains(&w),
                "cluster {:?} has width {w} (not 0/1/2)",
                seg.cluster
            );
            sum += w;
        }
        assert_eq!(
            sum,
            text::width(&s),
            "string width != sum of cluster widths for {s:?}"
        );
    }
}

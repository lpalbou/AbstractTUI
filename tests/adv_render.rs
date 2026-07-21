//! REDTEAM cycle-2 attack: RENDER's diff + presenter, judged by the VT
//! model. THE property (doctrine §1): bytes emitted for frame N, applied
//! to the model over frame N-1's screen, must reproduce frame N exactly —
//! cell content, colors, attributes, links — with zero unmodeled bytes.

use abstracttui::base::{Rgba, Size};
use abstracttui::render::{
    Attrs, Cell, ColorDepth, FrameDiff, PresentCaps, Presenter, Style, Surface,
};
use abstracttui::testing::frames::{assert_screen_matches, build_frame, random_ops, WORDS};
use abstracttui::testing::fuzzish::Rng;
use abstracttui::testing::{assert_snapshot, VtScreen};

// ---------------------------------------------------------------------------
// THE property test.
// ---------------------------------------------------------------------------

fn run_property_campaign(depth: ColorDepth, seeds: &[u64], frames_per_seed: usize) {
    let caps = PresentCaps {
        color: depth,
        ..PresentCaps::FULL
    };
    for &seed in seeds {
        let mut rng = Rng::new(seed);
        let size = Size::new(rng.range(20, 120) as i32, rng.range(6, 40) as i32);
        let mut screen = VtScreen::new(size);
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut bytes: Vec<u8> = Vec::new();

        let mut ops = random_ops(&mut rng, size, depth, true);
        let mut prev = Surface::new(size, Cell::EMPTY);
        // Seed the model with the presenter's view of the empty frame:
        // both start blank — nothing to do.
        for frame_no in 0..frames_per_seed {
            // Evolve: mutate 1..=3 ops, occasionally regenerate wholesale.
            if rng.chance(1, 10) {
                ops = random_ops(&mut rng, size, depth, true);
            } else {
                for _ in 0..rng.range(1, 3) {
                    let replacement = random_ops(&mut rng, size, depth, true);
                    let idx = rng.below(ops.len());
                    ops[idx] = replacement.into_iter().next().expect("nonempty");
                }
            }
            let next = build_frame(size, &ops);

            bytes.clear();
            let runs = diff.compute_full(&prev, &next);
            presenter.emit(runs, &next, &caps, &mut bytes);
            screen.feed(&bytes);

            let ctx = format!("seed {seed} frame {frame_no} ({depth:?})");
            assert_eq!(
                screen.unknown_seq_count(),
                0,
                "{ctx}: presenter emitted unmodeled bytes: {:?}\nbytes: {:?}",
                screen.unknown_samples(),
                String::from_utf8_lossy(&bytes)
            );
            assert_screen_matches(&screen, &next, &caps, &ctx);
            // Sync brackets balance every frame (RT1-16 family).
            assert_eq!(
                screen.counters().sync_begins,
                screen.counters().sync_ends,
                "{ctx}: unbalanced 2026 brackets"
            );
            prev = next;
        }
    }
}

#[test]
fn property_diff_present_truecolor_10_seeds_x_25_frames() {
    run_property_campaign(
        ColorDepth::TrueColor,
        &[1, 2, 3, 5, 8, 13, 21, 34, 55, 89],
        25,
    );
}

#[test]
fn property_diff_present_xterm256_palette_exact() {
    run_property_campaign(ColorDepth::Xterm256, &[101, 202, 303, 404], 15);
}

/// Final-audit spot check (cycle 9): the property judged on FRESH seeds
/// never used anywhere in the tree's history — an independent draw from
/// the input space, not a re-run of the pinned corpus. If the property
/// only held on the historical seeds by accident, this catches it.
#[test]
fn property_diff_present_final_audit_fresh_seeds() {
    run_property_campaign(ColorDepth::TrueColor, &[7777, 31337, 424242, 999983], 25);
    run_property_campaign(ColorDepth::Xterm256, &[600613, 271828], 15);
    run_property_campaign(ColorDepth::Ansi16, &[161803, 141421], 15);
}

#[test]
fn property_diff_present_ansi16_palette_exact() {
    run_property_campaign(ColorDepth::Ansi16, &[111, 222, 333], 15);
}

/// Baseline caps: no sync, no links, no undercurl — the degradation
/// paths must satisfy the same property.
#[test]
fn property_diff_present_baseline_caps() {
    let caps = PresentCaps::BASELINE;
    let mut rng = Rng::new(4242);
    let size = Size::new(40, 12);
    let mut screen = VtScreen::new(size);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    let mut prev = Surface::new(size, Cell::EMPTY);
    for frame_no in 0..20 {
        let ops = random_ops(&mut rng, size, ColorDepth::Ansi16, false);
        let next = build_frame(size, &ops);
        bytes.clear();
        let runs = diff.compute_full(&prev, &next);
        presenter.emit(runs, &next, &caps, &mut bytes);
        screen.feed(&bytes);
        assert_eq!(screen.unknown_seq_count(), 0, "frame {frame_no}");
        assert_screen_matches(&screen, &next, &caps, &format!("baseline frame {frame_no}"));
        assert_eq!(screen.counters().sync_begins, 0, "no 2026 when unsupported");
        prev = next;
    }
}

// ---------------------------------------------------------------------------
// Downlevel contrast property (their cycle-2 constraint).
// ---------------------------------------------------------------------------

/// Any fg/bg pair with contrast >= 2:1 must stay DISTINCT after
/// quantization, in both 256 and 16-color modes — verified through the
/// public pipeline (present -> VT model), not by calling internals.
#[test]
fn downlevel_preserves_distinct_pairs_with_contrast() {
    use abstracttui::theme::contrast_ratio;
    let mut rng = Rng::new(0xC0117A57);
    for depth in [ColorDepth::Xterm256, ColorDepth::Ansi16] {
        let caps = PresentCaps {
            color: depth,
            ..PresentCaps::BASELINE
        };
        let mut checked = 0;
        let mut attempts = 0;
        while checked < 400 && attempts < 20_000 {
            attempts += 1;
            let fg = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
            let bg = Rgba::rgb(rng.byte(), rng.byte(), rng.byte());
            if contrast_ratio(fg, bg) < 2.0 {
                continue;
            }
            checked += 1;
            let size = Size::new(4, 1);
            let mut surface = Surface::new(size, Cell::EMPTY);
            surface.draw_text(0, 0, "x", Style::new().fg(fg).bg(bg));
            let mut diff = FrameDiff::new();
            let mut presenter = Presenter::new();
            let mut bytes = Vec::new();
            let prev = Surface::new(size, Cell::EMPTY);
            presenter.emit(
                diff.compute_full(&prev, &surface),
                &surface,
                &caps,
                &mut bytes,
            );
            let mut screen = VtScreen::new(size);
            screen.feed(&bytes);
            let cell = screen.cell(0, 0).unwrap();
            let (qfg, qbg) = (cell.paint.fg, cell.paint.bg);
            assert!(
                qfg != qbg,
                "{depth:?}: contrast {:.2}:1 pair fg={} bg={} collapsed to {qfg:?}\nbytes {:?}",
                contrast_ratio(fg, bg),
                fg.to_hex(),
                bg.to_hex(),
                String::from_utf8_lossy(&bytes)
            );
        }
        assert!(
            checked >= 400,
            "premise starved: only {checked} contrast pairs"
        );
    }
}

// ---------------------------------------------------------------------------
// SGR economy: exact bytes for canonical transitions, pinned as goldens.
// ---------------------------------------------------------------------------

fn emit_two_cells(a: Style, b: Style, caps: &PresentCaps) -> Vec<u8> {
    let size = Size::new(4, 1);
    let mut surface = Surface::new(size, Cell::EMPTY);
    surface.draw_text(0, 0, "a", a);
    surface.draw_text(1, 0, "b", b);
    let prev = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut out = Vec::new();
    presenter.emit(diff.compute_full(&prev, &surface), &surface, caps, &mut out);
    out
}

fn printable_escape(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        match b {
            0x1b => s.push_str("<ESC>"),
            0x0d => s.push_str("<CR>"),
            b if (0x20..0x7f).contains(&b) => s.push(b as char),
            b => s.push_str(&format!("<{b:02x}>")),
        }
    }
    s
}

#[test]
fn sgr_economy_canonical_transitions_golden() {
    let caps = PresentCaps::FULL;
    let mut report = String::new();
    let cases: Vec<(&str, Style, Style)> = vec![
        (
            "same_style_no_repeat",
            Style::new().fg(Rgba::rgb(10, 20, 30)).attrs(Attrs::BOLD),
            Style::new().fg(Rgba::rgb(10, 20, 30)).attrs(Attrs::BOLD),
        ),
        (
            "bold_off_uses_22",
            Style::new().fg(Rgba::rgb(10, 20, 30)).attrs(Attrs::BOLD),
            Style::new().fg(Rgba::rgb(10, 20, 30)),
        ),
        (
            "many_attrs_off_uses_reset",
            Style::new()
                .fg(Rgba::rgb(10, 20, 30))
                .bg(Rgba::rgb(1, 2, 3))
                .attrs(Attrs::BOLD | Attrs::ITALIC | Attrs::UNDERLINE | Attrs::STRIKE),
            Style::new(),
        ),
        (
            "fg_change_only",
            Style::new().fg(Rgba::rgb(10, 20, 30)),
            Style::new().fg(Rgba::rgb(200, 20, 30)),
        ),
        (
            "undercurl_over_underline",
            Style::new().attrs(Attrs::UNDERLINE),
            Style::new().attrs(Attrs::UNDERCURL),
        ),
        (
            "dim_and_bold_shared_reset_readd",
            Style::new().attrs(Attrs::BOLD | Attrs::DIM),
            Style::new().attrs(Attrs::DIM),
        ),
    ];
    for (name, a, b) in cases {
        let bytes = emit_two_cells(a, b, &caps);
        report.push_str(&format!(
            "{name}: {} bytes\n  {}\n",
            bytes.len(),
            printable_escape(&bytes)
        ));
    }
    assert_snapshot("sgr_economy_transitions", &report);
}

#[test]
fn sgr_run_of_same_style_emits_one_sgr() {
    let caps = PresentCaps::FULL;
    let size = Size::new(30, 1);
    let mut surface = Surface::new(size, Cell::EMPTY);
    surface.draw_text(
        0,
        0,
        "same style all the way ok",
        Style::new().fg(Rgba::rgb(9, 9, 9)),
    );
    let prev = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut out = Vec::new();
    presenter.emit(
        diff.compute_full(&prev, &surface),
        &surface,
        &caps,
        &mut out,
    );
    // Count real SGR sequences (CSI ... final=='m'), not stray 'm' chars
    // in the text content.
    let mut sgr_seqs = 0;
    let mut i = 0;
    while i + 1 < out.len() {
        if out[i] == 0x1b && out[i + 1] == b'[' {
            let mut j = i + 2;
            while j < out.len() && !(0x40..=0x7e).contains(&out[j]) {
                j += 1;
            }
            if j < out.len() && out[j] == b'm' {
                sgr_seqs += 1;
            }
            i = j;
        }
        i += 1;
    }
    assert!(
        sgr_seqs <= 2,
        "one styled run must cost at most style+trailer SGRs, got {sgr_seqs} in {:?}",
        String::from_utf8_lossy(&out)
    );
}

// ---------------------------------------------------------------------------
// Wide-pair torture at blit clip edges.
// ---------------------------------------------------------------------------

/// Random blits of wide-glyph-rich surfaces at hostile clip offsets: the
/// destination must never hold a torn pair, and a subsequent present must
/// satisfy the property against the model.
#[test]
fn blit_clip_edges_never_tear_pairs() {
    let mut rng = Rng::new(0xB117);
    for round in 0..300 {
        let mut src = Surface::new(Size::new(12, 4), Cell::EMPTY);
        for y in 0..4 {
            src.draw_text(0, y, WORDS[rng.below(WORDS.len())], Style::new());
        }
        let mut dst = Surface::new(Size::new(10, 4), Cell::EMPTY);
        for y in 0..4 {
            dst.draw_text(0, y, "你好世界界", Style::new());
        }
        // Hostile: source rect straddles wide pairs; destination offset
        // lands mid-pair; both partially out of bounds.
        let src_rect = abstracttui::base::Rect::new(
            rng.below(12) as i32 - 1,
            rng.below(4) as i32,
            rng.range(1, 13) as i32,
            rng.range(1, 4) as i32,
        );
        let at = abstracttui::base::Point::new(rng.below(12) as i32 - 1, rng.below(5) as i32 - 1);
        dst.blit(&src, src_rect, at);

        // Invariant sweep: their own debug_validate PLUS an independent
        // walk (the referee does not outsource its verdict).
        if let Err(e) = dst.debug_validate() {
            panic!("round {round}: debug_validate failed after blit: {e}");
        }
        for y in 0..4 {
            for x in 0..10 {
                let cell = dst.get(x, y).unwrap();
                if cell.is_continuation() {
                    let leader_ok = x > 0
                        && dst
                            .get(x - 1, y)
                            .map(|c| c.is_wide_leader())
                            .unwrap_or(false);
                    assert!(leader_ok, "round {round}: orphan continuation at ({x},{y})");
                }
                if cell.is_wide_leader() {
                    let cont_ok = dst
                        .get(x + 1, y)
                        .map(|c| c.is_continuation())
                        .unwrap_or(false);
                    assert!(cont_ok, "round {round}: torn leader at ({x},{y})");
                }
            }
        }

        // And the blitted surface must still present correctly.
        let caps = PresentCaps::FULL;
        let prev = Surface::new(Size::new(10, 4), Cell::EMPTY);
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut bytes = Vec::new();
        presenter.emit(diff.compute_full(&prev, &dst), &dst, &caps, &mut bytes);
        let mut screen = VtScreen::new(Size::new(10, 4));
        screen.feed(&bytes);
        assert_eq!(screen.unknown_seq_count(), 0, "round {round}");
        assert_screen_matches(&screen, &dst, &caps, &format!("blit round {round}"));
    }
}

/// Pooled-glyph blits: ZWJ clusters spill to the per-surface pool; a blit
/// must adopt them into the destination pool, not leak foreign ids.
#[test]
fn blit_adopts_pooled_glyphs_across_surfaces() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466}";
    let mut src = Surface::new(Size::new(8, 1), Cell::EMPTY);
    src.draw_text(0, 0, family, Style::new());
    let mut dst = Surface::new(Size::new(8, 1), Cell::EMPTY);
    dst.blit(
        &src,
        abstracttui::base::Rect::new(0, 0, 8, 1),
        abstracttui::base::Point::ZERO,
    );
    let leader = *dst.get(0, 0).unwrap();
    assert_eq!(
        dst.glyph_str(&leader),
        family,
        "pooled cluster must resolve through the DESTINATION pool after blit"
    );
    // Present the destination and verify through the model.
    let caps = PresentCaps::FULL;
    let prev = Surface::new(Size::new(8, 1), Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    presenter.emit(diff.compute_full(&prev, &dst), &dst, &caps, &mut bytes);
    let mut screen = VtScreen::new(Size::new(8, 1));
    screen.feed(&bytes);
    assert_eq!(screen.cell(0, 0).unwrap().display(), family);
}

// ---------------------------------------------------------------------------
// external_write custody (damage contract §6).
// ---------------------------------------------------------------------------

/// Payload bytes appear exactly once, at the requested cursor position,
/// with SGR/link state closed BEFORE them, and the presenter re-syncs
/// afterward — the next frame still satisfies the property.
#[test]
fn external_write_custody_and_invalidation() {
    let size = Size::new(20, 5);
    let caps = PresentCaps::FULL;
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut screen = VtScreen::new(size);
    let mut bytes = Vec::new();

    // Frame 1: linked, styled content (open SGR/link state to flush).
    let mut f1 = Surface::new(size, Cell::EMPTY);
    let link = f1.register_link("https://x.example");
    f1.draw_text(
        0,
        0,
        "linked",
        Style::new().fg(Rgba::rgb(9, 9, 9)).link(link),
    );
    let prev = Surface::new(size, Cell::EMPTY);
    presenter.emit(diff.compute_full(&prev, &f1), &f1, &caps, &mut bytes);

    // External payload (kitty-shaped APC blob) at (3, 2).
    let payload = b"\x1b_Gi=7,a=T,f=24;AAAA\x1b\\";
    let before = bytes.len();
    presenter.external_write(&mut bytes, payload, abstracttui::base::Point::new(3, 2));
    let appended = &bytes[before..];
    // Payload present exactly once, verbatim.
    let hay = appended
        .windows(payload.len())
        .filter(|w| *w == payload.as_slice())
        .count();
    assert_eq!(
        hay, 1,
        "payload must appear exactly once in the appended bytes"
    );

    screen.feed(&bytes);
    // The model consumed the APC as ONE string frame — no unknown dirt,
    // no link/SGR leaking into it, cursor parked at the requested spot.
    assert_eq!(screen.unknown_seq_count(), 0);
    assert_eq!(screen.counters().string_frames, 1);
    assert_eq!(screen.cursor(), abstracttui::base::Point::new(3, 2));
    assert_eq!(
        screen.current_paint(),
        abstracttui::testing::Paint::default(),
        "SGR + link must be closed before external bytes"
    );

    // Frame 2 after invalidation: the property must still hold.
    let mut f2 = Surface::new(size, Cell::EMPTY);
    f2.draw_text(2, 3, "after", Style::new().bg(Rgba::rgb(3, 4, 5)));
    bytes.clear();
    presenter.emit(diff.compute_full(&f1, &f2), &f2, &caps, &mut bytes);
    // Invalidated: the first motion must be ABSOLUTE (CUP), never a
    // relative move computed from a stale virtual cursor.
    let text = String::from_utf8_lossy(&bytes);
    let after_sync = text.trim_start_matches("\u{1b}[?2026h");
    assert!(
        after_sync.starts_with("\u{1b}["),
        "post-external frame must start with an absolute re-sync: {text:?}"
    );
    screen.feed(&bytes);
    assert_screen_matches(&screen, &f2, &caps, "post-external frame");
}

// ---------------------------------------------------------------------------
// Risky-cluster cursor invalidation (RT1-7 fix, landed this cycle).
// ---------------------------------------------------------------------------

/// After emitting a ZWJ family, the presenter must not trust its virtual
/// cursor: the NEXT glyph in the same run must be positioned absolutely.
#[test]
fn risky_cluster_forces_absolute_cup_before_next_glyph() {
    let size = Size::new(20, 2);
    let caps = PresentCaps::FULL;
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466}";
    let mut surface = Surface::new(size, Cell::EMPTY);
    surface.draw_text(0, 0, family, Style::new());
    surface.draw_text(2, 0, "xy", Style::new());
    let prev = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    presenter.emit(
        diff.compute_full(&prev, &surface),
        &surface,
        &caps,
        &mut bytes,
    );

    // Between the family bytes and the 'x' there must be an absolute CUP
    // (ESC [ 1 ; 3 H) — the defensive re-sync.
    let fam = family.as_bytes();
    let fam_pos = bytes
        .windows(fam.len())
        .position(|w| w == fam)
        .expect("family emitted");
    let x_pos = bytes[fam_pos..]
        .iter()
        .position(|&b| b == b'x')
        .expect("x emitted")
        + fam_pos;
    let between = &bytes[fam_pos + fam.len()..x_pos];
    assert!(
        between.windows(2).any(|w| w == b"\x1b[") && between.ends_with(b"H"),
        "no absolute CUP between a risky cluster and the next glyph: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    // And the result still matches the model (shared convention).
    let mut screen = VtScreen::new(size);
    screen.feed(&bytes);
    assert_screen_matches(&screen, &surface, &caps, "risky cluster frame");
}

// ---------------------------------------------------------------------------
// Mosaic bridge (Surface::blit_mosaic, landed this cycle).
// ---------------------------------------------------------------------------

#[test]
fn blit_mosaic_patches_present_correctly() {
    use abstracttui::gfx::{mosaic, Bitmap, MosaicMode};
    // A 4x4 bitmap -> 4x2 half-block cells.
    let img = Bitmap::from_fn(4, 4, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba::rgb(255, 0, 0)
        } else {
            Rgba::rgb(0, 0, 255)
        }
    });
    let grid = mosaic::render(&img, 4, 2, MosaicMode::HalfBlock);
    let size = Size::new(10, 4);
    let mut surface = Surface::new(size, Cell::EMPTY);
    surface.blit_mosaic(
        grid.cell_patches(abstracttui::base::Point::ZERO),
        abstracttui::base::Point::new(1, 1),
    );
    surface
        .debug_validate()
        .expect("mosaic blit keeps invariants");

    let caps = PresentCaps::FULL;
    let prev = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    presenter.emit(
        diff.compute_full(&prev, &surface),
        &surface,
        &caps,
        &mut bytes,
    );
    let mut screen = VtScreen::new(size);
    screen.feed(&bytes);
    assert_eq!(screen.unknown_seq_count(), 0);
    // Half-block glyph with the checker colors landed at the offset.
    let cell = screen.cell(1, 1).expect("in bounds");
    assert_eq!(cell.ch(), '\u{2580}', "half-block leader expected");
    assert_eq!(cell.paint.fg, Some(Rgba::rgb(255, 0, 0)));
    assert_eq!(cell.paint.bg, Some(Rgba::rgb(0, 0, 255)));
}

// ---------------------------------------------------------------------------
// Last-column discipline, verified through the model's wrap tracking.
// ---------------------------------------------------------------------------

/// Writing the full last column must leave the model without a pending
/// wrap ever being CONSUMED: after the frame, no scroll may have happened
/// (the screen's rows still hold exactly the intended content).
#[test]
fn full_screen_write_never_scrolls_the_model() {
    let size = Size::new(10, 4);
    let mut surface = Surface::new(size, Cell::EMPTY);
    for y in 0..4 {
        surface.draw_text(0, y, "0123456789", Style::new());
    }
    let caps = PresentCaps::FULL;
    let prev = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    presenter.emit(
        diff.compute_full(&prev, &surface),
        &surface,
        &caps,
        &mut bytes,
    );
    let mut screen = VtScreen::new(size);
    screen.feed(&bytes);
    // Bottom-right written, nothing scrolled: row 0 intact.
    assert_eq!(
        screen.to_text(),
        "0123456789\n0123456789\n0123456789\n0123456789\n"
    );
    assert_eq!(
        screen.cell(9, 3).unwrap().ch(),
        '9',
        "bottom-right must be written"
    );
    // Trailer parks at bottom-left, never bottom-right (wrap hazard).
    assert_eq!(screen.cursor(), abstracttui::base::Point::new(0, 3));
}

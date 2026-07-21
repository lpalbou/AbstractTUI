//! Scroll detection tests: the decomposition property (shift + runs
//! reconstruct `next` exactly, cell-wise — no VT model needed) plus
//! detection guards and the presenter's scroll bytes.

use super::*;
use crate::base::{Point, Rgba, Size};
use crate::render::present::{PresentCaps, Presenter};
use crate::render::style::Style;

fn surf(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

/// Deterministic pseudo-random row content (seeded; includes CJK and
/// styled spans; inline-only glyphs so the oracle predicate needs no
/// cross-pool bookkeeping — pooled adoption is covered elsewhere).
/// Content depends on the SEED alone, never the row position — a
/// "scrolled" frame reuses seeds at new rows and must reproduce the
/// content byte-identically there.
fn fill_row(s: &mut Surface, y: i32, seed: u64) {
    let mut v = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x1234_5678_9ABC_DEF0;
    let mut next = || {
        v ^= v >> 33;
        v = v.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        v ^= v >> 29;
        v
    };
    let words = ["log", "警告", "ok", "ready", "worker", "err!"];
    let mut x = 0;
    while x < s.width() {
        let w = words[(next() % words.len() as u64) as usize];
        let color = Rgba::rgb((next() % 256) as u8, 128, (next() % 256) as u8);
        x += s.draw_text(x, y, w, Style::new().fg(color)).max(1);
    }
}

fn frame_of_rows(w: i32, h: i32, rows: &[u64]) -> Surface {
    let mut s = surf(w, h);
    for (y, &seed) in rows.iter().enumerate() {
        fill_row(&mut s, y as i32, seed);
    }
    s
}

/// THE decomposition oracle: terminal state after (shift over prev) then
/// (runs copied from next) must equal next everywhere.
fn assert_decomposition(prev: &Surface, next: &Surface, shift: Option<Shift>, runs: &[Run]) {
    let covered = |x: i32, y: i32| runs.iter().any(|r| r.y == y && x >= r.x && x < r.end());
    for y in 0..next.height() {
        for x in 0..next.width() {
            if covered(x, y) {
                continue; // runs copy next verbatim: correct by construction
            }
            let b = next.get(x, y).unwrap();
            match shift.map(|s| shift_source(&s, y)).unwrap_or(Src::Row(y)) {
                Src::Row(sy) => {
                    let a = prev.get(x, sy).unwrap();
                    assert!(
                        cells_equal(prev, next, a, b),
                        "uncovered change at ({x},{y}) vs prev row {sy}"
                    );
                }
                Src::Erased => {
                    assert!(
                        cells_equal(prev, next, &Cell::EMPTY, b),
                        "entering row cell ({x},{y}) not erased-equivalent and not covered"
                    );
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
enum Src {
    Row(i32),
    Erased,
}

/// Test-side mirror of the shift row mapping (kept independent so a bug
/// in the production mapping cannot hide itself).
fn shift_source(s: &Shift, y: i32) -> Src {
    if y < s.top || y >= s.bottom {
        return Src::Row(y);
    }
    let sy = if s.up { y + s.n } else { y - s.n };
    if sy >= s.top && sy < s.bottom {
        Src::Row(sy)
    } else {
        Src::Erased
    }
}

fn full(s: &Surface) -> Vec<crate::base::Rect> {
    vec![s.bounds()]
}

#[test]
fn detects_clean_scroll_up_and_decomposes() {
    let rows: Vec<u64> = (0..24).map(|i| 100 + i).collect();
    let prev = frame_of_rows(80, 24, &rows);
    // Scroll up by 2: rows 2.. move to 0..; two new rows enter at bottom.
    let mut next_rows = rows[2..].to_vec();
    next_rows.push(900);
    next_rows.push(901);
    let next = frame_of_rows(80, 24, &next_rows);

    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    let shift = shift.expect("clean scroll must be detected");
    assert_eq!((shift.n, shift.up), (2, true));
    assert_eq!((shift.top, shift.bottom), (0, 24));
    // Residual runs live only in the entering rows.
    assert!(
        runs.iter().all(|r| r.y >= 22),
        "only entering rows repaint: {runs:?}"
    );
    assert_decomposition(&prev, &next, Some(shift), &runs);
}

#[test]
fn detects_scroll_down() {
    let rows: Vec<u64> = (0..20).map(|i| 7 * i + 1).collect();
    let prev = frame_of_rows(60, 20, &rows);
    let mut next_rows = vec![555, 556, 557];
    next_rows.extend_from_slice(&rows[..17]);
    let next = frame_of_rows(60, 20, &next_rows);

    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    let shift = shift.expect("scroll down detected");
    assert_eq!((shift.n, shift.up), (3, false));
    assert!(
        runs.iter().all(|r| r.y < 3),
        "entering rows at the top: {runs:?}"
    );
    assert_decomposition(&prev, &next, Some(shift), &runs);
}

#[test]
fn scroll_with_mutations_still_decomposes() {
    // The log scrolled AND someone edited a line mid-band: the shift is
    // still chosen (enough saved rows) and runs cover the mutation.
    let rows: Vec<u64> = (0..24).map(|i| 40 + i).collect();
    let prev = frame_of_rows(80, 24, &rows);
    let mut next_rows = rows[1..].to_vec();
    next_rows.push(999);
    let mut next = frame_of_rows(80, 24, &next_rows);
    next.draw_text(10, 12, "EDITED", Style::new().fg(Rgba::rgb(255, 0, 0)));

    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    assert!(
        shift.is_some(),
        "one mutated row must not kill a 23-row win"
    );
    assert!(
        runs.iter().any(|r| r.y == 12),
        "the edit is covered by runs"
    );
    assert_decomposition(&prev, &next, shift, &runs);
}

#[test]
fn randomized_decomposition_property() {
    // Seeded random scrolls with random mutations; whatever detection
    // decides, the decomposition must hold cell-wise.
    let mut rng = 0xC0FFEEu64;
    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for round in 0..30 {
        let h = 12 + (rand() % 20) as i32;
        let w = 20 + (rand() % 60) as i32;
        let rows: Vec<u64> = (0..h as u64).map(|i| rand().wrapping_add(i)).collect();
        let prev = frame_of_rows(w, h, &rows);
        let n = 1 + (rand() % 5) as usize;
        let up = rand() % 2 == 0;
        let mut next_rows = if up && n < rows.len() {
            let mut v = rows[n..].to_vec();
            v.extend((0..n).map(|_| rand()));
            v
        } else if n < rows.len() {
            let mut v: Vec<u64> = (0..n).map(|_| rand()).collect();
            v.extend_from_slice(&rows[..rows.len() - n]);
            v
        } else {
            rows.clone()
        };
        // Random mutations.
        for _ in 0..(rand() % 3) {
            let idx = (rand() % next_rows.len() as u64) as usize;
            next_rows[idx] = rand();
        }
        let next = frame_of_rows(w, h, &next_rows);

        let mut diff = FrameDiff::new();
        let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
        let (shift, runs) = (sf.shift(), sf.runs().to_vec());
        assert_decomposition(&prev, &next, shift, &runs);
        let _ = round;
    }
}

#[test]
fn small_bands_and_partial_width_damage_fall_back() {
    let rows: Vec<u64> = (0..6).map(|i| i + 1).collect();
    let prev = frame_of_rows(40, 6, &rows);
    let mut next_rows = rows[1..].to_vec();
    next_rows.push(77);
    let next = frame_of_rows(40, 6, &next_rows);
    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    assert!(
        sf.shift().is_none(),
        "6-row band is under the byte-win floor"
    );

    // Partial-width damage never scrolls (DECSTBM is full-width only).
    let prev = frame_of_rows(40, 24, &(0..24).collect::<Vec<u64>>());
    let mut nr: Vec<u64> = (1..24).collect();
    nr.push(50);
    let next = frame_of_rows(40, 24, &nr);
    let sf = diff.compute_scrolled(&prev, &next, &[crate::base::Rect::new(1, 0, 38, 24)]);
    assert!(
        sf.shift().is_none(),
        "partial-width damage cannot use DECSTBM"
    );
}

#[test]
fn unrelated_changes_do_not_fake_a_scroll() {
    // Full repaint with unrelated content: no shift claimed.
    let prev = frame_of_rows(60, 20, &(0..20).collect::<Vec<u64>>());
    let next = frame_of_rows(60, 20, &(100..120).collect::<Vec<u64>>());
    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    assert!(shift.is_none());
    assert_decomposition(&prev, &next, None, &runs);
}

#[test]
fn plain_path_unchanged_when_no_shift() {
    // (None, runs) must equal compute()'s output bit-for-bit.
    let prev = frame_of_rows(60, 20, &(0..20).collect::<Vec<u64>>());
    let mut next = frame_of_rows(60, 20, &(0..20).collect::<Vec<u64>>());
    next.draw_text(5, 5, "delta", Style::new());
    let mut d1 = FrameDiff::new();
    let plain = d1.compute(&prev, &next, &full(&prev)).to_vec();
    let mut d2 = FrameDiff::new();
    let sf = d2.compute_scrolled(&prev, &next, &full(&prev));
    assert!(sf.shift().is_none());
    assert_eq!(sf.runs(), &plain[..]);
}

#[test]
fn presenter_scroll_bytes_are_exact_and_cursor_resyncs() {
    let rows: Vec<u64> = (0..24).map(|i| 100 + i).collect();
    let prev = frame_of_rows(80, 24, &rows);
    let mut next_rows = rows[1..].to_vec();
    next_rows.push(900);
    let next = frame_of_rows(80, 24, &next_rows);
    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    let shift = shift.unwrap();

    let mut p = Presenter::new();
    assert!(p.opts().scroll_optimization, "default is ON since cycle 5");
    // Warm presenter state (park + defaults).
    let mut out = Vec::new();
    p.emit(
        &[Run { y: 0, x: 0, len: 1 }],
        &prev,
        &PresentCaps::FULL,
        &mut out,
    );
    out.clear();
    p.emit_scrolled(
        ScrolledRuns {
            shift: Some(shift),
            runs: &runs,
        },
        &next,
        &PresentCaps::FULL,
        &mut out,
    );
    let text = String::from_utf8_lossy(&out);
    // Prelude: sync open, SGR reset, DECSTBM 1..24, SU 1, region reset.
    assert!(
        text.starts_with("\x1b[?2026h\x1b[0m\x1b[1;24r\x1b[S\x1b[r"),
        "scroll prelude bytes: {text:?}"
    );
    assert!(text.ends_with("\x1b[?2026l"));

    // Byte-win report (RT §2.7 numbers through the real path).
    let mut plain_diff = FrameDiff::new();
    let plain_runs = plain_diff.compute(&prev, &next, &full(&prev)).to_vec();
    let mut p2 = Presenter::new();
    let mut plain_out = Vec::new();
    p2.emit(
        &[Run { y: 0, x: 0, len: 1 }],
        &prev,
        &PresentCaps::FULL,
        &mut plain_out,
    );
    plain_out.clear();
    p2.emit(&plain_runs, &next, &PresentCaps::FULL, &mut plain_out);
    eprintln!(
        "SCROLL-BYTES: scroll-path={} plain-repaint={} ratio={:.1}x",
        out.len(),
        plain_out.len(),
        plain_out.len() as f64 / out.len() as f64
    );
    assert!(
        out.len() * 4 < plain_out.len(),
        "scroll path must win by >4x on a clean 24-row scroll: {} vs {}",
        out.len(),
        plain_out.len()
    );
}

// -- cycle 5: referee-verified (VtScreen models DECSTBM) ---------------------

/// THE byte-level property with the optimization engaged: emitted bytes
/// applied to the DECSTBM-aware VT model must reproduce `next` exactly,
/// across randomized scroll+mutation sequences. This is the in-module
/// twin of REDTEAM's adv_scroll integration suite.
#[test]
fn vtscreen_replays_scrolled_frames_exactly() {
    use crate::testing::frames::assert_screen_matches;
    use crate::testing::VtScreen;
    let caps = PresentCaps::FULL;
    let mut rng = 0xBEEFu64;
    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for round in 0..12 {
        let w = 24 + (rand() % 56) as i32;
        let h = 10 + (rand() % 18) as i32;
        let size = Size::new(w, h);
        let mut screen = VtScreen::new(size);
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut bytes = Vec::new();
        let mut prev = surf(w, h);
        let mut rows: Vec<u64> = (0..h as u64).map(|i| rand().wrapping_add(i)).collect();
        // First paint, then a sequence of scrolls with occasional edits.
        for (y, &seed) in rows.iter().enumerate() {
            fill_row(&mut prev, y as i32, seed);
        }
        let first = diff.compute_scrolled(&Surface::new(size, Cell::EMPTY), &prev, &full(&prev));
        presenter.emit_scrolled(first, &prev, &caps, &mut bytes);
        screen.feed(&bytes);
        assert_screen_matches(&screen, &prev, &caps, &format!("round {round} first paint"));

        for step in 0..6 {
            let n = (1 + (rand() % 3) as usize).min(rows.len());
            let up = rand() % 2 == 0;
            if up {
                rows.rotate_left(n);
                let len = rows.len();
                for r in &mut rows[len - n..] {
                    *r = rand();
                }
            } else {
                rows.rotate_right(n);
                for r in &mut rows[..n] {
                    *r = rand();
                }
            }
            if rand() % 3 == 0 {
                let idx = (rand() % rows.len() as u64) as usize;
                rows[idx] = rand(); // in-band edit against the scroll
            }
            let mut next = surf(w, h);
            for (y, &seed) in rows.iter().enumerate() {
                fill_row(&mut next, y as i32, seed);
            }
            bytes.clear();
            let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
            presenter.emit_scrolled(sf, &next, &caps, &mut bytes);
            screen.feed(&bytes);
            assert_eq!(
                screen.unknown_seq_count(),
                0,
                "round {round} step {step}: unmodeled bytes: {:?}",
                screen.unknown_samples()
            );
            assert_screen_matches(&screen, &next, &caps, &format!("round {round} step {step}"));
            prev.blit(&next, next.bounds(), Point::ZERO);
        }
    }
}

/// Cycle-7 audit: wide pairs at BAND EDGES under the scroll optimization.
/// The hazards this pins: (a) unchanged chrome rows full of wide glyphs
/// trimmed off the band must not confuse detection; (b) a mid-band edit
/// that clobbers HALF a wide pair on an otherwise-shifted row must
/// repaint leader+continuation coherently (run extension); (c) entering
/// rows made of wide glyphs, including the degraded-last-column case,
/// must replay exactly through the DECSTBM path.
#[test]
fn wide_pairs_at_band_edges_survive_scroll_optimization() {
    use crate::testing::frames::assert_screen_matches;
    use crate::testing::VtScreen;
    let caps = PresentCaps::FULL;
    let size = Size::new(21, 10); // odd width: last column degrades a wide glyph
    let cjk = Style::new()
        .fg(Rgba::rgb(230, 200, 90))
        .bg(Rgba::rgb(10, 12, 20));

    let cjk_row = |s: &mut Surface, y: i32| {
        // "漢字漢字漢字漢字漢字" is 20 cols; column 20 takes a degraded
        // wide glyph (drawn at the edge -> replacement/blank per policy).
        s.draw_text(0, y, "漢字漢字漢字漢字漢字", cjk);
        s.draw_text(20, y, "宽", cjk);
    };

    let mut prev = surf(size.w, size.h);
    cjk_row(&mut prev, 0); // top chrome (unchanged): trimmed off the band
    for y in 1..9 {
        fill_row(&mut prev, y, 400 + y as u64);
    }
    cjk_row(&mut prev, 9); // bottom chrome, wide at both edges

    // Scroll the 1..9 band up by 2; entering rows are wide-glyph rows;
    // one surviving row gets HALF a pair clobbered (narrow over the
    // continuation column of 漢 at x=0..2 -> writes at x=1).
    let mut next = surf(size.w, size.h);
    cjk_row(&mut next, 0);
    for y in 1..7 {
        fill_row(&mut next, y, 400 + (y + 2) as u64);
    }
    cjk_row(&mut next, 7);
    cjk_row(&mut next, 8);
    next.draw_text(1, 3, "!", cjk); // half-pair clobber inside the band
    cjk_row(&mut next, 9);

    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    // The optimization must ENGAGE (a fallback to plain runs would pass
    // replay trivially and test nothing).
    let shift = shift.expect("banded scroll with wide chrome must be detected");
    assert_eq!((shift.n, shift.up), (2, true));
    assert!(
        shift.top >= 1 && shift.bottom <= 9,
        "chrome trimmed: {shift:?}"
    );
    assert_decomposition(&prev, &next, Some(shift), &runs);

    // Byte-level: the DECSTBM emission replayed on the referee model
    // reproduces `next` exactly — wide pairs, degraded edge and all.
    let mut screen = VtScreen::new(size);
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    let first = diff.compute_scrolled(&Surface::new(size, Cell::EMPTY), &prev, &full(&prev));
    presenter.emit_scrolled(first, &prev, &caps, &mut bytes);
    screen.feed(&bytes);
    assert_screen_matches(&screen, &prev, &caps, "wide chrome first paint");

    bytes.clear();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    presenter.emit_scrolled(sf, &next, &caps, &mut bytes);
    screen.feed(&bytes);
    assert_eq!(screen.unknown_seq_count(), 0);
    assert_screen_matches(&screen, &next, &caps, "wide pairs across the scrolled band");
}

/// REDTEAM's published workloads through the PAIRED path: property holds
/// and the bytes/frame land far under their plain-path baselines
/// (cycle-5 filing: log-append 2,318 B/frame, list-scroll 1,607, banded
/// 1,648).
#[test]
fn redteam_workload_bytes_with_optimization_on() {
    use crate::testing::frames::{
        assert_screen_matches, banded_list_frame, list_frame, log_append_frame,
    };
    use crate::testing::VtScreen;
    let style = Style::new()
        .fg(Rgba::rgb(190, 190, 200))
        .bg(Rgba::rgb(14, 16, 24));
    let run = |size: Size, frames: &[Surface], ctx: &str, baseline: usize| -> usize {
        let caps = PresentCaps::FULL;
        let mut screen = VtScreen::new(size);
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut bytes = Vec::new();
        let mut prev = Surface::new(size, Cell::EMPTY);
        let mut total = 0usize;
        for (i, next) in frames.iter().enumerate() {
            bytes.clear();
            let sf = diff.compute_scrolled(&prev, next, &[next.bounds()]);
            presenter.emit_scrolled(sf, next, &caps, &mut bytes);
            total += bytes.len();
            screen.feed(&bytes);
            assert_eq!(
                screen.unknown_seq_count(),
                0,
                "{ctx} frame {i}: unmodeled bytes"
            );
            assert_screen_matches(&screen, next, &caps, &format!("{ctx} frame {i}"));
            prev.blit(next, next.bounds(), Point::ZERO);
        }
        let per_frame = total / frames.len();
        eprintln!(
            "SCROLL-OPT {ctx}: {per_frame} B/frame optimized vs {baseline} baseline ({:.1}x)",
            baseline as f64 / per_frame as f64
        );
        per_frame
    };

    let size = Size::new(90, 28);
    let frames: Vec<Surface> = (1..=30)
        .map(|n| log_append_frame(size, n, 2, style))
        .collect();
    let log_pf = run(size, &frames, "log-append", 2318);

    let size = Size::new(70, 20);
    let mut frames = Vec::new();
    for offset in 0..25usize {
        frames.push(list_frame(size, offset, style));
    }
    for offset in (5..25usize).rev() {
        frames.push(list_frame(size, offset, style));
    }
    let list_pf = run(size, &frames, "list-scroll", 1607);

    let size = Size::new(70, 22);
    let frames: Vec<Surface> = (0..30usize)
        .map(|off| banded_list_frame(size, off, style))
        .collect();
    let banded_pf = run(size, &frames, "banded-scroll", 1648);

    // The win must be real on the scroll-dominated shapes. (First paints
    // are amortized into the average, so thresholds are conservative.)
    assert!(
        log_pf * 2 < 2318,
        "log-append must at least halve: {log_pf}"
    );
    assert!(
        list_pf * 2 < 1607,
        "list-scroll must at least halve: {list_pf}"
    );
    assert!(banded_pf < 1648, "banded must not regress: {banded_pf}");
}

#[test]
fn pure_scroll_with_no_runs_still_emits() {
    // All entering rows are EMPTY -> zero residual runs; the frame is the
    // scroll alone.
    let mut prev = surf(80, 24);
    for y in 0..24 {
        fill_row(&mut prev, y, y as u64 + 1);
    }
    let mut next = surf(80, 24);
    for y in 0..23 {
        fill_row(&mut next, y, y as u64 + 2); // rows shifted up by 1
    }
    // Row 23 stays EMPTY (erased-equivalent).
    let mut diff = FrameDiff::new();
    let sf = diff.compute_scrolled(&prev, &next, &full(&prev));
    let (shift, runs) = (sf.shift(), sf.runs().to_vec());
    let shift = shift.expect("clean shift");
    assert!(
        runs.is_empty(),
        "erased-equivalent entering row needs no paint: {runs:?}"
    );
    let mut p = Presenter::new();
    let mut out = Vec::new();
    p.emit_scrolled(
        ScrolledRuns {
            shift: Some(shift),
            runs: &runs,
        },
        &next,
        &PresentCaps::BASELINE,
        &mut out,
    );
    assert!(!out.is_empty(), "a pure scroll is a real frame");
    assert_decomposition(&prev, &next, Some(shift), &runs);
}

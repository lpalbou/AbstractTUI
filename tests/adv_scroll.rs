//! REDTEAM cycle-4 attack: scroll-shaped present workloads. The
//! diff/present property must hold for log-append / list-scroll /
//! banded-scroll sequences REGARDLESS of whether RENDER's scroll-region
//! optimization is engaged, and the emitted byte counts are published
//! as the optimization's before/after metric.

use abstracttui::base::{Rect, Rgba, Size};
use abstracttui::render::{Cell, FrameDiff, PresentCaps, Presenter, Style, Surface};
use abstracttui::testing::frames::{
    assert_screen_matches, banded_list_frame, list_frame, log_append_frame,
};
use abstracttui::testing::{Rng, VtScreen};

fn style() -> Style {
    Style::new()
        .fg(Rgba::rgb(190, 190, 200))
        .bg(Rgba::rgb(14, 16, 24))
}

/// Same property drive as [`run_sequence`], but through RENDER's
/// scroll-region path: `compute_scrolled` + `emit_scrolled`. The referee
/// VtScreen executes the emitted DECSTBM/SU/SD for real, so a wrong
/// shift, wrong margins, or shift-relative runs against the wrong base
/// all land as cell mismatches. Returns (total bytes, shifted frames).
fn run_sequence_scrolled(size: Size, frames: &[Surface], ctx: &str) -> (usize, usize) {
    run_sequence_scrolled_damage(size, frames, None, ctx)
}

/// Scrolled drive with an explicit per-frame damage rect (None = full
/// frame). Tight damage is what the reactive layer provides in real use;
/// the detector's anchors live at the union's edge rows, so this is the
/// lever that decides whether banded scrolls engage.
fn run_sequence_scrolled_damage(
    size: Size,
    frames: &[Surface],
    damage: Option<Rect>,
    ctx: &str,
) -> (usize, usize) {
    let caps = PresentCaps::FULL;
    let mut screen = VtScreen::new(size);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    let mut prev = Surface::new(size, Cell::EMPTY);
    let mut total = 0usize;
    let mut shifted = 0usize;
    for (i, next) in frames.iter().enumerate() {
        bytes.clear();
        // First paint is always full damage (matches real damage
        // tracking); the tight rect applies from frame 1 on.
        let rect = if i == 0 {
            Rect::from_size(size)
        } else {
            damage.unwrap_or(Rect::from_size(size))
        };
        let sr = diff.compute_scrolled(&prev, next, &[rect]);
        if sr.shift().is_some() {
            shifted += 1;
        }
        presenter.emit_scrolled(sr, next, &caps, &mut bytes);
        total += bytes.len();
        screen.feed(&bytes);
        assert_eq!(
            screen.unknown_seq_count(),
            0,
            "{ctx} frame {i}: unmodeled bytes: {:?}",
            screen.unknown_samples()
        );
        // Scroll custody: margins must never leak past the frame.
        assert_eq!(
            screen.margins(),
            None,
            "{ctx} frame {i}: DECSTBM left set after frame"
        );
        assert_screen_matches(&screen, next, &caps, &format!("{ctx} frame {i} (scrolled)"));
        prev.blit(next, next.bounds(), abstracttui::base::Point::ZERO);
    }
    (total, shifted)
}

/// Drive a frame sequence through diff+present+model; return total bytes.
fn run_sequence(size: Size, frames: &[Surface], ctx: &str) -> usize {
    let caps = PresentCaps::FULL;
    let mut screen = VtScreen::new(size);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes = Vec::new();
    let mut prev = Surface::new(size, Cell::EMPTY);
    let mut total = 0usize;
    for (i, next) in frames.iter().enumerate() {
        bytes.clear();
        let runs = diff.compute_full(&prev, next);
        presenter.emit(runs, next, &caps, &mut bytes);
        total += bytes.len();
        screen.feed(&bytes);
        assert_eq!(
            screen.unknown_seq_count(),
            0,
            "{ctx} frame {i}: unmodeled bytes: {:?}",
            screen.unknown_samples()
        );
        assert_screen_matches(&screen, next, &caps, &format!("{ctx} frame {i}"));
        prev.blit(next, next.bounds(), abstracttui::base::Point::ZERO);
    }
    total
}

#[test]
fn log_append_sequence_property_and_bytes() {
    let size = Size::new(90, 28);
    let frames: Vec<Surface> = (1..=30)
        .map(|n| log_append_frame(size, n, 2, style()))
        .collect();
    let total = run_sequence(size, &frames, "log-append");
    eprintln!(
        "scroll metric: log-append 30 frames x 2 lines = {total} bytes ({} / frame)",
        total / 30
    );
}

#[test]
fn list_scroll_down_and_up_property_and_bytes() {
    let size = Size::new(70, 20);
    let mut frames = Vec::new();
    for offset in 0..25usize {
        frames.push(list_frame(size, offset, style()));
    }
    for offset in (5..25usize).rev() {
        frames.push(list_frame(size, offset, style()));
    }
    let n = frames.len();
    let total = run_sequence(size, &frames, "list-scroll");
    eprintln!(
        "scroll metric: list-scroll {n} frames = {total} bytes ({} / frame)",
        total / n
    );
}

#[test]
fn banded_scroll_with_fixed_chrome_property_and_bytes() {
    let size = Size::new(70, 22);
    let frames: Vec<Surface> = (0..30usize)
        .map(|off| banded_list_frame(size, off, style()))
        .collect();
    let total = run_sequence(size, &frames, "banded-scroll");
    eprintln!(
        "scroll metric: banded-scroll 30 frames = {total} bytes ({} / frame)",
        total / 30
    );
    // Chrome rows must be emitted at most once (they never change): the
    // steady-state per-frame cost must be well under a full-frame paint.
    // (A regression that repaints chrome every frame roughly doubles the
    // per-frame bytes for this shape — the ratio catches it.)
    let full_paint = {
        let one = banded_list_frame(size, 0, style());
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let prev = Surface::new(size, Cell::EMPTY);
        let mut out = Vec::new();
        presenter.emit(
            diff.compute_full(&prev, &one),
            &one,
            &PresentCaps::FULL,
            &mut out,
        );
        out.len()
    };
    let steady_per_frame = total.saturating_sub(full_paint) / 29;
    assert!(
        steady_per_frame < full_paint,
        "steady banded-scroll frame ({steady_per_frame} B) must cost less than a \
         full paint ({full_paint} B) — chrome is being repainted"
    );
}

/// Scrolls mixed with in-band edits (a highlighted selection bar moving
/// against the scroll): the compound case where scroll deltas + cell
/// diffs interleave. Property only — this is where region emission goes
/// wrong first.
#[test]
fn scroll_with_moving_selection_property() {
    let size = Size::new(60, 16);
    let sel_style = Style::new()
        .fg(Rgba::rgb(10, 10, 14))
        .bg(Rgba::rgb(220, 180, 90));
    let mut frames = Vec::new();
    for step in 0..24usize {
        let offset = step; // scroll one per frame
        let mut f = list_frame(size, offset, style());
        // Selection bar oscillates within the view while content scrolls.
        let sel_row = (step * 3 % 14) as i32 + 1;
        f.draw_text(
            0,
            sel_row,
            &format!("item {:05} — details", offset + sel_row as usize),
            sel_style,
        );
        frames.push(f);
    }
    run_sequence(size, &frames, "scroll+selection");
}

// ---------------------------------------------------------------------
// Cycle 5: the scroll-opt referee verdict. RENDER's compute_scrolled +
// emit_scrolled run against the DECSTBM VtScreen — the property is the
// flip gate, the byte ratio is the published win.
// ---------------------------------------------------------------------

#[test]
fn scrolled_log_append_property_and_bytes_won() {
    let size = Size::new(90, 28);
    let frames: Vec<Surface> = (1..=30)
        .map(|n| log_append_frame(size, n, 2, style()))
        .collect();
    let plain = run_sequence(size, &frames, "log-append-plain");
    let (opt, shifted) = run_sequence_scrolled(size, &frames, "log-append-opt");
    eprintln!(
        "scroll verdict: log-append plain={plain} B, scrolled={opt} B, \
         win={:.1}x, shifted {shifted}/30 frames",
        plain as f64 / opt as f64
    );
    assert!(shifted > 0, "log append never engaged the scroll path");
    assert!(
        opt < plain,
        "scroll path must not cost MORE bytes than plain repaint"
    );
}

#[test]
fn scrolled_list_updown_property_and_bytes_won() {
    let size = Size::new(70, 20);
    let mut frames = Vec::new();
    for offset in 0..25usize {
        frames.push(list_frame(size, offset, style()));
    }
    for offset in (5..25usize).rev() {
        frames.push(list_frame(size, offset, style()));
    }
    let plain = run_sequence(size, &frames, "list-plain");
    let (opt, shifted) = run_sequence_scrolled(size, &frames, "list-opt");
    let n = frames.len();
    eprintln!(
        "scroll verdict: list up+down plain={plain} B, scrolled={opt} B, \
         win={:.1}x, shifted {shifted}/{n} frames",
        plain as f64 / opt as f64
    );
    assert!(shifted > 0, "list scroll never engaged the scroll path");
}

#[test]
fn scrolled_banded_chrome_property_and_bytes_won() {
    let size = Size::new(70, 22);
    let frames: Vec<Surface> = (0..30usize)
        .map(|off| banded_list_frame(size, off, style()))
        .collect();
    let plain = run_sequence(size, &frames, "banded-plain");
    // Full-frame damage: the union's edge rows are unchanged chrome, so
    // the anchor heuristic has nothing to align — expected to decline
    // (bytes equal plain, property still holds).
    let (opt_full, shifted_full) = run_sequence_scrolled(size, &frames, "banded-opt-fullbleed");
    // Band-tight damage (what damage tracking actually reports for a
    // scrolling pane): rows 1..h-1 changed, chrome rows 0 and h-1 not.
    let band = Rect::new(0, 1, size.w, size.h - 2);
    let (opt_band, shifted_band) =
        run_sequence_scrolled_damage(size, &frames, Some(band), "banded-opt-tight");
    eprintln!(
        "scroll verdict: banded plain={plain} B; full-frame damage scrolled={opt_full} B \
         (shifted {shifted_full}/30); band-tight damage scrolled={opt_band} B \
         (win={:.1}x, shifted {shifted_band}/30)",
        plain as f64 / opt_band as f64
    );
    // Chrome rows sit OUTSIDE the shift band: any margin mistake smears
    // them and the property assert inside the drive has already fired.
    assert!(
        shifted_band > 0,
        "band-tight damage must engage the scroll path for a banded list"
    );
}

/// Randomized rounds: scroll by a random amount in a random direction,
/// then mutate a few random cells (in and OUT of the band), sometimes
/// scroll two frames in a row, sometimes not at all. compute_scrolled
/// must always produce bytes that reconstruct the target exactly —
/// detection choosing badly may only cost bytes, never pixels.
#[test]
fn scrolled_random_mutation_rounds_hold_the_property() {
    let size = Size::new(64, 18);
    for seed in 0..6u64 {
        let mut rng = Rng::new(0x0005_C011_0000 + seed);
        let mut offset = 30usize;
        let mut frames = Vec::new();
        for _ in 0..40 {
            match rng.below(10) {
                0..=5 => offset += rng.below(3) + 1, // scroll down
                6..=8 => offset = offset.saturating_sub(rng.below(3) + 1),
                _ => {} // hold
            }
            let mut f = list_frame(size, offset, style());
            for _ in 0..rng.below(4) {
                let x = rng.below(size.w as usize - 10) as i32;
                let y = rng.below(size.h as usize) as i32;
                f.draw_text(x, y, "*EDIT*", style().fg(Rgba::rgb(255, 80, 80)));
            }
            frames.push(f);
        }
        run_sequence_scrolled(size, &frames, &format!("random-rounds seed {seed}"));
    }
}

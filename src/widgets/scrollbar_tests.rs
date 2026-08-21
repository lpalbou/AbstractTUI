//! The scrollbar seam: the geometry, the inverse, the gesture.
//!
//! The property test is the point of this file. A scrollbar whose
//! pointer→offset mapping is not the inverse of its offset→thumb
//! placement moves the thumb out from under the cursor on the press that
//! grabs it — the field report that produced this module ("I am unsure
//! it is draggable at all"), reproduced here as an assertion instead of
//! a feeling.

use super::*;

fn m(h: i32, total: i32, first: i32) -> Metrics {
    metrics(Rect::new(0, 0, 40, h), 1, first, total)
}

#[test]
fn thumb_is_floored_but_never_fills_a_travelling_track() {
    // A 3000-row buffer in a 30-row pane asks for 0 cells.
    let bar = m(30, 3000, 0);
    assert_eq!(bar.thumb.h, MIN_THUMB, "floored, not a dot");
    assert_eq!(bar.travel, 27);
    // Content that FITS is the honest full-length thumb.
    let fits = m(30, 10, 0);
    assert_eq!(fits.thumb.h, 30);
    assert_eq!(fits.travel, 0);
    assert!(!fits.overflows());
    // A two-row bar still leaves the thumb somewhere to go.
    let tiny = m(2, 100, 0);
    assert!(tiny.thumb.h < 2, "floor yields to h - 1: {tiny:?}");
}

#[test]
fn the_bottom_cell_means_the_bottom() {
    // Drawing FLOORS, so a thumb parked on the last cell is a promise:
    // the offset really is at its maximum (what a follow-tail app reads
    // position off).
    let bar = m(30, 300, 270);
    assert_eq!(bar.thumb.bottom(), 30, "at max_off the thumb is at the end");
    let almost = m(30, 300, 269);
    assert!(
        almost.thumb.bottom() < 30,
        "one row short of the end must not draw as the end: {almost:?}"
    );
}

#[test]
fn offset_at_is_the_inverse_of_metrics() {
    // THE invariant: for every reachable offset, pressing the thumb's
    // own top cell and dragging nowhere leaves the thumb exactly where
    // it was drawn. Before the seam this drifted by up to 307 rows on a
    // 3000-row transcript.
    for (h, total) in [
        (30, 300),
        (30, 3000),
        (10, 100),
        (50, 60),
        (4, 20),
        (2, 100),
    ] {
        let max_off = (total - h).max(0);
        for first in 0..=max_off {
            let before = m(h, total, first);
            let Zone::Thumb { grab_dy } = hit(&before, Point::new(39, before.thumb.y))
                .expect("the thumb's own top cell hits the thumb")
            else {
                panic!("press on the thumb top must be a thumb hit, not track");
            };
            assert_eq!(grab_dy, 0, "grabbed at the top edge");
            let to = offset_at(&before, before.thumb.y, grab_dy);
            let after = m(h, total, to);
            assert_eq!(
                after.thumb.y, before.thumb.y,
                "h={h} total={total} first={first}: the thumb moved under a press ON it \
                 (offset {first} -> {to})"
            );
        }
    }
}

#[test]
fn a_drag_moves_the_thumb_with_the_pointer() {
    // Grab the middle of the thumb, drag five rows down: the thumb's top
    // lands five rows lower, not somewhere proportional-but-elsewhere.
    let bar = m(30, 300, 135);
    let zone = hit(&bar, Point::new(39, bar.thumb.y + 1)).expect("hit");
    let grab = grab_for(&bar, zone);
    assert_eq!(grab, 1);
    let to = offset_at(&bar, bar.thumb.y + 1 + 5, grab);
    let after = m(30, 300, to);
    assert_eq!(after.thumb.y, bar.thumb.y + 5, "thumb tracks the pointer");
}

#[test]
fn a_drag_past_either_end_clamps_instead_of_wrapping() {
    let bar = m(30, 300, 135);
    assert_eq!(offset_at(&bar, -99, 0), 0);
    assert_eq!(offset_at(&bar, 999, 0), bar.max_off);
}

#[test]
fn a_track_press_centers_the_thumb_on_the_pointer() {
    // The teleport convention: the thumb arrives UNDER the cursor, so
    // the drag that follows starts from where the eye already is.
    let bar = m(30, 300, 0);
    let zone = hit(&bar, Point::new(39, 20)).expect("hit");
    assert_eq!(zone, Zone::Track);
    let grab = grab_for(&bar, zone);
    let to = offset_at(&bar, 20, grab);
    let after = m(30, 300, to);
    assert!(
        after.thumb.y <= 20 && 20 < after.thumb.bottom(),
        "pressed row 20, thumb landed at {:?}",
        after.thumb
    );
}

#[test]
fn a_press_outside_the_strip_is_not_ours() {
    let bar = m(30, 300, 0);
    assert_eq!(
        hit(&bar, Point::new(38, 5)),
        None,
        "one cell left of the bar"
    );
    assert_eq!(hit(&bar, Point::new(39, 30)), None, "one row below the bar");
    assert!(hit(&bar, Point::new(39, 5)).is_some());
}

#[test]
fn width_carves_from_the_right_and_never_widens_into_content() {
    let wide = metrics(Rect::new(0, 0, 40, 30), 2, 0, 300);
    assert_eq!(wide.track, Rect::new(38, 0, 2, 30));
    assert_eq!(wide.thumb.w, 2, "the thumb spans the strip");
    // A caller that already owns its 1-column rect gets the identity.
    let own = metrics(Rect::new(39, 0, 1, 30), 1, 0, 300);
    assert_eq!(own.track, Rect::new(39, 0, 1, 30));
    // An absurd width is clamped to the rect, never past it.
    let clamped = metrics(Rect::new(0, 0, 3, 30), 9, 0, 300);
    assert_eq!(clamped.track.w, 3);
}

#[test]
fn a_fitting_content_steers_nothing() {
    let bar = m(30, 10, 0);
    assert_eq!(offset_at(&bar, 25, 0), 0, "no overflow, no travel");
}

#[test]
fn a_collapsed_rect_owns_no_strip() {
    // Overflow pressure can squeeze a pane to nothing mid-frame; the bar
    // must not carve a column outside it.
    for rect in [Rect::new(4, 4, 0, 10), Rect::new(4, 4, 10, 0)] {
        let bar = metrics(rect, 1, 5, 300);
        assert_eq!(bar.track.w, 0);
        assert!(!bar.overflows());
        assert_eq!(hit(&bar, Point::new(4, 4)), None);
    }
}

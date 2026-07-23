//! Canonical-space geometry shared by the passes.
//!
//! The layered pipeline computes in (cross, flow) space: ranks advance
//! along the *flow* axis, siblings spread along the *cross* axis. A
//! final linear map takes (cross, flow) to screen (x, y) per
//! [`Direction`] — LR is the transpose of TD, BT/RL mirror the flow
//! axis by negation (origin normalization absorbs the offset). Node
//! cards never rotate: a `w x h` card stays `w x h` in every direction.

use abstracttui::base::{Point, Rect, Size};

use crate::desc::Direction;

/// Extent of a card along the flow axis for `dir`.
pub(crate) fn flow_extent(dir: Direction, size: Size) -> f64 {
    if dir.is_vertical() {
        f64::from(size.h)
    } else {
        f64::from(size.w)
    }
}

/// Extent of a card along the cross axis for `dir`.
pub(crate) fn cross_extent(dir: Direction, size: Size) -> f64 {
    if dir.is_vertical() {
        f64::from(size.w)
    } else {
        f64::from(size.h)
    }
}

/// Map a card at (cross, flow) start corner into a screen rect.
pub(crate) fn map_rect(dir: Direction, cross: f64, flow: f64, size: Size) -> Rect {
    let fe = flow_extent(dir, size);
    let flow = if dir.is_reversed() {
        -(flow + fe)
    } else {
        flow
    };
    let (x, y) = match dir {
        Direction::TopDown | Direction::BottomTop => (cross, flow),
        Direction::LeftRight | Direction::RightLeft => (flow, cross),
    };
    Rect::new(round(x), round(y), size.w, size.h)
}

/// Map a waypoint at (cross, flow) into a screen point.
///
/// Mirrors negate the cell's FAR edge, `-(flow + 1)`: a waypoint is a
/// CELL (the half-open interval `[f, f+1)`), so its mirror is the
/// interval `[-(f+1), -f)` — exactly the rule [`map_rect`] applies
/// with `extent = 1`. The earlier bare `-flow` shifted every BT/RL
/// waypoint one cell along the flow axis, landing source anchors
/// INSIDE their card (found cycle 2 by the view half's BT stroke
/// golden; pinned by
/// `tests/view_attack_list.rs::bt_rl_waypoints_mirror_like_rects_and_stay_out_of_cards`).
/// Round-half-away commutes with this mirror (`round(-(f+1)) ==
/// -(round(f)+1)`), so fractional band centers mirror consistently
/// with their canonical twin.
pub(crate) fn map_point(dir: Direction, cross: f64, flow: f64) -> Point {
    let flow = if dir.is_reversed() {
        -(flow + 1.0)
    } else {
        flow
    };
    let (x, y) = match dir {
        Direction::TopDown | Direction::BottomTop => (cross, flow),
        Direction::LeftRight | Direction::RightLeft => (flow, cross),
    };
    Point::new(round(x), round(y))
}

/// Round half away from zero — symmetric under negation, so mirrored
/// directions round identically to their canonical twin.
pub(crate) fn round(v: f64) -> i32 {
    v.round() as i32
}

/// The point where the straight segment from `rect`'s center toward
/// `toward` (a center in f64 cells) leaves the rect border. Used by the
/// force and grid passes for straight-line edge anchors. Falls back to
/// the center for degenerate (coincident) centers.
pub(crate) fn clip_border(rect: Rect, toward: (f64, f64)) -> Point {
    let cx = f64::from(rect.x) + f64::from(rect.w) / 2.0;
    let cy = f64::from(rect.y) + f64::from(rect.h) / 2.0;
    let dx = toward.0 - cx;
    let dy = toward.1 - cy;
    let hw = f64::from(rect.w) / 2.0;
    let hh = f64::from(rect.h) / 2.0;
    let tx = if dx.abs() > 1e-9 {
        hw / dx.abs()
    } else {
        f64::INFINITY
    };
    let ty = if dy.abs() > 1e-9 {
        hh / dy.abs()
    } else {
        f64::INFINITY
    };
    let t = tx.min(ty);
    if !t.is_finite() {
        return Point::new(round(cx), round(cy));
    }
    Point::new(round(cx + dx * t), round(cy + dy * t))
}

/// Waypoints for a self-edge: a small lobe on the right face of the
/// card (constant, direction-independent — documented v1 choice).
pub(crate) fn self_loop(rect: Rect) -> Vec<Point> {
    let cy = rect.y + rect.h / 2;
    let x = rect.right();
    vec![
        Point::new(x, cy - 1),
        Point::new(x + 2, cy - 1),
        Point::new(x + 2, cy + 1),
        Point::new(x, cy + 1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_are_transposes_and_mirrors() {
        let size = Size::new(5, 5);
        let td = map_rect(Direction::TopDown, 3.0, 7.0, size);
        let lr = map_rect(Direction::LeftRight, 3.0, 7.0, size);
        assert_eq!((td.x, td.y), (3, 7));
        assert_eq!((lr.x, lr.y), (7, 3), "LR transposes TD");
        let bt = map_rect(Direction::BottomTop, 3.0, 7.0, size);
        assert_eq!((bt.x, bt.y), (3, -12), "BT mirrors flow by negation");
        let rl = map_rect(Direction::RightLeft, 3.0, 7.0, size);
        assert_eq!((rl.x, rl.y), (-12, 3));
    }

    #[test]
    fn border_clip_hits_the_facing_side() {
        let r = Rect::new(0, 0, 10, 4);
        // Target far to the right: exit through x = 10, at mid-height.
        assert_eq!(clip_border(r, (100.0, 2.0)), Point::new(10, 2));
        // Target below: exit through the bottom edge.
        assert_eq!(clip_border(r, (5.0, 50.0)), Point::new(5, 4));
        // Degenerate: coincident centers fall back to the center.
        assert_eq!(clip_border(r, (5.0, 2.0)), Point::new(5, 2));
    }
}

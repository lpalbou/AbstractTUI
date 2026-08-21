//! The scrollbar: ONE geometry, one hit test, one paint, one inverse.
//!
//! Every widget that scrolls a viewport paints the same strip in its
//! rightmost columns — `Scroll`, `List`, `Table`, `FilePicker`. Before
//! this module each of them owned a piece of the answer and none owned
//! the whole one: the thumb's placement lived inside a paint loop and
//! was never returned, so a widget that wanted to DRAG the thumb had to
//! invent a second pointer→offset mapping. Two mappings that are not
//! inverses of each other put the thumb somewhere the cursor is not,
//! which is exactly how a working scrollbar comes to feel broken.
//!
//! ## The invariant
//!
//! [`offset_at`] is the exact inverse of [`metrics`]: for a press at row
//! `y` with grab `dy`, `metrics(.., offset_at(.., y, dy)).thumb.y` is
//! `y - dy`. **The thumb lands under the cursor and stays there** for
//! the whole drag, at every content length. The rounding is deliberate
//! in both directions: the DRAWING floors (a thumb at the track's bottom
//! cell means the offset is really at its maximum — a follow-tail app
//! reads position off that pixel), and the INVERSE ceils (the smallest
//! offset that draws its thumb on the pressed cell), which is what makes
//! the round trip land on the cell itself instead of one above it.
//!
//! ## The gesture
//!
//! A press on the THUMB moves nothing — it only remembers where inside
//! the thumb the pointer grabbed ([`Zone::Thumb`]'s `grab_dy`), so the
//! following drag slides the content by exactly the distance the pointer
//! travels. A press on bare TRACK is a teleport (the macOS convention,
//! and the only sane one for a track this short): the thumb centers on
//! the pointer and the drag continues from there.
//!
//! ## Width and weight
//!
//! The strip is `width` columns carved from the right of the rect it is
//! given (a caller that already owns a 1-column rect passes `width = 1`
//! and the carve is the identity). Callers that reserve the gutter in
//! layout must reserve the SAME width — [`metrics`] never widens itself
//! into content.
//!
//! The glyphs are block elements, not box drawing: `█` for the thumb is
//! a full cell of ink where `┃` was a hairline, and `▏` for the rail
//! keeps the track legible without competing. Block elements are also
//! the safest family on degraded terminals (`gfx::mosaic` falls back to
//! them for exactly that reason), so the heavier bar is not a
//! compatibility trade.
//!
//! OWNER: REACT (the widget-layer scroll seam).

use crate::base::{Point, Rect, Rgba};
use crate::render::Style;
use crate::ui::StyledCanvas;

/// Shortest thumb the eye can find, read a position from, and follow
/// (rows). The exact proportion of a long transcript rounds to zero — a
/// 3000-row buffer in a 30-row pane asks for 0 cells — and a 1-cell
/// thumb is a dot nobody can track while scrolling.
pub(crate) const MIN_THUMB: i32 = 3;

/// The rail glyph: a one-eighth block, drawn in the strip's first column.
const RAIL: &str = "▏";
/// The thumb glyph: a full cell of ink, drawn across every strip column.
const THUMB: &str = "█";

/// The solved strip, in SCREEN coordinates. `travel` and `max_off` are
/// the two ranges the mapping runs between; both are already clamped, so
/// callers never divide by zero.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Metrics {
    /// The whole strip (the rightmost `width` columns of the rect).
    pub track: Rect,
    /// The thumb, inside `track`.
    pub thumb: Rect,
    /// Largest first-row the content allows (`total - track.h`, floored
    /// at 0 — zero means the content fits and the thumb fills the track).
    pub max_off: i32,
    /// Rows the thumb can travel (`track.h - thumb.h`).
    pub travel: i32,
}

impl Metrics {
    /// Does the content overflow at all? A fitting content draws a
    /// full-length thumb (the honest answer) and steers nothing.
    pub fn overflows(&self) -> bool {
        self.max_off > 0 && self.travel > 0
    }
}

/// Where a press landed inside the strip.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    /// On the thumb, `grab_dy` rows below its top edge.
    Thumb { grab_dy: i32 },
    /// On bare track — a teleport, centering the thumb on the pointer.
    Track,
}

/// Solve the strip: the rightmost `width` columns of `rect`, with the
/// thumb sized proportionally (floored at [`MIN_THUMB`]) and placed at
/// `first`. `total` is content rows; `first` is the top visible row.
pub(crate) fn metrics(rect: Rect, width: i32, first: i32, total: i32) -> Metrics {
    if rect.w <= 0 || rect.h <= 0 {
        // A collapsed rect owns no column: an empty strip hits nothing,
        // steers nothing, and paints nothing (never one cell to the LEFT
        // of the widget, which `rect.right() - w` would carve).
        return Metrics {
            track: Rect::new(rect.x, rect.y, 0, 0),
            thumb: Rect::new(rect.x, rect.y, 0, 0),
            max_off: 0,
            travel: 0,
        };
    }
    let w = width.clamp(1, rect.w);
    let track = Rect::new(rect.right() - w, rect.y, w, rect.h.max(1));
    let h = track.h;
    let total = total.max(h);
    // Thumb LENGTH is proportional with a floor, and the floor never
    // fills the track: a thumb with no room to travel reports no
    // position at all. It yields to `h - 1` on very short bars, and to
    // `h` itself when the content fits.
    let floor = MIN_THUMB.min(h - 1).max(1);
    let thumb_h = ((h * h) / total.max(1)).clamp(floor, h);
    let max_off = total - h;
    let travel = h - thumb_h;
    // FLOOR (see the module note): the bottom cell means the bottom.
    let thumb_y = if max_off > 0 && travel > 0 {
        track.y + (first.clamp(0, max_off) * travel) / max_off
    } else {
        track.y
    };
    Metrics {
        track,
        thumb: Rect::new(track.x, thumb_y, w, thumb_h),
        max_off,
        travel,
    }
}

/// Which zone `p` is in, or `None` when the pointer is outside the strip.
pub(crate) fn hit(m: &Metrics, p: Point) -> Option<Zone> {
    if !m.track.contains(p) {
        return None;
    }
    if p.y >= m.thumb.y && p.y < m.thumb.bottom() {
        Some(Zone::Thumb {
            grab_dy: p.y - m.thumb.y,
        })
    } else {
        Some(Zone::Track)
    }
}

/// The offset that puts the thumb's TOP at `y - grab_dy` — the exact
/// inverse of [`metrics`]'s placement (see the module note on rounding).
/// A press on bare track passes `grab_dy = m.thumb.h / 2` to center the
/// thumb on the pointer.
pub(crate) fn offset_at(m: &Metrics, y: i32, grab_dy: i32) -> i32 {
    if !m.overflows() {
        return 0;
    }
    let top = (y - grab_dy - m.track.y).clamp(0, m.travel);
    // CEIL: the smallest offset whose drawn thumb covers this cell.
    ((top * m.max_off) + m.travel - 1) / m.travel
}

/// The grab a press implies: the pointer's own offset inside the thumb,
/// or the thumb's center for a teleport off bare track.
pub(crate) fn grab_for(m: &Metrics, zone: Zone) -> i32 {
    match zone {
        Zone::Thumb { grab_dy } => grab_dy,
        Zone::Track => m.thumb.h / 2,
    }
}

/// Paint the strip. `hot` (pointer over it, or a live drag) swaps the
/// thumb ink for the caller's hot token — the affordance that answers
/// "is this thing clickable?" before the user has to guess.
pub(crate) fn draw(
    canvas: &mut dyn StyledCanvas,
    m: &Metrics,
    hot: bool,
    rail_ink: Rgba,
    thumb_ink: Rgba,
    hot_ink: Rgba,
    ground: Rgba,
) {
    if m.track.w <= 0 || m.track.h <= 0 {
        return;
    }
    // One memset for the strip's ground (the cheap fill path takes
    // spaces only), then ink over it: the rail in the first column, the
    // thumb across every column.
    let blank = Style::new().fg(rail_ink).bg(ground);
    canvas.fill_styled(m.track, ' ', &blank);
    for y in m.track.y..m.track.bottom() {
        canvas.print_styled(Point::new(m.track.x, y), RAIL, &blank);
    }
    let ink = if hot { hot_ink } else { thumb_ink };
    let style = Style::new().fg(ink).bg(ground);
    let bottom = m.thumb.bottom().min(m.track.bottom());
    for y in m.thumb.y..bottom {
        for x in m.thumb.x..m.thumb.right() {
            canvas.print_styled(Point::new(x, y), THUMB, &style);
        }
    }
}

#[cfg(test)]
#[path = "scrollbar_tests.rs"]
mod tests;

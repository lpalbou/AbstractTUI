//! Feed windowed painter (child module of `feed`, split for the
//! file-size discipline — one file, one task: this one turns the
//! typeset entries into pixels for the visible band; `feed.rs` owns
//! the public model, the state handle and the widget wiring).
//!
//! OWNER: CONTENT (app-widgets wave).

use crate::base::Rect;
use crate::theme::TokenSet;
use crate::ui::StyledCanvas;

use super::super::markdown::draw_rows;
use super::item::SharedDrawFn;
use super::typeset::Segment;
use super::FeedState;

/// Windowed paint: only entries intersecting `rect ∩ canvas bounds`
/// touch the canvas. Custom draws run after the state borrow releases
/// (they are app code).
pub(super) fn draw_feed(
    state: &FeedState,
    t: &TokenSet,
    selected: Option<&str>,
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
) {
    let mut customs: Vec<(SharedDrawFn, Rect)> = Vec::new();
    {
        let mut inner = state.inner.borrow_mut();
        if rect.w > 1 && inner.width != rect.w {
            // Width discovery / resize: re-typeset (pure cache fill;
            // the reactive extent syncs via the deferred fixup).
            inner.width = rect.w;
            inner.retypeset_all();
            drop(inner);
            state.schedule_geometry_sync();
            inner = state.inner.borrow_mut();
        }
        let bounds = Rect::from_size(canvas.size());
        let band = rect.intersect(bounds);
        if band.is_empty() || inner.entries.is_empty() {
            return;
        }
        let first_row = band.y - rect.y;
        let last_row = first_row + band.h; // exclusive
        let inner = &*inner;
        // Selection highlight: ground the selected item's band FIRST —
        // rows paint over with transparent backgrounds, so item inks
        // stay and the tint shows through (code-fence rows keep their
        // own ground by design).
        if let Some(i) = selected.and_then(|k| inner.index.get(k).copied()) {
            let top = inner.prefix[i];
            let h = inner.entries[i].height;
            if top < last_row && top + h > first_row {
                canvas.fill(
                    Rect::new(rect.x, rect.y + top, rect.w, h),
                    ' ',
                    t.selection_fg,
                    t.selection_bg,
                );
            }
        }
        // First entry whose span can reach the band (binary search on
        // prefix starts, then step back one).
        let mut i = inner
            .prefix
            .partition_point(|&p| p <= first_row)
            .saturating_sub(1);
        while i < inner.entries.len() {
            let top = inner.prefix[i];
            if top >= last_row {
                break;
            }
            let entry = &inner.entries[i];
            let mut seg_top = top;
            for seg in &entry.segments {
                let h = seg.height();
                if seg_top >= last_row {
                    break;
                }
                if seg_top + h > first_row {
                    match seg {
                        Segment::Rows(rows) => {
                            let skip = (first_row - seg_top).max(0) as usize;
                            let visible = ((last_row - seg_top).min(h) as usize).min(rows.len());
                            if skip < visible {
                                let y = rect.y + seg_top + skip as i32;
                                draw_rows(
                                    canvas,
                                    Rect::new(rect.x, y, rect.w, (visible - skip) as i32),
                                    t,
                                    &rows[skip..visible],
                                );
                            }
                        }
                        Segment::Custom { draw, height } => {
                            // The custom block gets its FULL rect (its
                            // top may sit above the band); the canvas
                            // clips.
                            customs.push((
                                draw.clone(),
                                Rect::new(rect.x, rect.y + seg_top, rect.w, *height),
                            ));
                        }
                    }
                }
                seg_top += h;
            }
            i += 1;
        }
    }
    for (draw, r) in customs {
        draw(canvas, r);
    }
}

//! Wave-10 size/ratio adversarial sweep — spine: the FUSION-class fix
//! (operator-mandated, gateway-console field incident 2026-07-24).
//!
//! The incident, both halves proven console-side: a fixed 1-line title
//! bar VANISHED once real data loaded — flex crushed it to zero
//! (content over-demand) AND the zero-area child's draw closure STILL
//! RAN with the degenerate rect. A hand-rolled closure that clips only
//! horizontally then paints "its" row smeared the title onto whichever
//! sibling owned that y — two texts fused on one row.
//!
//! Engine root cause: `UiTree::draw_node` culled on
//! `!rect.intersects(clip) && !rect.is_empty()` — empty rects never
//! intersect anything, so they FELL THROUGH the cull and their draw
//! closures ran. The fix skips a node's OWN paint when its rect is
//! empty, preserving the `probe_when_culled` measurement exemption
//! (first-app/0281) and still walking children (an empty parent's
//! children can be non-empty: absolute children size independently,
//! flow children with an explicit min hold their extent — pinned
//! below).
//!
//! Axes (a)-(f) of the sweep live in `wave_size_sweep_parts/`.
//! Findings + the guarantees-vs-recipes table:
//! reviews/wave10/size-ratio-sweep.md.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver};
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::layout::{Dimension, Inset, Style as LayoutStyle};
use abstracttui::reactive::{create_root, Signal};
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{dyn_view, text, BufferCanvas, Element, UiTree};
use abstracttui::widgets::Scroll;

#[path = "wave_size_sweep_parts/harness.rs"]
pub mod harness;

#[path = "wave_size_sweep_parts/chrome.rs"]
mod chrome;

#[path = "wave_size_sweep_parts/pagehost.rs"]
mod pagehost;

#[path = "wave_size_sweep_parts/modal_drawer.rs"]
mod modal_drawer;

#[path = "wave_size_sweep_parts/resize_live.rs"]
mod resize_live;

#[path = "wave_size_sweep_parts/unicode_narrow.rs"]
mod unicode_narrow;

use harness::{
    assert_matches_fresh_paint, config, drive_to_idle, hand_rolled_bar, heavy_page, CHROME_MARK,
};

// ---------------------------------------------------------------------------
// The fusion, at the tree level (the failing-first reproduction)
// ---------------------------------------------------------------------------

/// Two rows in a column: a 1-line title bar with a hand-rolled draw
/// closure, then heavy text content. Content pressure crushes the bar
/// to zero height. PRE-FIX: the bar's closure still ran with the
/// degenerate rect and painted its full row at y=0 — the row the
/// content now owns — so row 0 showed content glyphs FUSED with bar
/// glyphs. POST-FIX: a zero-area node paints nothing; collapse is
/// clean absence.
#[test]
fn fusion_zero_crushed_bar_must_not_paint_the_sibling_row() {
    let size = Size::new(40, 8);
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|_cx| {
        let view = Element::new()
            .style(LayoutStyle::column())
            .child(hand_rolled_bar("TITLE").build())
            .child(heavy_page(40))
            .build();
        // Mounted outside the closure would drop `tree` borrows; the
        // established pattern mounts inside create_root.
        tree.mount(_cx, view);
    });
    tree.layout();

    // Layout truth first: the bar IS crushed to zero (content pressure
    // 40 rows into 8), the page owns the full viewport.
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    let row0 = canvas.row_text(0);
    assert!(
        row0.starts_with("data-row-000"),
        "page content must own row 0 once the bar collapsed: {row0:?}"
    );
    assert!(
        !row0.contains(CHROME_MARK),
        "FUSION: the zero-crushed bar's draw closure painted onto the \
         sibling's row — row 0 mixes bar glyphs into page content: {row0:?}"
    );
    // No bar glyph anywhere: collapse means clean absence, not a smear
    // on some other row.
    for y in 0..size.h {
        let row = canvas.row_text(y);
        assert!(
            !row.contains(CHROME_MARK),
            "bar glyphs leaked to row {y}: {row:?}"
        );
    }
    root.dispose();
}

/// The console seat's exact scenario shape, through the REAL pipeline:
/// header line(1) + grow page + footer line(1) at 100x16 with heavy
/// content. Both chrome rows crush (weighted shrink sends virtually
/// all loss to the 200-row page, but 1-cell bars still hit zero);
/// post-fix the screen shows page rows only — no fused chrome glyphs
/// anywhere — and the zero-collapse notice NAMES the crush (the
/// solver-side diagnostic must not depend on the skipped draw).
#[test]
fn fusion_console_shape_header_page_footer_at_100x16() {
    let size = Size::new(100, 16);
    let mut app = App::new(size);
    app.mount(|_cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(hand_rolled_bar("HEADER").build())
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .child(heavy_page(200))
                    .build(),
            )
            .child(hand_rolled_bar("FOOTER").build())
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    let screen = vt.to_text();
    assert!(
        screen.contains("data-row-000"),
        "page content must render:\n{screen}"
    );
    assert!(
        !screen.contains(CHROME_MARK),
        "FUSION: crushed chrome painted into page rows:\n{screen}"
    );
    // The engine NAMES the collapse (debug builds): the notice rides
    // the solver, not the (now skipped) draw.
    if cfg!(debug_assertions) {
        let notices = app.startup_notices().join("\n");
        assert!(
            notices.contains("collapsed to 0"),
            "zero-collapse notice must still fire after the fix: {notices:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Children of an empty-rect parent (the honest answer: they CAN paint)
// ---------------------------------------------------------------------------

/// An empty parent's children are not necessarily empty — the skip
/// must be per-node, never per-subtree. Two lanes under this layout
/// engine: (1) an ABSOLUTE child sizes against the parent's content
/// box with its own explicit dimensions; (2) a FLOW child with an
/// explicit `min` on the parent's MAIN axis holds its extent even when
/// the parent's own box is crushed (the freeze loop respects
/// minimums; cross-axis extents cap at the parent's content — a
/// zero-height Row parent crushes every flow child, which is why the
/// crushed parent here is a Column). Both children must still paint
/// while the crushed parent's own draw is skipped.
#[test]
fn empty_parent_children_with_own_extent_still_paint() {
    let size = Size::new(30, 6);
    // Page rows are 8 chars wide; the children paint at columns >= 20,
    // where no later sibling ever paints over them (pre-order: the
    // page paints AFTER the parent subtree and would cover same-cell
    // glyphs — the assertion must target cells the page never owns).
    let narrow_page = || {
        let body: String = (0..40)
            .map(|i| format!("data-{i:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        text(body)
    };
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        // The crushed parent: fixed 1-row COLUMN bar under 40 rows of
        // pressure, with its own (hand-rolled, fusing) draw closure.
        let absolute_child = Element::new()
            .style(
                LayoutStyle::default()
                    .absolute(Inset {
                        left: Some(20),
                        top: Some(3),
                        right: None,
                        bottom: None,
                    })
                    .width(Dimension::Cells(6))
                    .height(Dimension::Cells(1)),
            )
            .draw(|canvas, rect| {
                canvas.print(
                    Point::new(rect.x, rect.y),
                    "ABSKID",
                    Rgba::WHITE,
                    Rgba::TRANSPARENT,
                );
            });
        let min_flow_child = Element::new()
            .style(LayoutStyle::default().h(1).min_h(1))
            .draw(|canvas, rect| {
                canvas.print(
                    Point::new(rect.right() - 6, rect.y),
                    "MINKID",
                    Rgba::WHITE,
                    Rgba::TRANSPARENT,
                );
            });
        let crushed_parent = Element::new()
            .style(LayoutStyle {
                direction: abstracttui::layout::Direction::Column,
                ..LayoutStyle::line(1)
            })
            .draw(move |canvas, rect| {
                let cols = rect.w.max(0) as usize;
                let bar: String = std::iter::repeat_n(CHROME_MARK, cols).collect();
                canvas.print(
                    Point::new(rect.x, rect.y),
                    &bar,
                    Rgba::rgb(220, 220, 40),
                    Rgba::TRANSPARENT,
                );
            })
            .child(absolute_child.build())
            .child(min_flow_child.build());
        let view = Element::new()
            .style(LayoutStyle::column())
            .child(crushed_parent.build())
            .child(narrow_page())
            .build();
        tree.mount(cx, view);
    });
    tree.layout();
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);

    // The parent's own paint is gone (no fusion)...
    for y in 0..size.h {
        let row = canvas.row_text(y);
        assert!(
            !row.contains(CHROME_MARK),
            "crushed parent's own draw leaked to row {y}: {row:?}"
        );
    }
    // ...but its children, whose solved rects are NON-empty, still
    // paint (rects stay truthful; overflow is Visible by default).
    let all: String = (0..size.h).map(|y| canvas.row_text(y) + "\n").collect();
    assert!(
        all.contains("ABSKID"),
        "absolute child of a crushed parent must still paint:\n{all}"
    );
    assert!(
        all.contains("MINKID"),
        "min-held flow child of a crushed parent must still paint:\n{all}"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// The probe exemption (first-app/0281): measurement must not starve
// ---------------------------------------------------------------------------

/// A measured-mode Scroll whose content shrinks to ZERO rows: the
/// content wrapper's rect goes empty, and the extent probe rides the
/// wrapper's draw closure. The empty-rect paint skip must EXEMPT
/// `probe_when_culled` nodes or the extent freezes at the pre-shrink
/// value and the held offset is never repaired (the 0281 void state,
/// now reachable through zero-AREA rects instead of out-of-clip ones).
#[test]
fn scroll_extent_probe_still_reads_a_zero_area_collapse() {
    type Slot<T> = Rc<RefCell<Option<Signal<T>>>>;
    let size = Size::new(24, 6);
    let mut tree = UiTree::new(size);
    let rows_slot: Slot<i32> = Rc::default();
    let extent_slot: Slot<(i32, i32)> = Rc::default();
    let offset_slot: Slot<i32> = Rc::default();
    let (rs, es, os) = (rows_slot.clone(), extent_slot.clone(), offset_slot.clone());
    let (root, ()) = create_root(|cx| {
        let rows = cx.signal(30i32);
        let extent = cx.signal((0i32, 0i32));
        let offset = cx.signal(0i32);
        *rs.borrow_mut() = Some(rows);
        *es.borrow_mut() = Some(extent);
        *os.borrow_mut() = Some(offset);
        let content = dyn_view(LayoutStyle::default(), move || {
            let n = rows.get().max(0);
            if n == 0 {
                // TRULY zero-area content (a `text("")` leaf still
                // measures one empty line): the wrapper's Auto height
                // solves to 0 and its rect goes empty.
                return Element::new()
                    .style(LayoutStyle::default().w(0).h(0))
                    .build();
            }
            let body: String = (0..n)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n");
            text(body)
        });
        let view = Element::new()
            .style(LayoutStyle::fill())
            .child(
                Scroll::new(content)
                    .extent_signal(extent)
                    .offset_y(offset)
                    .view(cx),
            )
            .build();
        tree.mount(cx, view);
    });
    let extent = extent_slot.borrow().expect("extent");
    let rows = rows_slot.borrow().expect("rows");
    let offset = offset_slot.borrow().expect("offset");

    // Settle the deferred geometry loop: draw -> due timers -> effects,
    // until the extent reports the 30-row content.
    let settle = |tree: &mut UiTree| {
        for _ in 0..8 {
            tree.layout();
            let mut canvas = BufferCanvas::new(size);
            tree.draw(&mut canvas);
            abstracttui::reactive::run_due_timers(std::time::Instant::now());
            abstracttui::reactive::flush_effects();
        }
    };
    settle(&mut tree);
    assert_eq!(
        extent.get_untracked().1,
        30,
        "measured extent must see the 30-row content"
    );

    // Hold a deep offset, then collapse the content to NOTHING.
    offset.set(24);
    settle(&mut tree);
    rows.set(0);
    settle(&mut tree);
    assert_eq!(
        extent.get_untracked().1,
        0,
        "the probe must read the zero-area collapse (empty-rect skip \
         must exempt probe_when_culled) — a frozen extent starves the \
         offset repair"
    );
    assert_eq!(
        offset.get_untracked(),
        0,
        "the held offset must be repaired once the extent reads zero"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// Damage semantics across the empty <-> non-empty threshold
// ---------------------------------------------------------------------------

/// The console incident's lifecycle, byte-truth checked: light content
/// (bar visible) -> data loads (bar crushed: CLEAN absence) -> data
/// clears (bar returns intact). Every state must equal a fresh-paint
/// oracle cell-for-cell — a rect crossing the empty threshold in
/// either direction must repaint correctly through the damage path.
#[test]
fn crush_transitions_repaint_cleanly_both_directions() {
    let size = Size::new(60, 10);
    // Plain fn so the ORACLE closure stays `Send` (the fresh-thread
    // oracle rule — see `harness::assert_matches_fresh_paint`); the
    // signal handle only matters to the incumbent.
    fn mount_with(app: &mut App, initial: i32) -> Signal<i32> {
        let slot: Rc<RefCell<Option<Signal<i32>>>> = Rc::default();
        let s = slot.clone();
        app.mount(move |cx| {
            let rows = cx.signal(initial);
            *s.borrow_mut() = Some(rows);
            Element::new()
                .style(LayoutStyle::column())
                .child(hand_rolled_bar("TITLE").build())
                .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                    let n = rows.get().max(0) as usize;
                    let body: String = (0..n)
                        .map(|i| format!("data-row-{i:03}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    text(body)
                }))
                .build()
        })
        .expect("mount");
        let sig = slot.borrow().expect("signal");
        sig
    }

    let mut app = App::new(size);
    let rows = mount_with(&mut app, 4);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert!(
        vt.to_text().contains("TITLE"),
        "light content: the bar is visible\n{}",
        vt.to_text()
    );

    // Data loads: the bar crushes. Non-empty -> empty must vacate the
    // bar's row cleanly (the page repaints it from truth).
    rows.set(300);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert!(
        !vt.to_text().contains(CHROME_MARK) && !vt.to_text().contains("TITLE"),
        "heavy content: the bar must be cleanly absent\n{}",
        vt.to_text()
    );
    assert_matches_fresh_paint("crush (empty)", &vt, size, |app| {
        let _ = mount_with(app, 300);
    });

    // Data clears: empty -> non-empty must repaint the returning bar.
    rows.set(4);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert!(
        vt.to_text().contains("TITLE"),
        "light again: the bar returns intact\n{}",
        vt.to_text()
    );
    assert_matches_fresh_paint("uncrush (returns)", &vt, size, |app| {
        let _ = mount_with(app, 4);
    });
}

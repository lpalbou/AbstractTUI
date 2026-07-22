//! Unit tests for the selection layer: row-flow spans, screen-text
//! extraction (wide cells, trim, blank rows), the event-claim rules,
//! paint/damage bookkeeping, and the pane walk.

use super::*;
use crate::base::Size;
use crate::input::{KeyEvent, MouseEvent};
use crate::render::Style;

fn region(anchor: (i32, i32), head: (i32, i32), clamp: Rect) -> Region {
    Region {
        anchor: Point::new(anchor.0, anchor.1),
        head: Point::new(head.0, head.1),
        clamp,
    }
}

fn spans(r: &Region) -> Vec<Rect> {
    let mut out = Vec::new();
    r.row_spans(&mut out);
    out
}

fn mouse(kind: MouseKind, button: MouseButton, x: i32, y: i32) -> Event {
    Event::Mouse(MouseEvent::new(kind, button, Point::new(x, y), Mods::NONE))
}

fn viewport_clamp() -> Box<dyn FnMut(Point) -> Rect> {
    Box::new(|_| Rect::new(0, 0, 80, 24))
}

// ---------------------------------------------------------------- spans

#[test]
fn single_row_spans_min_to_max_inclusive() {
    let clamp = Rect::new(0, 0, 20, 5);
    // Reading order normalizes a right-to-left drag on one row.
    let fwd = spans(&region((2, 2), (5, 2), clamp));
    let rev = spans(&region((5, 2), (2, 2), clamp));
    assert_eq!(fwd, vec![Rect::new(2, 2, 4, 1)]); // cells 2..=5
    assert_eq!(fwd, rev);
}

#[test]
fn multi_row_flows_first_full_last_within_clamp() {
    let clamp = Rect::new(1, 1, 10, 5); // x 1..11, y 1..6
    let s = spans(&region((4, 2), (6, 4), clamp));
    assert_eq!(
        s,
        vec![
            Rect::new(4, 2, 7, 1),  // anchor -> pane right edge
            Rect::new(1, 3, 10, 1), // full pane span
            Rect::new(1, 4, 6, 1),  // pane left edge -> head (inclusive)
        ]
    );
    // A bottom-up drag selects the same cells.
    assert_eq!(s, spans(&region((6, 4), (4, 2), clamp)));
}

#[test]
fn spans_never_escape_the_clamp() {
    let clamp = Rect::new(2, 1, 5, 2); // x 2..7, y 1..3
    for span in spans(&region((0, 0), (100, 100), clamp)) {
        assert_eq!(
            span.intersect(clamp),
            span,
            "span {span:?} escaped {clamp:?}"
        );
    }
}

// ------------------------------------------------------------ extraction

#[test]
fn extract_trims_trailing_space_and_joins_rows() {
    let mut s = Surface::new(Size::new(12, 3), crate::render::Cell::EMPTY);
    s.draw_text(0, 0, "alpha", Style::new());
    s.draw_text(0, 1, "beta", Style::new());
    let r = region((0, 0), (11, 1), Rect::new(0, 0, 12, 3));
    // Blank cells read as spaces, then per-row trailing trim.
    assert_eq!(extract_text(&s, &r), "alpha\nbeta");
}

#[test]
fn extract_keeps_interior_blank_rows() {
    let mut s = Surface::new(Size::new(8, 3), crate::render::Cell::EMPTY);
    s.draw_text(0, 0, "top", Style::new());
    s.draw_text(0, 2, "bottom", Style::new());
    let r = region((0, 0), (7, 2), Rect::new(0, 0, 8, 3));
    assert_eq!(extract_text(&s, &r), "top\n\nbottom");
}

#[test]
fn extract_never_splits_wide_glyphs() {
    // "ab 世界 cd": 世 leader at x=3 (continuation x=4), 界 at x=5/6.
    let mut s = Surface::new(Size::new(12, 1), crate::render::Cell::EMPTY);
    s.draw_text(0, 0, "ab 世界 cd", Style::new());
    let clamp = Rect::new(0, 0, 12, 1);
    // Span starting ON 世's continuation pulls the leader in.
    let r = region((4, 0), (9, 0), clamp);
    assert_eq!(extract_text(&s, &r), "世界 cd");
    // Span ending ON 界's leader keeps the whole glyph (its continuation
    // is the first excluded cell and gets pulled in).
    let r = region((0, 0), (5, 0), clamp);
    assert_eq!(extract_text(&s, &r), "ab 世界");
}

#[test]
fn extract_mid_glyph_endpoints_cover_the_pair_symmetrically() {
    let mut s = Surface::new(Size::new(6, 1), crate::render::Cell::EMPTY);
    s.draw_text(0, 0, "x世y", Style::new());
    let clamp = Rect::new(0, 0, 6, 1);
    // Anchor and head both on the continuation cell: the single-cell
    // span still yields the whole glyph.
    let r = region((2, 0), (2, 0), clamp);
    assert_eq!(extract_text(&s, &r), "世");
}

// ------------------------------------------------------------- intercept

#[test]
fn selection_claims_left_drag_only_wheel_and_buttons_pass() {
    let sel = selection();
    sel.set_enabled(true);
    let mut clamp = viewport_clamp();

    // Wheel: never claimed (scrolling keeps working mid-selection).
    let wheel = mouse(MouseKind::WheelDown, MouseButton::None, 3, 3);
    assert_eq!(sel.on_input(&wheel, &mut clamp), SelectionAct::Pass);

    // Right button: not ours.
    let rdown = mouse(MouseKind::Down, MouseButton::Right, 3, 3);
    assert_eq!(sel.on_input(&rdown, &mut clamp), SelectionAct::Pass);

    // Left down arms, drag paints, release copies (region stays).
    let down = mouse(MouseKind::Down, MouseButton::Left, 2, 1);
    assert_eq!(sel.on_input(&down, &mut clamp), SelectionAct::Consumed);
    assert!(!sel.is_active(), "a click alone never paints");
    let drag = mouse(MouseKind::Drag, MouseButton::Left, 6, 2);
    assert_eq!(sel.on_input(&drag, &mut clamp), SelectionAct::Consumed);
    assert!(sel.is_active());
    let up = mouse(MouseKind::Up, MouseButton::Left, 6, 2);
    assert_eq!(sel.on_input(&up, &mut clamp), SelectionAct::Copy);
    assert!(sel.is_active(), "release keeps the region visible");

    // A fresh click clears (spec: Esc/click clears).
    let down = mouse(MouseKind::Down, MouseButton::Left, 9, 9);
    assert_eq!(sel.on_input(&down, &mut clamp), SelectionAct::Consumed);
    assert!(!sel.is_active());

    // Disabled: everything passes, even left drags.
    sel.set_enabled(false);
    let down = mouse(MouseKind::Down, MouseButton::Left, 2, 1);
    assert_eq!(sel.on_input(&down, &mut clamp), SelectionAct::Pass);
}

#[test]
fn copy_and_clear_keys_exist_only_while_a_region_is_visible() {
    let sel = selection();
    sel.set_enabled(true);
    let mut clamp = viewport_clamp();

    let ctrl_c = Event::Key(KeyEvent::char('c').with_mods(Mods::CTRL));
    let plain_c = Event::Key(KeyEvent::char('c'));
    let enter = Event::Key(KeyEvent::plain(KeyCode::Enter));
    let esc = Event::Key(KeyEvent::plain(KeyCode::Esc));

    // No region: all pass (Ctrl+C stays the default quit).
    for ev in [&ctrl_c, &plain_c, &enter, &esc] {
        assert_eq!(sel.on_input(ev, &mut clamp), SelectionAct::Pass);
    }

    // With a region: copy keys copy, Esc clears.
    sel.on_input(&mouse(MouseKind::Down, MouseButton::Left, 1, 1), &mut clamp);
    sel.on_input(&mouse(MouseKind::Drag, MouseButton::Left, 5, 1), &mut clamp);
    assert_eq!(sel.on_input(&plain_c, &mut clamp), SelectionAct::Copy);
    assert_eq!(sel.on_input(&ctrl_c, &mut clamp), SelectionAct::Copy);
    assert_eq!(sel.on_input(&enter, &mut clamp), SelectionAct::Copy);
    // Other keys route normally under an active selection.
    let other = Event::Key(KeyEvent::char('x'));
    assert_eq!(sel.on_input(&other, &mut clamp), SelectionAct::Pass);
    assert_eq!(sel.on_input(&esc, &mut clamp), SelectionAct::Consumed);
    assert!(!sel.is_active());
}

#[test]
fn drag_clamps_to_the_pane_resolved_at_anchor() {
    let sel = selection();
    sel.set_enabled(true);
    // The pane under the anchor is a 6x2 box; drags outside clamp in.
    let pane = Rect::new(2, 1, 6, 2);
    let mut clamp: Box<dyn FnMut(Point) -> Rect> = Box::new(move |_| pane);
    sel.on_input(&mouse(MouseKind::Down, MouseButton::Left, 3, 1), &mut clamp);
    sel.on_input(
        &mouse(MouseKind::Drag, MouseButton::Left, 50, 20),
        &mut clamp,
    );
    let region = sel.active_region().expect("region");
    let mut out = Vec::new();
    region.row_spans(&mut out);
    for span in out {
        assert_eq!(span.intersect(pane), span, "span {span:?} escaped the pane");
    }
}

#[test]
fn disabling_mid_selection_clears_and_marks_repair() {
    let sel = selection();
    sel.set_enabled(true);
    let mut clamp = viewport_clamp();
    sel.on_input(&mouse(MouseKind::Down, MouseButton::Left, 1, 1), &mut clamp);
    sel.on_input(&mouse(MouseKind::Drag, MouseButton::Left, 4, 1), &mut clamp);
    assert!(sel.is_active());
    sel.set_enabled(false);
    assert!(!sel.is_active());
    // The repair damage for the (never-painted) region is owed but empty
    // painted bookkeeping keeps it cheap — just verify no panic and that
    // a fresh damage drain yields the pending flag consumed.
    let mut surf = Surface::new(Size::new(10, 3), crate::render::Cell::EMPTY);
    let mut drained = Vec::new();
    surf.take_damage(&mut drained); // clear construction damage
    sel.add_flatten_damage(&mut surf);
    // No painted rects, region cleared before ever painting: nothing owed.
    let mut out = Vec::new();
    surf.take_damage(&mut out);
    assert!(out.is_empty(), "nothing was ever painted: {out:?}");
}

// ---------------------------------------------------------- paint/damage

#[test]
fn paint_patches_inks_keeps_glyphs_and_records_rects() {
    let sel = selection();
    sel.set_enabled(true);
    let mut clamp = viewport_clamp();
    sel.on_input(&mouse(MouseKind::Down, MouseButton::Left, 0, 0), &mut clamp);
    sel.on_input(&mouse(MouseKind::Drag, MouseButton::Left, 4, 0), &mut clamp);

    let mut frame = Surface::new(Size::new(10, 2), crate::render::Cell::EMPTY);
    frame.draw_text(0, 0, "hi 世x", Style::new());
    let fg = Rgba::rgb(1, 2, 3);
    let bg = Rgba::rgb(9, 8, 7);
    sel.paint_into(&mut frame, fg, bg);

    // Glyphs intact, inks replaced, continuation mirrored by pair repair.
    let h = frame.get(0, 0).copied().unwrap();
    assert_eq!(frame.glyph_str(&h), "h");
    assert_eq!((h.fg, h.bg), (fg, bg));
    let wide = frame.get(3, 0).copied().unwrap();
    assert_eq!(frame.glyph_str(&wide), "世");
    assert_eq!((wide.fg, wide.bg), (fg, bg));
    let cont = frame.get(4, 0).copied().unwrap();
    assert!(cont.is_continuation());
    assert_eq!(cont.bg, bg, "continuation carries the leader's inks");

    // Painted bookkeeping: one row rect covering the (glyph-snapped) span.
    // Repair damage after a clear covers exactly that rect.
    sel.clear();
    let mut surf = Surface::new(Size::new(10, 2), crate::render::Cell::EMPTY);
    let mut drained = Vec::new();
    surf.take_damage(&mut drained);
    sel.add_flatten_damage(&mut surf);
    let mut out = Vec::new();
    surf.take_damage(&mut out);
    assert_eq!(out, vec![Rect::new(0, 0, 5, 1)]);
}

#[test]
fn flatten_damage_covers_old_and_new_regions() {
    let sel = selection();
    sel.set_enabled(true);
    let mut clamp = viewport_clamp();
    sel.on_input(&mouse(MouseKind::Down, MouseButton::Left, 0, 0), &mut clamp);
    sel.on_input(&mouse(MouseKind::Drag, MouseButton::Left, 3, 0), &mut clamp);

    let mut frame = Surface::new(Size::new(10, 2), crate::render::Cell::EMPTY);
    frame.draw_text(0, 0, "abcdefgh", Style::new());
    let mut scratch = Vec::new();
    frame.take_damage(&mut scratch); // drop construction/draw damage

    // First paint pass: damage covers the new region.
    sel.add_flatten_damage(&mut frame);
    let mut first = Vec::new();
    frame.take_damage(&mut first);
    assert_eq!(first, vec![Rect::new(0, 0, 4, 1)]);
    sel.paint_into(&mut frame, Rgba::rgb(1, 1, 1), Rgba::rgb(2, 2, 2));

    // Extend the drag: the next flatten damage covers old ∪ new.
    sel.on_input(&mouse(MouseKind::Drag, MouseButton::Left, 6, 1), &mut clamp);
    sel.add_flatten_damage(&mut frame);
    let mut second = Vec::new();
    frame.take_damage(&mut second);
    let covers = |x: i32, y: i32| second.iter().any(|r| r.contains(Point::new(x, y)));
    assert!(covers(0, 0), "old painted cells recompose: {second:?}");
    assert!(covers(6, 1), "new head row recomposes: {second:?}");

    // Nothing changed since: the hook owes nothing.
    sel.paint_into(&mut frame, Rgba::rgb(1, 1, 1), Rgba::rgb(2, 2, 2));
    sel.add_flatten_damage(&mut frame);
    let mut third = Vec::new();
    frame.take_damage(&mut third);
    assert!(third.is_empty(), "steady selection costs zero damage");
}

// ------------------------------------------------------------- pane walk

#[test]
fn pane_walk_prefers_clipping_or_padded_ancestors_else_root() {
    use crate::layout::Edges;
    use crate::ui::{text, Element};

    let mut app = super::super::App::new(Size::new(40, 12));
    app.mount(|_cx| {
        Element::new()
            .style(crate::layout::Style::row())
            .child(
                // Left pane: clipped scroll region, 20 wide.
                Element::new()
                    .style(
                        crate::layout::Style::column()
                            .width(crate::layout::Dimension::Cells(20))
                            .clip(),
                    )
                    .child(text("left one"))
                    .child(text("left two"))
                    .build(),
            )
            .child(
                // Right pane: bordered-block shape (padding 1).
                Element::new()
                    .style(
                        crate::layout::Style::column()
                            .grow(1.0)
                            .padding(Edges::all(1)),
                    )
                    .child(text("right"))
                    .build(),
            )
            .build()
    })
    .unwrap();
    app.pump();
    app.tree().layout();

    // Anchor in the left pane: clamp = the clipped pane, not the 1-row
    // text leaf and not the whole viewport.
    let left = app.tree().pane_rect_at(Point::new(3, 1)).unwrap();
    assert_eq!(left, Rect::new(0, 0, 20, 12));
    // Anchor in the right pane: the padded content box (border gutter
    // excluded on all sides).
    let right = app.tree().pane_rect_at(Point::new(25, 3)).unwrap();
    assert_eq!(right, Rect::new(21, 1, 18, 10));
    // Anchor on the right pane's padding gutter: not inside any pane's
    // content box -> the walk falls back to the root rect.
    let gutter = app.tree().pane_rect_at(Point::new(20, 0)).unwrap();
    assert_eq!(gutter, Rect::new(0, 0, 40, 12));
    // Off-tree: None.
    assert_eq!(app.tree().pane_rect_at(Point::new(100, 100)), None);
}

//! Block tests — `#[path]` sibling of block.rs (file-size discipline,
//! the disclosure_tests.rs pattern). The draw-only surface tests ride
//! DESIGN's `test_util::draw_into` (root draw closure in isolation);
//! the close affordance is interactive and goes through the REAL tree +
//! dispatch (`itest_util`), app-kits 0605.

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::widgets::itest_util::{mount_widget, mouse, render};
use crate::widgets::test_util::{draw_into, row};

const SIZE: Size = Size { w: 20, h: 5 };

#[test]
fn rounded_border_with_left_title() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Rounded)
        .title("Log")
        .element(&t);
    let c = draw_into(view, SIZE);
    assert_eq!(row(&c, 0), format!("╭ Log {}╮", "─".repeat(13)));
    assert_eq!(row(&c, 4), format!("╰{}╯", "─".repeat(18)));
    assert_eq!(c.cell(crate::base::Point::new(0, 2)).unwrap().0, '│');
    // Border color is the border token; title is muted.
    assert_eq!(
        c.cell(crate::base::Point::new(5, 0)).unwrap().1,
        t.text_muted
    );
    assert_eq!(c.cell(crate::base::Point::new(0, 0)).unwrap().1, t.border);
}

#[test]
fn focus_ring_switches_to_border_focus() {
    let t = default_theme().tokens;
    let view = Block::new().focused(true).element(&t);
    let c = draw_into(view, SIZE);
    assert_eq!(
        c.cell(crate::base::Point::new(0, 0)).unwrap().1,
        t.border_focus
    );
}

#[test]
fn fill_paints_interior_and_none_border_draws_nothing() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::None)
        .fill(t.surface)
        .element(&t);
    let c = draw_into(view, SIZE);
    assert_eq!(row(&c, 0).trim(), "");
    assert_eq!(c.cell(crate::base::Point::new(3, 2)).unwrap().2, t.surface);
}

#[test]
fn title_truncates_and_double_heavy_render() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Double)
        .title("A very long title that cannot possibly fit")
        .element(&t);
    let c = draw_into(view, Size::new(12, 3));
    let top = row(&c, 0);
    assert!(top.starts_with('╔') && top.ends_with('╗'), "{top:?}");
    assert!(top.contains(" A very l "), "truncated to the run: {top:?}");

    let view = Block::new().border(BorderKind::Heavy).element(&t);
    let c = draw_into(view, Size::new(4, 2));
    assert_eq!(row(&c, 0), "┏━━┓");
    assert_eq!(row(&c, 1), "┗━━┛");
}

#[test]
fn shadow_strip_lifts_the_panel() {
    let t = default_theme().tokens;
    let view = Block::new()
        .fill(t.surface)
        .shadow(t.shadow_ground)
        .element(&t);
    let c = draw_into(view, Size::new(10, 4));
    // Bottom-right strip wears the pre-composited shadow ground…
    assert_eq!(
        c.cell(crate::base::Point::new(9, 2)).unwrap().2,
        t.shadow_ground
    );
    assert_eq!(
        c.cell(crate::base::Point::new(5, 3)).unwrap().2,
        t.shadow_ground
    );
    // …the offset corner cell (0, bottom) stays untouched…
    assert_ne!(
        c.cell(crate::base::Point::new(0, 3)).unwrap().2,
        t.shadow_ground
    );
    // …and the panel chrome shrank to make room (border at w-2).
    assert_eq!(c.cell(crate::base::Point::new(8, 0)).unwrap().0, '┐');
    // No shadow: chrome spans the full rect.
    let view = Block::new().element(&t);
    let c = draw_into(view, Size::new(10, 4));
    assert_eq!(c.cell(crate::base::Point::new(9, 0)).unwrap().0, '┐');
}

#[test]
fn degenerate_rects_never_panic() {
    let t = default_theme().tokens;
    for size in [
        Size::new(0, 0),
        Size::new(1, 1),
        Size::new(2, 1),
        Size::new(1, 4),
    ] {
        let view = Block::new().title("x").element(&t);
        let _ = draw_into(view, size);
    }
}

#[test]
fn bordered_block_pads_children() {
    let t = default_theme().tokens;
    let el = Block::new().element(&t);
    // The stroke room rides the PROTECTED floor now (RT8-7), not
    // the plain style — mount applies it.
    assert_eq!(el.padding_floor, Some(Edges::all(1)));
    let el = Block::new().border(BorderKind::None).element(&t);
    assert_eq!(el.padding_floor, Some(Edges::ZERO));
}

#[test]
fn rt8_7_user_style_on_block_element_keeps_the_border_inset() {
    // THE cycle-8 first-use trap: `.style(grow)` on the returned
    // Element used to clobber the border padding, dropping content
    // onto the frame. The floor survives it now.
    use crate::base::Size;
    use crate::reactive::create_root;
    use crate::ui::{text, BufferCanvas, UiTree};
    let t = default_theme().tokens;
    let mut tree = UiTree::new(Size::new(12, 4));
    let (root, ()) = create_root(|cx| {
        let view = Block::new()
            .title("Panel")
            .child(text("inner"))
            .element(&t)
            // The newcomer's line, verbatim:
            .style(crate::layout::LayoutStyle::default().grow(1.0))
            .build();
        tree.mount(cx, view);
    });
    let mut canvas = BufferCanvas::new(Size::new(12, 4));
    tree.draw(&mut canvas);
    let top = canvas.row_text(0);
    assert!(
        top.contains("Panel"),
        "title intact on the frame row: {top:?}"
    );
    assert!(
        canvas.row_text(1).contains("inner"),
        "content INSIDE the border, not on it: {:?}",
        canvas.row_text(1)
    );
    assert!(!top.contains("inner"), "content never lands on the frame");
    root.dispose();
}

// --------------------------------------------------------------------
// The close affordance (app-kits 0605). Interactive: through the real
// tree + dispatch.
// --------------------------------------------------------------------

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::Point;
use crate::ui::{Key, MouseButton, MouseKind};

fn fires() -> (Rc<RefCell<u32>>, impl FnMut() + 'static) {
    let n = Rc::new(RefCell::new(0u32));
    let n2 = n.clone();
    (n, move || *n2.borrow_mut() += 1)
}

/// A mounted closable block filling `size` (mounted tests need an
/// explicit layout: an auto-sized root block has no children here).
fn mount_closable(
    size: Size,
    build: impl FnOnce(Block) -> Block,
) -> (
    Rc<RefCell<u32>>,
    crate::reactive::RootScope,
    crate::ui::UiTree,
) {
    let t = default_theme().tokens;
    let (count, on) = fires();
    let (root, mut tree) = mount_widget(size, |_cx| {
        build(
            Block::new()
                .border(BorderKind::Rounded)
                .title("Log")
                .on_close(on)
                .layout(LayoutStyle::default().w(size.w).h(size.h)),
        )
        .element(&t)
        .build()
    });
    // The geometry probe publishes at DRAW time (clicks before the
    // first frame are honestly inert — pinned separately below).
    let _ = render(&mut tree, size);
    (count, root, tree)
}

/// The glyph-research pin (0595 method): `✕` U+2715 is single-width —
/// every run-geometry computation in block.rs assumes it.
#[test]
fn close_glyph_is_single_width() {
    assert_eq!(crate::text::width(CLOSE_GLYPH), 1);
    assert_eq!(crate::text::width(close_text(3)), 3);
    assert_eq!(crate::text::width(close_text(1)), 1);
}

#[test]
fn close_run_renders_padded_at_the_title_rows_right_end() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Rounded)
        .title("Log")
        .on_close(|| {})
        .element(&t);
    let c = draw_into(view, SIZE);
    // w=20: interior 1..=18, run at 16..=18 → `╭ Log ────────── ✕ ╮`.
    assert_eq!(row(&c, 0), format!("╭ Log {} ✕ ╮", "─".repeat(10)));
    // Muted at rest, on the border row, inside the corner.
    let (glyph, ink, _bg) = c.cell(Point::new(17, 0)).unwrap();
    assert_eq!(glyph, '✕');
    assert_eq!(ink, t.text_muted);
    assert_eq!(c.cell(Point::new(19, 0)).unwrap().0, '╮', "corner intact");
}

#[test]
fn truncation_ladder_title_yields_first_then_the_glyph() {
    let t = default_theme().tokens;
    let top_at = |w: i32| {
        let view = Block::new()
            .border(BorderKind::Rounded)
            .title("Log")
            .on_close(|| {})
            .element(&t);
        row(&draw_into(view, Size::new(w, 3)), 0)
    };
    // Wide: full title + padded run.
    assert_eq!(top_at(20), format!("╭ Log {} ✕ ╮", "─".repeat(10)));
    // Tight: the TITLE truncates, the run stands whole.
    assert_eq!(top_at(9), "╭ Lo  ✕ ╮");
    // Tighter: the title is GONE before the run gives one cell.
    assert_eq!(top_at(7), "╭── ✕ ╮");
    assert_eq!(top_at(5), "╭ ✕ ╮");
    // The pads yield next: bare glyph at the last interior cell.
    assert_eq!(top_at(4), "╭─✕╮");
    assert_eq!(top_at(3), "╭✕╮");
    // 1–2 columns: the ✕ yields too — corners only, nothing else.
    assert_eq!(top_at(2), "╭╮");
    for w in [1, 2] {
        assert!(!top_at(w).contains('✕'), "no ✕ at w={w}: {:?}", top_at(w));
    }
}

#[test]
fn right_aligned_title_clamps_left_of_the_run() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Rounded)
        .title("Log")
        .title_align(TitleAlign::Right)
        .on_close(|| {})
        .element(&t);
    let top = row(&draw_into(view, SIZE), 0);
    // The title hugs the run, never rides under it: `── Log  ✕ ╮`.
    assert_eq!(top, format!("╭{} Log  ✕ ╮", "─".repeat(10)));
}

#[test]
fn long_title_never_paints_under_the_close_run() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Rounded)
        .title("A very long discussion pane title")
        .on_close(|| {})
        .element(&t);
    let c = draw_into(view, SIZE);
    let top = row(&c, 0);
    // Run cells (16..=18) carry the affordance, never title chars.
    assert!(top.ends_with(" ✕ ╮"), "{top:?}");
    // Title pads and run pads both stand: `…long d ` + ` ✕ ` + corner.
    assert_eq!(top, "╭ A very long d  ✕ ╮", "truncated to the reserved run");
}

#[test]
fn shadow_close_anchors_to_the_lifted_panel() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Rounded)
        .shadow(t.shadow_ground)
        .on_close(|| {})
        .element(&t);
    let c = draw_into(view, Size::new(12, 4));
    // Panel is w-1 wide: corner at 10, run at 7..=9, strip at 11.
    assert_eq!(c.cell(Point::new(8, 0)).unwrap().0, '✕');
    assert_eq!(c.cell(Point::new(10, 0)).unwrap().0, '╮');
    assert_ne!(c.cell(Point::new(11, 0)).unwrap().0, '✕');
}

#[test]
fn borderless_close_floats_top_right() {
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::None)
        .fill(t.surface)
        .on_close(|| {})
        .element(&t);
    let c = draw_into(view, Size::new(10, 3));
    // No chrome row: the ✕ floats on row 0's right end (inside).
    assert_eq!(c.cell(Point::new(8, 0)).unwrap().0, '✕');
    assert_eq!(row(&c, 0), format!("{}✕ ", " ".repeat(8)));
}

#[test]
fn click_close_fires_once_on_release_inside_only() {
    let size = Size::new(20, 5);
    let (count, _root, mut tree) = mount_closable(size, |b| b);
    // Run = cells 16..=18 on row 0; glyph at 17; corner at 19.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 1, "down+up inside the run = one close");

    // The corner glyph cell is NOT the affordance.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 19, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 19, 0);
    // Neither is the title area of the same row.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 3, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 3, 0);
    // Nor the block body.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 10, 2);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 10, 2);
    assert_eq!(*count.borrow(), 1, "only the run closes");
}

#[test]
fn press_drag_out_release_never_fires_and_unpresses() {
    let size = Size::new(20, 5);
    let (count, _root, mut tree) = mount_closable(size, |b| b);
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 5, 3);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 5, 3);
    assert_eq!(*count.borrow(), 0, "release outside cancels (0.2.20)");
    // No stuck pressed visual: the glyph is back to muted.
    let t = default_theme().tokens;
    let frame = render(&mut tree, size);
    assert_eq!(frame.cell(Point::new(17, 0)).unwrap().1, t.text_muted);
    // The mirror gesture: press OUTSIDE the run, drag IN, release on
    // the ✕ — never fires either (the gesture must START on it).
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 3, 0);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 0, "drag-in never closes");
    // And the next real click still works.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn hover_restyles_to_error_and_leave_restores() {
    let t = default_theme().tokens;
    let size = Size::new(20, 5);
    let (_count, _root, mut tree) = mount_closable(size, |b| b);
    let ink = |tree: &mut crate::ui::UiTree| render(tree, size).cell(Point::new(17, 0)).unwrap().1;
    assert_eq!(ink(&mut tree), t.text_muted, "muted at rest");
    mouse(&mut tree, MouseKind::Move, 17, 0);
    assert_eq!(ink(&mut tree), t.error, "hover wears the danger ink");
    // Within the row but off the run: back to muted.
    mouse(&mut tree, MouseKind::Move, 5, 0);
    assert_eq!(ink(&mut tree), t.text_muted, "run hover is run-precise");
    // Onto the run, then off the block entirely (MouseLeave heals).
    mouse(&mut tree, MouseKind::Move, 17, 0);
    assert_eq!(ink(&mut tree), t.error);
    mouse(&mut tree, MouseKind::Move, 30, 10);
    assert_eq!(ink(&mut tree), t.text_muted, "leave clears the hover");
}

#[test]
fn pressed_close_wears_bold_danger() {
    let t = default_theme().tokens;
    let size = Size::new(20, 5);
    let (_count, _root, mut tree) = mount_closable(size, |b| b);
    mouse(&mut tree, MouseKind::Move, 17, 0);
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    let frame = render(&mut tree, size);
    assert_eq!(frame.cell(Point::new(17, 0)).unwrap().1, t.error);
    assert!(
        frame
            .attrs_at(Point::new(17, 0))
            .contains(crate::render::Attrs::BOLD),
        "press = danger ink + BOLD"
    );
}

#[test]
fn click_before_first_draw_is_inert() {
    // The geometry probe publishes at draw time; before anything is
    // visible there is nothing honest to click.
    let t = default_theme().tokens;
    let size = Size::new(20, 5);
    let (count, on) = fires();
    let (_root, mut tree) = mount_widget(size, |_cx| {
        Block::new()
            .title("Log")
            .on_close(on)
            .layout(LayoutStyle::default().w(size.w).h(size.h))
            .element(&t)
            .build()
    });
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 0, "nothing visible, nothing closable");
    let _ = render(&mut tree, size);
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 1, "after the first frame it works");
}

/// The fusion pin: a crushed closable block paints NOTHING outside
/// itself — no phantom ✕ row over the sibling that owns the cells now.
#[test]
fn crushed_block_paints_no_phantom_close_row() {
    let t = default_theme().tokens;
    let size = Size::new(20, 4);
    let (_root, mut tree) = mount_widget(size, |_cx| {
        crate::ui::Element::new()
            .style(LayoutStyle::column().w(size.w).h(size.h))
            .child(
                // Crushed to zero height.
                Block::new()
                    .title("dead")
                    .on_close(|| {})
                    .layout(LayoutStyle::default().w(20).h(0))
                    .element(&t)
                    .build(),
            )
            .child(crate::ui::text("below content"))
            .build()
    });
    let frame = render(&mut tree, size);
    assert!(
        frame.row_text(0).contains("below content"),
        "the sibling owns row 0: {:?}",
        frame.row_text(0)
    );
    for y in 0..size.h {
        assert!(
            !frame.row_text(y).contains('✕'),
            "no phantom ✕ anywhere: row {y} = {:?}",
            frame.row_text(y)
        );
    }
    // And the crushed block's ✕ is unhittable (the descent never
    // enters a zero-area rect).
    let hit = tree.hit_test(Point::new(17, 0));
    assert!(hit.is_some(), "the sibling text is hit instead");
}

/// The hovered-then-crushed variant (self-attack): a stale hot state
/// must not paint through a collapse — the probe publishes the empty
/// panel even though the block's own paint is skipped.
#[test]
fn hover_then_crush_paints_no_phantom_row() {
    let t = default_theme().tokens;
    let size = Size::new(20, 4);
    let h = std::rc::Rc::new(std::cell::Cell::new(3i32));
    let h2 = h.clone();
    let hsig: std::rc::Rc<std::cell::RefCell<Option<crate::reactive::Signal<i32>>>> =
        Default::default();
    let hsig2 = hsig.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let hh = cx.signal(h2.get());
        *hsig2.borrow_mut() = Some(hh);
        crate::ui::Element::new()
            .style(LayoutStyle::column().w(size.w).h(size.h))
            .child(
                Block::new()
                    .title("pane")
                    .on_close(|| {})
                    .element(&t)
                    .style_signal(move || LayoutStyle::default().w(20).h(hh.get()))
                    .build(),
            )
            .child(crate::ui::text("below content"))
            .build()
    });
    let _ = render(&mut tree, size);
    // Hover the live ✕ (block h=3: run at 16..=18 row 0).
    mouse(&mut tree, MouseKind::Move, 17, 0);
    let frame = render(&mut tree, size);
    assert_eq!(frame.cell(Point::new(17, 0)).unwrap().1, t.error);
    // Crush WITHOUT any mouse motion.
    hsig.borrow().unwrap().set(0);
    crate::reactive::flush_effects();
    let frame = render(&mut tree, size);
    for y in 0..size.h {
        assert!(
            !frame.row_text(y).contains('✕'),
            "stale hot never paints a phantom row: {y} = {:?}",
            frame.row_text(y)
        );
    }
}

/// 0297 disposal law: `on_close` may synchronously dispose the block's
/// scope — the operator's exact usage (the callback removes the panel).
#[test]
fn on_close_may_dispose_the_blocks_scope() {
    let t = default_theme().tokens;
    let size = Size::new(20, 5);
    let count = Rc::new(RefCell::new(0u32));
    let mut tree = crate::ui::UiTree::new(size);
    let (root, ()) = crate::reactive::create_root(|cx| {
        let panel_cx = cx.child();
        let c = count.clone();
        let view = Block::new()
            .title("Log")
            .on_close(move || {
                *c.borrow_mut() += 1;
                panel_cx.dispose();
            })
            .layout(LayoutStyle::default().w(size.w).h(size.h))
            .element(&t)
            .build();
        tree.mount(panel_cx, view);
    });
    tree.layout();
    let mut canvas = crate::ui::BufferCanvas::new(size);
    tree.draw(&mut canvas);
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 1, "the close fired");
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    // The dead panel's callback can never re-fire (the disposal pin).
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 17, 0);
    assert_eq!(*count.borrow(), 1);
    root.dispose();
}

#[test]
fn close_affordance_reports_an_access_button_never_focusable() {
    let size = Size::new(20, 5);
    let (_count, _root, mut tree) = mount_closable(size, |b| b);
    let a11y = tree.accessibility_tree_text();
    assert!(
        a11y.contains("button \"Close Log\""),
        "honest a11y surface: {a11y}"
    );
    // Never focusable: Tab finds nothing (the 0.2.12 P1).
    crate::widgets::itest_util::key(&mut tree, Key::Tab);
    assert_eq!(tree.focused(), None, "chrome never joins the tab order");
}

#[test]
fn plain_block_render_is_unchanged_by_the_feature() {
    // No `on_close` = no reservation, no run, byte-identical title
    // math (title_end == x1 keeps avail = w-4).
    let t = default_theme().tokens;
    let view = Block::new()
        .border(BorderKind::Rounded)
        .title("Log")
        .element(&t);
    let c = draw_into(view, SIZE);
    assert_eq!(row(&c, 0), format!("╭ Log {}╮", "─".repeat(13)));
    assert!(!row(&c, 0).contains('✕'));
}

#[test]
fn close_run_geometry_ladder() {
    // The shared helper is the ONE geometry owner — pin its ladder.
    let r = |w, h| Rect::new(0, 0, w, h);
    // Bordered: padded at interior ≥ 3, bare at 1–2, gone below.
    assert_eq!(close_run(r(20, 3), true), Some(Rect::new(16, 0, 3, 1)));
    assert_eq!(close_run(r(5, 3), true), Some(Rect::new(1, 0, 3, 1)));
    assert_eq!(close_run(r(4, 3), true), Some(Rect::new(2, 0, 1, 1)));
    assert_eq!(close_run(r(3, 3), true), Some(Rect::new(1, 0, 1, 1)));
    assert_eq!(close_run(r(2, 3), true), None);
    assert_eq!(close_run(r(1, 3), true), None);
    // No rows, no affordance — the h=0 fusion pin.
    assert_eq!(close_run(r(20, 0), true), None);
    // Borderless: the ladder rides the full width.
    assert_eq!(close_run(r(10, 3), false), Some(Rect::new(7, 0, 3, 1)));
    assert_eq!(close_run(r(2, 3), false), Some(Rect::new(1, 0, 1, 1)));
    assert_eq!(close_run(r(0, 3), false), None);
}

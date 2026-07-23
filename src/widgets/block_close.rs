//! Block's close affordance internals — private shipped sibling of
//! block.rs (file-size discipline, the feed_typeset.rs pattern):
//! the run geometry (ONE owner for paint + hit), and the interactive
//! overlay child. Design rulings in `docs/backlog/completed/app-kits/
//! 0605_block_close_affordance.md`; user-facing contract in the
//! [block module docs](super::block) and docs/api.md.
//!
//! OWNER: DESIGN (block chrome; the interactive child follows REACT's
//! Button conventions).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::base::{Point, Rect, Rgba};
use crate::layout::{Dimension, Inset, Style as LayoutStyle};
use crate::ui::{
    dyn_view, dyn_view_scoped, Element, EventCtx, MouseButton, MouseKind, Phase, Role, UiEvent,
    View,
};

/// The close glyph, `✕` U+2715 (the drawer's spelling): East-Asian-
/// NARROW and absent from emoji-data — single-width in every terminal
/// convention. `×` U+00D7 is East-Asian-AMBIGUOUS (double-width under
/// ambiguous-wide settings) and was rejected — the 0595 glyph-research
/// method. Width 1 is test-pinned.
pub(super) const CLOSE_GLYPH: &str = "✕";

/// The panel box after the shadow strip (the lifted chrome's true rect).
pub(super) fn panel_rect(rect: Rect, shadow: bool) -> Rect {
    if shadow {
        Rect::new(rect.x, rect.y, (rect.w - 1).max(0), (rect.h - 1).max(0))
    } else {
        rect
    }
}

/// The affordance's painted text for a run of `w` cells: padded when
/// the run is the full 3 cells, the bare glyph otherwise.
pub(super) fn close_text(w: i32) -> &'static str {
    if w >= 3 {
        " ✕ "
    } else {
        CLOSE_GLYPH
    }
}

/// The close affordance's cell run on `panel`'s title row, or `None`
/// when there is no honest room. Ladder (the pinned truncation order —
/// the TITLE yields first, this run only yields to the frame itself):
/// interior ≥ 3 → padded ` ✕ ` (3 cells, a forgiving click target);
/// interior 1–2 → the bare glyph at the last interior cell; interior
/// ≤ 0 (bordered w ≤ 2) or no rows → nothing. Never a cell on or
/// outside the corners — the fusion law.
pub(super) fn close_run(panel: Rect, bordered: bool) -> Option<Rect> {
    if panel.h < 1 || panel.w < 1 {
        return None;
    }
    let (iw, end) = if bordered {
        (panel.w - 2, panel.right() - 1)
    } else {
        (panel.w, panel.right())
    };
    if iw >= 3 {
        Some(Rect::new(end - 3, panel.y, 3, 1))
    } else if iw >= 1 {
        Some(Rect::new(end - 1, panel.y, 1, 1))
    } else {
        None
    }
}

/// The interactive half of the close affordance: an ABSOLUTE overlay
/// spanning the panel's title row — out of flow, so a closable block's
/// children lay out byte-identically to a plain one, and its width
/// derives from the interior (a crushed block solves it to zero cells:
/// unhittable and unpaintable by geometry, not by guards). All rect
/// math reads the root draw's probe cell, never this child's own rect.
///
/// Built as a `dyn_view_scoped` with NO tracked reads: the closure runs
/// once and its generation scope owns the hover/pressed signals for the
/// block's whole life — how a widget built without a `Scope` parameter
/// (`element(&t)`) gains contained reactivity. The inner restyle
/// `dyn_view` is the only reactive region (1 row) and builds an EMPTY
/// element at rest — no draw closure, no idle cost.
pub(super) fn close_child(
    geom: Rc<Cell<Rect>>,
    bordered: bool,
    title: Option<String>,
    hot_ink: Rgba,
    on_close: Box<dyn FnMut()>,
) -> View {
    let label = match title {
        Some(t) => format!("Close {t}"),
        None => String::from("Close panel"),
    };
    // Shareable FnMut: the generation closure is FnMut by type (its
    // captures must survive the contract even though it runs once) and
    // the Up handler needs the callback too — the SharedCallback
    // dispatch-only-slot contract applies (held borrow across `f`).
    let mut f = on_close;
    let cb: crate::widgets::SharedCallback<()> = Rc::new(RefCell::new(Some(
        Box::new(move |()| f()) as Box<dyn FnMut(())>,
    )));
    let row = LayoutStyle::default()
        .absolute(Inset {
            left: Some(0),
            right: Some(0),
            // The border row sits one cell ABOVE the content box; a
            // borderless block has no chrome row, so the ✕ floats on
            // its first content row (never above the block).
            top: Some(if bordered { -1 } else { 0 }),
            bottom: None,
        })
        .height(Dimension::Cells(1));
    dyn_view_scoped(row, move |gcx| {
        let hot = gcx.signal(false);
        let pressed = gcx.signal(false);
        let geom_h = geom.clone();
        let cb_h = cb.clone();
        let label = label.clone();
        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            match ev {
                UiEvent::Mouse(m) => {
                    let run = close_run(geom_h.get(), bordered);
                    let inside = run.is_some_and(|r| r.contains(m.pos));
                    match m.kind {
                        // `set_if_changed`: pointer traffic across the
                        // title row must not damage anything unless the
                        // hot state actually flips.
                        MouseKind::Move => {
                            hot.set_if_changed(inside);
                        }
                        MouseKind::Down(MouseButton::Left) if inside => {
                            pressed.set(true);
                            ctx.stop_propagation();
                            // Outside the run (title area): not ours —
                            // the press bubbles on to the block/app.
                        }
                        MouseKind::Up(MouseButton::Left) if pressed.get_untracked() => {
                            // Release-inside decides (the Button 0.2.20
                            // convention; hover is frozen under capture,
                            // the rect check is the truthful inside-ness
                            // test). ALL bookkeeping lands BEFORE the
                            // callback (0297): `on_close` may dispose
                            // this very subtree.
                            pressed.set(false);
                            ctx.stop_propagation();
                            if inside {
                                if let Some(f) = cb_h.borrow_mut().as_mut() {
                                    f(());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Delivered when the pointer leaves this row's subtree —
                // the self-healing half of the hover state.
                UiEvent::MouseLeave => {
                    hot.set_if_changed(false);
                }
                _ => {}
            }
        };
        let geom_d = geom.clone();
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
            )
            // Honest a11y for a mouse-only control (the drawer ✕
            // precedent): a Button by role, NEVER focusable — a
            // focusable ✕ steals the panel's first focus from its
            // content (the 0.2.12 P1).
            .role(Role::Button)
            .access_label(label)
            .on(Phase::Bubble, handler)
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
                move || {
                    // Read BOTH signals unconditionally: `||` would
                    // short-circuit the `pressed` read while hot and
                    // untrack it — a press would then never restyle.
                    let hot_now = hot.get();
                    let pressed_now = pressed.get();
                    if !hot_now && !pressed_now {
                        // At rest the root's muted ✕ shows through —
                        // this region carries no draw closure at all.
                        return Element::new().build();
                    }
                    let geom = geom_d.clone();
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(1)),
                        )
                        .draw(move |canvas, _rect| {
                            // Geometry from the probe at DRAW time (the
                            // root paints first in every pass, so this
                            // is the same panel the frame shows — a
                            // resize can never strand a hot ✕ at its
                            // old cells).
                            let Some(run) = close_run(geom.get(), bordered) else {
                                return;
                            };
                            let mut style = crate::render::Style::new().fg(hot_ink);
                            if pressed_now {
                                style = style.attrs(crate::render::Attrs::BOLD);
                            }
                            canvas.print_styled(
                                Point::new(run.x, run.y),
                                close_text(run.w),
                                &style,
                            );
                        })
                        .build()
                },
            ))
            .build()
    })
}

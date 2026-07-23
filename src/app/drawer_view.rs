//! Drawer panel chrome (private sibling of drawer.rs, the
//! anchored_owned.rs file-split pattern): the OPAQUE themed panel with
//! its leading-edge hairline, the optional header row (title + muted
//! Esc hint + ✕ close affordance), and the substrate-owned Escape
//! handler. Tokens resolve from the ACTIVE theme at open time (the
//! Modal rule — a mid-open theme switch lands at the next open).
//!
//! The panel ground is `surface` and is OPAQUE by token contract — the
//! entity-app drawer's documented lesson ("page text bled straight
//! through the assistant drawer") made overlay-panel opacity a rule,
//! not a taste.
//!
//! OWNER: DRAWER (0585).

use crate::base::{Point, Rect};
use crate::layout::{Dimension, Edges, Style as LayoutStyle};
use crate::render::Style;
use crate::ui::{Element, Key, Mods, MouseButton, MouseKind, Phase, Role, UiEvent, View};

use super::drawer::{DrawerCloseReason, DrawerConfig, DrawerEdge, DrawerFocus, DrawerHandle};
use super::theme::current_theme;

/// Wrap `content` in the drawer panel: fill + hairline + optional
/// header + Escape handling. `handle` powers the Esc and ✕ paths.
pub(super) fn panel_view(cfg: &DrawerConfig, handle: DrawerHandle, content: View) -> View {
    let tokens = &current_theme().tokens;
    let ground = tokens.surface;
    let ink = tokens.text;
    let hairline = tokens.border;
    let muted = tokens.text_muted;

    let edge = cfg.edge;
    let modal = cfg.focus == DrawerFocus::Modal;
    // Reserve the hairline's column/row through padding so content can
    // never overwrite it; one breathing cell on the other sides.
    let pad = match edge {
        DrawerEdge::Right => Edges {
            left: 2,
            right: 1,
            top: 0,
            bottom: 0,
        },
        DrawerEdge::Left => Edges {
            left: 1,
            right: 2,
            top: 0,
            bottom: 0,
        },
        DrawerEdge::Top => Edges {
            left: 1,
            right: 1,
            top: 0,
            bottom: 1,
        },
        DrawerEdge::Bottom => Edges {
            left: 1,
            right: 1,
            top: 1,
            bottom: 0,
        },
    };

    let mut root = Element::new()
        .style(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0))
                .padding(pad),
        )
        .role(if modal { Role::Dialog } else { Role::Region })
        .access_label(cfg.title.clone().unwrap_or_else(|| "drawer".into()))
        .draw(move |canvas, rect| {
            let fill = Style::new().fg(ink).bg(ground);
            canvas.fill_styled(rect, ' ', &fill);
            let line = Style::new().fg(hairline).bg(ground);
            match edge {
                // The hairline sits on the panel's LEADING edge (the
                // side facing the page).
                DrawerEdge::Right => {
                    canvas.fill_styled(Rect::new(rect.x, rect.y, 1, rect.h), '│', &line)
                }
                DrawerEdge::Left => {
                    canvas.fill_styled(Rect::new(rect.right() - 1, rect.y, 1, rect.h), '│', &line)
                }
                DrawerEdge::Top => {
                    canvas.fill_styled(Rect::new(rect.x, rect.bottom() - 1, rect.w, 1), '─', &line)
                }
                DrawerEdge::Bottom => {
                    canvas.fill_styled(Rect::new(rect.x, rect.y, rect.w, 1), '─', &line)
                }
            }
        });
    if modal {
        root = root.focus_trap();
    } else {
        // Passive: the panel root is the click-to-focus target — a
        // press anywhere inside gives the panel the keyboard (the
        // engine's focused-overlay key rule); until then keys stay
        // with the main surface.
        root = root.focusable();
    }
    // Substrate-owned Escape (the Popup rule): fires for any key the
    // content left unconsumed. In Passive mode keys only reach this
    // tree while it holds focus — Esc closes a panel you are IN.
    {
        let handle = handle.clone();
        root = root.on(Phase::Bubble, move |ctx, ev| {
            if let UiEvent::Key(k) = ev {
                if k.key == Key::Escape && k.mods == Mods::NONE {
                    handle.close_with(DrawerCloseReason::Escape);
                    ctx.stop_propagation();
                }
            }
        });
    }
    if let Some(title) = &cfg.title {
        root = root.child(header_row(title.clone(), modal, ink, muted, ground, handle));
    }
    root = root.child(
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .grow(1.0),
            )
            .child(content)
            .build(),
    );
    root.build()
}

/// One pinned header row: title left, muted Esc hint + ✕ close
/// affordance right. The ✕ activates on a left press ONLY (a close
/// affordance, not a full Button — the drawer stays free of widget
/// imports, the popups.rs layering); Esc is the keyboard close. It is
/// deliberately NOT focusable — see the review2-F2 note at the
/// element below.
fn header_row(
    title: String,
    modal: bool,
    ink: crate::base::Rgba,
    muted: crate::base::Rgba,
    ground: crate::base::Rgba,
    handle: DrawerHandle,
) -> View {
    let title_el = Element::new()
        .style(LayoutStyle::default().grow(1.0).height(Dimension::Cells(1)))
        .draw(move |canvas, rect| {
            let style = Style::new().fg(ink).bg(ground).bold();
            canvas.print_styled(Point::new(rect.x, rect.y), &title, &style);
        })
        .build();
    let hint = modal.then(|| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(4))
                    .height(Dimension::Cells(1))
                    .shrink(0.0),
            )
            .draw(move |canvas, rect| {
                let style = Style::new().fg(muted).bg(ground);
                canvas.print_styled(Point::new(rect.x, rect.y), "esc ", &style);
            })
            .build()
    });
    let close = {
        let fire = move || handle.close_with(DrawerCloseReason::Api);
        // MOUSE-ONLY affordance, deliberately NOT focusable (review2
        // F2, drawer-on-tabs): a focusable ✕ was the panel's FIRST
        // focusable, so a modal drawer's focus_init landed on the
        // header instead of the CONTENT — a hosted PageHost's chord
        // interceptor sat off the key path and nested navigation was
        // dead until a click. Chrome must never steal initial focus
        // from the page it frames; Esc stays the keyboard close.
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(1))
                    .height(Dimension::Cells(1))
                    .shrink(0.0),
            )
            .role(Role::Button)
            .access_label("Close drawer (esc)")
            .draw(move |canvas, rect| {
                let style = Style::new().fg(muted).bg(ground);
                canvas.print_styled(Point::new(rect.x, rect.y), "✕", &style);
            })
            .on(Phase::Target, move |ctx, ev| {
                if let UiEvent::Mouse(m) = ev {
                    if matches!(m.kind, MouseKind::Down(MouseButton::Left)) {
                        fire();
                        ctx.stop_propagation();
                    }
                }
            })
            .build()
    };
    let mut row = Element::new().style(
        LayoutStyle::row()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Cells(1))
            .shrink(0.0),
    );
    row = row.child(title_el);
    if let Some(hint) = hint {
        row = row.child(hint);
    }
    row.child(close).build()
}

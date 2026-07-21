//! Scroll: a generic clipped viewport over oversized MOUNTED content.
//!
//! The content is mounted ONCE (widget state inside survives scrolling);
//! offsets drive a reactive layout style (`Element::style_signal`) that
//! repositions the content wrapper with negative absolute insets — no
//! remount, real solved rects, so hit testing and focus inside scrolled
//! content keep working. The viewport clips via layout's
//! `clip_overflow`; scrolled-away children are neither painted nor
//! hit-testable (tree-level guarantees).
//!
//! v1 HONESTY: the content extent comes from `content_size(w, h)` — an
//! explicit hint, because handlers have no layout-query surface yet.
//! When one lands, the hint becomes optional and defaults to measured
//! content (request filed).
//!
//! Wheel scrolls vertically (horizontal wheel scrolls x); arrows/PgUp/
//! PgDn/Home/End work while focused; the scrollbar thumb drags with
//! pointer capture (mouse-down auto-captures, so drags keep steering the
//! thumb after the pointer leaves it).
//!
//! OWNER: REACT.

use crate::base::Rect;
use crate::layout::{Dimension, Inset, Position, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent, View};

use super::list::draw_scrollbar;

pub struct Scroll {
    content: View,
    content_size: (i32, i32),
    vertical: bool,
    horizontal: bool,
    offset_y: Option<Signal<i32>>,
    offset_x: Option<Signal<i32>>,
    layout: Option<LayoutStyle>,
}

impl Scroll {
    pub fn new(content: View) -> Scroll {
        Scroll {
            content,
            content_size: (0, 0),
            vertical: true,
            horizontal: false,
            offset_y: None,
            offset_x: None,
            layout: None,
        }
    }

    /// The content's full extent in cells (see module honesty note).
    pub fn content_size(mut self, w: i32, h: i32) -> Scroll {
        self.content_size = (w, h);
        self
    }

    pub fn axes(mut self, horizontal: bool, vertical: bool) -> Scroll {
        self.horizontal = horizontal;
        self.vertical = vertical;
        self
    }

    /// Bind external offset signals (dashboards syncing panes).
    pub fn offset_y(mut self, sig: Signal<i32>) -> Scroll {
        self.offset_y = Some(sig);
        self
    }

    pub fn offset_x(mut self, sig: Signal<i32>) -> Scroll {
        self.offset_x = Some(sig);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Scroll {
        self.layout = Some(layout);
        self
    }

    /// Canonical one-call build (cycle 8): tokens resolve from the
    /// app's THEME CONTEXT (a tracked read — building inside a
    /// `dyn_view` re-renders on theme switch) and the finished `View`
    /// comes back ready for `.child(..)`. Use `element(cx, &tokens)`
    /// when you need explicit theming or extra Element customization.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let track = t.border;
        let thumb = t.text_muted;
        let ground = t.surface;

        let (content_w, content_h) = self.content_size;
        let ox = self.offset_x.unwrap_or_else(|| cx.signal(0i32));
        let oy = self.offset_y.unwrap_or_else(|| cx.signal(0i32));
        let (vertical, horizontal) = (self.vertical, self.horizontal);
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));

        // The mounted-once content wrapper: negative insets = scrolling.
        // Explicit size so absolute layout never consults intrinsics for
        // huge content.
        let wrapper = Element::new()
            .style_signal(move || LayoutStyle {
                position: Position::Absolute,
                inset: Inset {
                    left: Some(-ox.get()),
                    top: Some(-oy.get()),
                    right: None,
                    bottom: None,
                },
                width: Dimension::Cells(content_w.max(1)),
                height: Dimension::Cells(content_h.max(1)),
                ..LayoutStyle::default()
            })
            .child(self.content);

        let viewport = Element::new()
            .style(
                LayoutStyle::default()
                    .grow(1.0)
                    // Scroll (not just Clip): scrolled-away content
                    // neither paints nor hits, AND the node advertises
                    // itself to wheel routing / ensure-visible.
                    .scroll(),
            )
            .role(crate::ui::Role::ScrollArea)
            .child(wrapper.build());

        let scroll_by = move |dx: i32, dy: i32, view: Rect| {
            if horizontal && dx != 0 {
                ox.update(|o| *o = (*o + dx).clamp(0, (content_w - view.w).max(0)));
            }
            if vertical && dy != 0 {
                oy.update(|o| *o = (*o + dy).clamp(0, (content_h - view.h).max(0)));
            }
        };

        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let rect = ctx.current_rect();
            match ev {
                UiEvent::Mouse(m) => {
                    let (dx, dy) = match m.kind {
                        MouseKind::ScrollUp => (0, -3),
                        MouseKind::ScrollDown => (0, 3),
                        MouseKind::ScrollLeft => (-3, 0),
                        MouseKind::ScrollRight => (3, 0),
                        _ => (0, 0),
                    };
                    if dx != 0 || dy != 0 {
                        scroll_by(dx, dy, rect);
                        ctx.stop_propagation();
                    }
                }
                UiEvent::Key(k) => {
                    let (dx, dy) = match k.key {
                        Key::Up => (0, -1),
                        Key::Down => (0, 1),
                        Key::Left => (-1, 0),
                        Key::Right => (1, 0),
                        Key::PageUp => (0, -rect.h.max(1)),
                        Key::PageDown => (0, rect.h.max(1)),
                        Key::Home => (0, -content_h),
                        Key::End => (0, content_h),
                        _ => return,
                    };
                    scroll_by(dx, dy, rect);
                    ctx.stop_propagation();
                }
                _ => {}
            }
        };

        // Scrollbar: its own Dyn column so offset changes damage exactly
        // this strip; drag maps pointer y to offset with capture keeping
        // the drag alive outside the strip.
        let bar = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Cells(1))
                .height(Dimension::Percent(1.0)),
            move || {
                let offset = oy.get();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Cells(1))
                            .height(Dimension::Percent(1.0)),
                    )
                    .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                        if let UiEvent::Mouse(m) = ev {
                            let grabbed = matches!(
                                m.kind,
                                MouseKind::Down(MouseButton::Left)
                                    | MouseKind::Drag(MouseButton::Left)
                            );
                            if grabbed {
                                let bar = ctx.current_rect();
                                let usable = (bar.h - 1).max(1);
                                let frac = (m.pos.y - bar.y).clamp(0, usable);
                                let max_off = (content_h - bar.h).max(0);
                                oy.set((frac * max_off) / usable);
                                ctx.stop_propagation();
                            }
                        }
                    })
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        draw_scrollbar(canvas, rect, offset, content_h, track, thumb, ground);
                    })
                    .build()
            },
        );

        let mut root = Element::new()
            .style(layout)
            .focusable()
            .on(Phase::Bubble, handler)
            .child(viewport.build());
        if vertical {
            root = root.child(bar);
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::layout::Style as LayoutStyle;
    use crate::theme::default_theme;
    use crate::ui::{text, Element, Key, MouseButton, MouseKind};
    use crate::widgets::itest_util::{key, mount_widget, mouse, render};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 1-column-wide content: 20 numbered rows.
    fn tall_content() -> (View, i32) {
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..20 {
            col = col.child(text(format!("row {i}")));
        }
        (col.build(), 20)
    }

    #[test]
    fn wheel_and_keys_scroll_and_clip() {
        let t = &default_theme().tokens;
        let size = Size::new(12, 4);
        let (content, h) = tall_content();
        let (_root, mut tree) = mount_widget(size, |cx| {
            Scroll::new(content)
                .content_size(10, h)
                .element(cx, t)
                .build()
        });
        let canvas = render(&mut tree, size);
        assert!(canvas.row_text(0).starts_with("row 0"));
        assert!(!canvas.row_text(3).contains("row 7"), "clipped to viewport");
        mouse(&mut tree, MouseKind::ScrollDown, 2, 1); // +3
        let canvas = render(&mut tree, size);
        assert!(
            canvas.row_text(0).starts_with("row 3"),
            "{:?}",
            canvas.row_text(0)
        );
        key(&mut tree, Key::Tab);
        key(&mut tree, Key::Down); // +1
        let canvas = render(&mut tree, size);
        assert!(canvas.row_text(0).starts_with("row 4"));
        key(&mut tree, Key::End);
        let canvas = render(&mut tree, size);
        assert!(
            canvas.row_text(3).starts_with("row 19"),
            "clamped to bottom"
        );
    }

    #[test]
    fn scrolled_away_content_is_not_hit_testable() {
        let t = &default_theme().tokens;
        let size = Size::new(12, 4);
        let (content, h) = tall_content();
        let (_root, mut tree) = mount_widget(size, |cx| {
            Scroll::new(content)
                .content_size(10, h)
                .element(cx, t)
                .build()
        });
        mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
        tree.layout();
        // "row 0"'s text instance now sits ABOVE the viewport (negative
        // y). A hit at (2, 0) must resolve inside the visible content,
        // never to a node whose solved rect is scrolled out.
        let hit = tree.hit_test(crate::base::Point::new(2, 0)).expect("hit");
        let r = tree.rect_of(hit);
        assert!(r.y >= 0, "hit a scrolled-away instance at {r:?}");
    }

    #[test]
    fn nested_scrolls_route_the_wheel_to_the_nearest() {
        // RT3-4's shape: an inner scroll inside an outer scroll's content.
        // A wheel over the inner must move ONLY the inner offset.
        let t = &default_theme().tokens;
        let size = Size::new(40, 14);
        type OffsetPair = (crate::reactive::Signal<i32>, crate::reactive::Signal<i32>);
        let holders: Rc<RefCell<Option<OffsetPair>>> = Rc::new(RefCell::new(None));
        let h2 = holders.clone();
        let (_root, mut tree) = mount_widget(size, move |cx| {
            let outer_y = cx.signal(0i32);
            let inner_y = cx.signal(0i32);
            *h2.borrow_mut() = Some((outer_y, inner_y));
            let (inner_content, _) = tall_content();
            let inner = Scroll::new(inner_content)
                .content_size(30, 50)
                .offset_y(inner_y)
                .layout(
                    LayoutStyle::default()
                        .width(crate::layout::Dimension::Cells(34))
                        .height(crate::layout::Dimension::Cells(6)),
                )
                .element(cx, t)
                .build();
            let (outer_rows, _) = tall_content();
            let content = Element::new()
                .style(LayoutStyle::column())
                .child(inner)
                .child(outer_rows)
                .build();
            Scroll::new(content)
                .content_size(36, 100)
                .offset_y(outer_y)
                .layout(LayoutStyle::default().grow(1.0))
                .element(cx, t)
                .build()
        });
        tree.layout();
        let (outer_y, inner_y) = holders.borrow().expect("signals");
        mouse(&mut tree, MouseKind::Move, 5, 2);
        mouse(&mut tree, MouseKind::ScrollDown, 5, 2);
        assert!(
            inner_y.get_untracked() > 0,
            "inner consumes: {}",
            inner_y.get_untracked()
        );
        assert_eq!(outer_y.get_untracked(), 0, "outer must not double-scroll");
        // Below the inner widget: the outer takes it.
        let inner_before = inner_y.get_untracked();
        mouse(&mut tree, MouseKind::Move, 5, 12);
        mouse(&mut tree, MouseKind::ScrollDown, 5, 12);
        assert!(
            outer_y.get_untracked() > 0,
            "outer takes the wheel outside inner"
        );
        assert_eq!(inner_y.get_untracked(), inner_before);
    }

    #[test]
    fn scrollbar_drag_jumps_the_offset() {
        let t = &default_theme().tokens;
        let size = Size::new(12, 4);
        let (content, h) = tall_content();
        let (_root, mut tree) = mount_widget(size, |cx| {
            Scroll::new(content)
                .content_size(10, h)
                .element(cx, t)
                .build()
        });
        // The bar is the last column; drag the thumb to the bottom.
        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 11, 0);
        mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 11, 3);
        mouse(&mut tree, MouseKind::Up(MouseButton::Left), 11, 3);
        let canvas = render(&mut tree, size);
        assert!(
            canvas.row_text(0).starts_with("row 16"),
            "drag to bottom = max offset: {:?}",
            canvas.row_text(0)
        );
    }
}

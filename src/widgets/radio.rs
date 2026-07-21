//! RadioGroup: one-of-N choice bound to a `Signal<usize>`.
//!
//! One focusable element for the WHOLE group (a radio group is one tab
//! stop, per every platform HIG): Up/Down move the selection while
//! focused, click selects a row directly. `(•)` marks the selection;
//! focus renders the selection pair on the active row only.
//!
//! ```ignore
//! let pick = cx.signal(0usize);
//! RadioGroup::of(["Small", "Large"])
//!     .selection(pick)
//!     .element(cx, &theme.tokens)
//! ```
//!
//! OWNER: REACT.

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent};

pub struct RadioGroup {
    items: Vec<String>,
    selection: Option<Signal<usize>>,
    layout: Option<LayoutStyle>,
    on_change: Option<Box<dyn FnMut(usize)>>,
}

impl RadioGroup {
    /// Ergonomic constructor: anything iterable into strings —
    /// `RadioGroup::of(["s", "m", "l"])`. (`new` keeps `Vec<String>`
    /// so existing `.collect()` call sites stay inferable.)
    pub fn of<I, S>(items: I) -> RadioGroup
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        RadioGroup::new(items.into_iter().map(Into::into).collect())
    }

    pub fn new(items: Vec<String>) -> RadioGroup {
        RadioGroup {
            items,
            selection: None,
            layout: None,
            on_change: None,
        }
    }

    pub fn selection(mut self, selection: Signal<usize>) -> RadioGroup {
        self.selection = Some(selection);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> RadioGroup {
        self.layout = Some(layout);
        self
    }

    pub fn on_change(mut self, f: impl FnMut(usize) + 'static) -> RadioGroup {
        self.on_change = Some(Box::new(f));
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
        let text_fg = t.text;
        let ground = t.surface;
        let accent = t.accent;
        let sel_fg = t.selection_fg;
        let sel_bg = t.selection_bg;

        let items = std::rc::Rc::new(self.items);
        let len = items.len();
        let selection = self.selection.unwrap_or_else(|| cx.signal(0usize));
        let on_change: crate::widgets::SharedCallback<usize> =
            std::rc::Rc::new(std::cell::RefCell::new(self.on_change));
        let width = items
            .iter()
            .map(|s| crate::text::width(s))
            .max()
            .unwrap_or(0)
            + 4;
        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(len as i32))
        });

        let focused = cx.signal(false);
        let pick = {
            let on_change = on_change.clone();
            move |target: usize| {
                let target = target.min(len.saturating_sub(1));
                if selection.get_untracked() != target {
                    selection.set(target);
                    if let Some(f) = on_change.borrow_mut().as_mut() {
                        f(target);
                    }
                }
            }
        };
        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| match ev {
            UiEvent::Key(k) => {
                if !focused.get_untracked() {
                    return;
                }
                let cur = selection.get_untracked();
                let target = match k.key {
                    Key::Up | Key::Left => cur.saturating_sub(1),
                    Key::Down | Key::Right => cur + 1,
                    Key::Home => 0,
                    Key::End => len.saturating_sub(1),
                    _ => return,
                };
                pick(target);
                ctx.stop_propagation();
            }
            UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)) => {
                let row = m.pos.y - ctx.current_rect().y;
                if row >= 0 && (row as usize) < len {
                    pick(row as usize);
                }
                ctx.stop_propagation();
            }
            _ => {}
        };

        let access_items = items.clone();
        Element::new()
            .style(layout)
            .role(crate::ui::Role::RadioGroup)
            .access_value(move || {
                access_items
                    .get(selection.get_untracked())
                    .cloned()
                    .unwrap_or_default()
            })
            .focusable()
            .focus_signal(focused)
            .on(Phase::Bubble, handler)
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
                move || {
                    let sel = selection.get();
                    let focus = focused.get();
                    let items = items.clone();
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Percent(1.0)),
                        )
                        .draw(move |canvas, rect| {
                            let base = Style::new().fg(text_fg).bg(ground);
                            canvas.fill_styled(rect, ' ', &base);
                            for (i, label) in items.iter().enumerate() {
                                let y = rect.y + i as i32;
                                if y >= rect.bottom() {
                                    break;
                                }
                                let active = i == sel;
                                let style = if active && focus {
                                    Style::new().fg(sel_fg).bg(sel_bg)
                                } else if active {
                                    Style::new().fg(accent).bg(ground)
                                } else {
                                    base
                                };
                                if active && focus {
                                    canvas.fill_styled(
                                        crate::base::Rect::new(rect.x, y, rect.w, 1),
                                        ' ',
                                        &style,
                                    );
                                }
                                let mark = if active { "(•)" } else { "( )" };
                                canvas.print_styled(
                                    crate::base::Point::new(rect.x, y),
                                    mark,
                                    &style,
                                );
                                canvas.print_styled(
                                    crate::base::Point::new(rect.x + 4, y),
                                    label,
                                    &style,
                                );
                            }
                        })
                        .build()
                },
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::itest_util::{click, key, mount_widget};

    #[test]
    fn arrows_move_selection_when_focused_and_click_selects() {
        let t = default_theme().tokens;
        let mut state = None;
        let (root, mut tree) = mount_widget(Size::new(20, 3), |cx| {
            let sel = cx.signal(0usize);
            state = Some(sel);
            Element::new()
                .child(
                    RadioGroup::of(["a", "b", "c"])
                        .selection(sel)
                        .element(cx, &t)
                        .build(),
                )
                .build()
        });
        let sel = state.unwrap();
        // Unfocused arrows do nothing (group is one tab stop).
        key(&mut tree, Key::Down);
        assert_eq!(sel.get_untracked(), 0);
        click(&mut tree, 2, 2); // row 2 selects + focuses
        crate::reactive::flush_effects();
        assert_eq!(sel.get_untracked(), 2);
        key(&mut tree, Key::Up);
        crate::reactive::flush_effects();
        assert_eq!(sel.get_untracked(), 1, "focused arrows move");
        key(&mut tree, Key::Home);
        assert_eq!(sel.get_untracked(), 0);
        root.dispose();
    }
}

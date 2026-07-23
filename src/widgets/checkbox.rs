//! Checkbox: `[x] label` bound to a `Signal<bool>`.
//!
//! Space/Enter toggles while focused; click toggles and focuses. Style
//! guide §3.2 borderless rules: focus renders the selection pair over
//! the glyph+label, hover is accent ink garnish, disabled is faint.
//!
//! ```ignore
//! let on = cx.signal(false);
//! Checkbox::new("Autosave").checked(on).element(cx, &theme.tokens)
//! ```
//!
//! OWNER: REACT.

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, Key, MouseButton, MouseKind, Phase, UiEvent};

pub struct Checkbox {
    label: String,
    checked: Option<Signal<bool>>,
    disabled: bool,
    layout: Option<LayoutStyle>,
    on_change: Option<Box<dyn FnMut(bool)>>,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Checkbox {
        Checkbox {
            label: label.into(),
            checked: None,
            disabled: false,
            layout: None,
            on_change: None,
        }
    }

    /// Bind an external checked signal; default is internal.
    pub fn checked(mut self, checked: Signal<bool>) -> Checkbox {
        self.checked = Some(checked);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Checkbox {
        self.disabled = disabled;
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Checkbox {
        self.layout = Some(layout);
        self
    }

    pub fn on_change(mut self, f: impl FnMut(bool) + 'static) -> Checkbox {
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
        let faint = t.text_faint;

        let label = self.label;
        let disabled = self.disabled;
        let checked = self.checked.unwrap_or_else(|| cx.signal(false));
        let on_change: crate::widgets::SharedCallback<bool> =
            std::rc::Rc::new(std::cell::RefCell::new(self.on_change));
        let width = crate::text::width(&label) + 4; // "[x] "
        let layout = self.layout.unwrap_or_else(|| {
            // shrink 0: never crushed to zero by overflow (0240 #2).
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(1))
                .shrink(0.0)
        });

        let hovered = cx.signal(false);
        let focused = cx.signal(false);
        let toggle = {
            let on_change = on_change.clone();
            move || {
                let now = !checked.get_untracked();
                checked.set(now);
                if let Some(f) = on_change.borrow_mut().as_mut() {
                    f(now);
                }
            }
        };

        let mut el = Element::new()
            .style(layout)
            .role(crate::ui::Role::Checkbox)
            .access_label(label.clone())
            .access_value(move || if checked.get_untracked() { "on" } else { "off" }.into())
            .hover_signal(hovered)
            .focus_signal(focused);
        if !disabled {
            el = el.focusable().on(Phase::Bubble, move |ctx, ev| match ev {
                UiEvent::Key(k) if k.key == Key::Enter || k.key == Key::Char(' ') => {
                    if focused.get_untracked() {
                        toggle();
                        ctx.stop_propagation();
                    }
                }
                UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)) => {
                    toggle();
                    ctx.stop_propagation();
                }
                _ => {}
            });
        }
        el.child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1)),
            move || {
                let on = checked.get();
                let hover = hovered.get();
                let focus = focused.get();
                let label = label.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Cells(1)),
                    )
                    .draw(move |canvas, rect| {
                        let (fg, bg) = if disabled {
                            (faint, ground)
                        } else if focus {
                            (sel_fg, sel_bg)
                        } else if hover {
                            (accent, ground)
                        } else {
                            (text_fg, ground)
                        };
                        let style = Style::new().fg(fg).bg(bg);
                        canvas.fill_styled(rect, ' ', &style);
                        let mark = if on { "[x]" } else { "[ ]" };
                        canvas.print_styled(rect.origin(), mark, &style);
                        canvas.print_styled(
                            crate::base::Point::new(rect.x + 4, rect.y),
                            &label,
                            &style,
                        );
                    })
                    .build()
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Point, Size};
    use crate::theme::default_theme;
    use crate::widgets::itest_util::{click, key, mount_widget, render};

    /// Disposal-safety law (backlog 0297): Checkbox writes `checked`
    /// BEFORE `on_change`, so the callback may dispose the checkbox's
    /// scope synchronously. Audited clean at filing; pinned here so it
    /// stays that way.
    #[test]
    fn on_change_may_dispose_the_checkboxes_scope() {
        let t = default_theme().tokens;
        let mut tree = crate::ui::UiTree::new(Size::new(20, 1));
        let (root, ()) = crate::reactive::create_root(|cx| {
            let modal_cx = cx.child();
            let view = Checkbox::new("Wrap")
                .on_change(move |_| modal_cx.dispose())
                .element(modal_cx, &t)
                .build();
            tree.mount(modal_cx, view);
        });
        tree.layout();
        click(&mut tree, 1, 0); // toggle -> on_change -> dispose
        assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
        root.dispose();
    }

    #[test]
    fn toggles_by_click_and_by_key_when_focused() {
        let t = default_theme().tokens;
        let mut state = None;
        let (root, mut tree) = mount_widget(Size::new(20, 1), |cx| {
            let on = cx.signal(false);
            state = Some(on);
            Element::new()
                .child(Checkbox::new("Wrap").checked(on).element(cx, &t).build())
                .build()
        });
        let on = state.unwrap();
        click(&mut tree, 1, 0);
        crate::reactive::flush_effects();
        assert!(on.get_untracked(), "click toggles on");
        // Click also focused it; Space toggles back off.
        key(&mut tree, Key::Char(' '));
        crate::reactive::flush_effects();
        assert!(!on.get_untracked(), "space toggles off while focused");
        tree.layout();
        let canvas = render(&mut tree, Size::new(20, 1));
        assert_eq!(
            canvas.cell(Point::new(1, 0)).map(|c| c.0),
            Some(' '),
            "unchecked box"
        );
        root.dispose();
    }

    #[test]
    fn disabled_neither_focuses_nor_toggles() {
        let t = default_theme().tokens;
        let mut state = None;
        let (root, mut tree) = mount_widget(Size::new(20, 1), |cx| {
            let on = cx.signal(false);
            state = Some(on);
            Element::new()
                .child(
                    Checkbox::new("Wrap")
                        .checked(on)
                        .disabled(true)
                        .element(cx, &t)
                        .build(),
                )
                .build()
        });
        let on = state.unwrap();
        click(&mut tree, 1, 0);
        key(&mut tree, Key::Enter);
        crate::reactive::flush_effects();
        assert!(!on.get_untracked());
        root.dispose();
    }
}

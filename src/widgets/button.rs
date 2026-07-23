//! Button: a focusable, clickable label with hover/pressed/focused/
//! disabled visuals driven entirely by signals + tokens.
//!
//! The canonical build is [`Button::view`] (theme from context); a
//! compiled example, headless through the real tree + dispatch:
//!
//! ```
//! use abstracttui::base::Size;
//! use abstracttui::reactive::create_root;
//! use abstracttui::ui::{Element, Key, KeyEvent, UiEvent, UiTree};
//! use abstracttui::widgets::Button;
//! use std::{cell::Cell, rc::Rc};
//!
//! let clicks = Rc::new(Cell::new(0));
//! let c = clicks.clone();
//! let mut tree = UiTree::new(Size::new(20, 1));
//! let (root, ()) = create_root(|cx| {
//!     let view = Element::new()
//!         .child(Button::new("Save").on_click(move || c.set(c.get() + 1)).view(cx))
//!         .build();
//!     tree.mount(cx, view);
//! });
//! tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));   // focus it
//! tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Enter))); // activate
//! assert_eq!(clicks.get(), 1);
//! root.dispose();
//! ```
//!
//! Interaction contract: click = mouse press + release with the release
//! still inside the button (pointer capture keeps the release routed
//! here even if the pointer wandered; the rect check decides), or
//! Enter/Space while focused. Disabled buttons draw faint and consume
//! nothing.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::Scope;
use crate::render::{Attrs, Style};
use crate::theme::{TokenId, TokenSet};
use crate::ui::{dyn_view, Element, Key, MouseButton, MouseKind, Phase, UiEvent};

/// Token roles for each visual state — overridable so a destructive
/// button can swap tones without custom draw code. Defaults follow the
/// BINDING widget style guide (theme-identity.md §3.2, borderless
/// column): hover shifts INK to `accent` (bg unchanged, garnish only);
/// focus and press wear the SELECTION PAIR (the audited "you are here /
/// this acts on Enter" mechanism); disabled is `text_faint` ink.
#[derive(Copy, Clone, Debug)]
pub struct ButtonStyle {
    pub fg: TokenId,
    pub bg: TokenId,
    pub hover_ink: TokenId,
    pub focus_fg: TokenId,
    pub focus_bg: TokenId,
    pub disabled_fg: TokenId,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        ButtonStyle {
            fg: TokenId::Text,
            bg: TokenId::SurfaceRaised,
            hover_ink: TokenId::Accent,
            focus_fg: TokenId::SelectionFg,
            focus_bg: TokenId::SelectionBg,
            disabled_fg: TokenId::TextFaint,
        }
    }
}

pub struct Button {
    label: String,
    style: ButtonStyle,
    layout: Option<LayoutStyle>,
    disabled: bool,
    on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Button {
        Button {
            label: label.into(),
            style: ButtonStyle::default(),
            layout: None,
            disabled: false,
            on_click: None,
        }
    }

    pub fn style(mut self, style: ButtonStyle) -> Button {
        self.style = style;
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Button {
        self.layout = Some(layout);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Button {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, f: impl FnMut() + 'static) -> Button {
        self.on_click = Some(Box::new(f));
        self
    }

    /// Build the element. `cx` owns the button's internal state signals;
    /// tokens resolve NOW into plain colors (damage contract §5).
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
        let fg = t.get(self.style.fg);
        let bg = t.get(self.style.bg);
        let hover_ink = t.get(self.style.hover_ink);
        let focus_fg = t.get(self.style.focus_fg);
        let focus_bg = t.get(self.style.focus_bg);
        let disabled_fg = t.get(self.style.disabled_fg);

        let label = self.label;
        let disabled = self.disabled;
        let width = crate::text::width(&label) + 2; // 1-cell pad each side
        let layout = self.layout.unwrap_or_else(|| {
            // shrink 0: a default-styled control never yields its one
            // row/column to overflow pressure (0240 follow-up #2 — an
            // overflowing sibling used to crush buttons to zero rows).
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(1))
                .shrink(0.0)
        });

        let hovered = cx.signal(false);
        let pressed = cx.signal(false);
        let focused = cx.signal(false);
        let on_click: crate::widgets::SharedCallback<()> = Rc::new(RefCell::new(
            self.on_click
                .map(|mut f| Box::new(move |()| f()) as Box<dyn FnMut(())>),
        ));

        let fire = {
            let on_click = on_click.clone();
            move || {
                if let Some(f) = on_click.borrow_mut().as_mut() {
                    f(());
                }
            }
        };

        let mut el = Element::new()
            .style(layout)
            .role(crate::ui::Role::Button)
            .access_label(label.clone())
            .hover_signal(hovered)
            .focus_signal(focused);
        if disabled {
            el = el.access_value(|| "disabled".into());
        }
        if !disabled {
            el = el.focusable().on(Phase::Bubble, move |ctx, ev| match ev {
                // Keyboard activation requires FOCUS: unfocused keys can
                // still route here through the root fallback, and a
                // button that fires from them would hijack the app.
                UiEvent::Key(k) if k.key == Key::Enter || k.key == Key::Char(' ') => {
                    if focused.get_untracked() {
                        ctx.stop_propagation();
                        fire();
                    }
                }
                UiEvent::Mouse(m) => match m.kind {
                    MouseKind::Down(MouseButton::Left) => {
                        pressed.set(true);
                        ctx.stop_propagation();
                    }
                    MouseKind::Up(MouseButton::Left) => {
                        // Release-inside decides the click. Hover state is
                        // frozen while the pointer is captured, so the rect
                        // check is the truthful inside-ness test.
                        let inside = ctx.current_rect().contains(m.pos);
                        let clicks = pressed.get_untracked() && inside;
                        // Disposal-safety law (0297 — the 0250 ruling
                        // engine-wide): ALL widget bookkeeping lands
                        // BEFORE the user callback. `pressed` is cleared
                        // first, so an `on_click` that disposes this
                        // button's scope synchronously (the modal
                        // approve/deny close) finds no dangling signal
                        // write behind it. `stop_propagation` is
                        // dispatch-owned (disposal-safe either side);
                        // it rides ahead of the callback for the same
                        // bookkeeping-first reading.
                        pressed.set(false);
                        ctx.stop_propagation();
                        if clicks {
                            fire();
                        }
                    }
                    _ => {}
                },
                _ => {}
            });
        }

        el.child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1)),
            move || {
                // State snapshot (tracked): re-render on any state change.
                // Visuals per the §3.2 borderless column: press/focus wear
                // the selection pair (press adds BOLD), hover shifts ink
                // only, disabled is faint ink.
                let hovered = hovered.get();
                let pressed = pressed.get();
                let focused = focused.get();
                let label = label.clone();
                let (fg, bg, bold) = if disabled {
                    (disabled_fg, bg, false)
                } else if pressed {
                    (focus_fg, focus_bg, true)
                } else if focused {
                    (focus_fg, focus_bg, false)
                } else if hovered {
                    (hover_ink, bg, false)
                } else {
                    (fg, bg, false)
                };
                Element::new()
                    .style(LayoutStyle::default().width(Dimension::Percent(1.0)))
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        let mut style = Style::new().fg(fg).bg(bg);
                        if bold {
                            style = style.attrs(Attrs::BOLD);
                        }
                        canvas.fill_styled(rect, ' ', &style);
                        let x = rect.x + ((rect.w - crate::text::width(&label)).max(0)) / 2;
                        canvas.print_styled(crate::base::Point::new(x, rect.y), &label, &style);
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
    use crate::ui::{Key, MouseButton, MouseKind};
    use crate::widgets::itest_util::{click, key, mount_widget, mouse, render};

    fn clicks() -> (Rc<RefCell<u32>>, impl FnMut() + 'static) {
        let n = Rc::new(RefCell::new(0u32));
        let n2 = n.clone();
        (n, move || *n2.borrow_mut() += 1)
    }

    #[test]
    fn mouse_click_fires_once_and_only_inside() {
        let t = &default_theme().tokens;
        let (count, on) = clicks();
        let (_root, mut tree) = mount_widget(Size::new(12, 1), |cx| {
            Button::new("Go").on_click(on).element(cx, t).build()
        });
        click(&mut tree, 2, 0);
        assert_eq!(*count.borrow(), 1, "down+up inside = one click");
        // Press inside, drag out (captured), release outside: no click.
        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 0);
        mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 40, 0);
        mouse(&mut tree, MouseKind::Up(MouseButton::Left), 40, 0);
        assert_eq!(*count.borrow(), 1, "release outside cancels");
    }

    #[test]
    fn keyboard_activates_when_focused() {
        let t = &default_theme().tokens;
        let (count, on) = clicks();
        let (_root, mut tree) = mount_widget(Size::new(12, 1), |cx| {
            Button::new("Go").on_click(on).element(cx, t).build()
        });
        key(&mut tree, Key::Enter);
        assert_eq!(*count.borrow(), 0, "unfocused button ignores Enter");
        key(&mut tree, Key::Tab); // focus it
        key(&mut tree, Key::Enter);
        key(&mut tree, Key::Char(' '));
        assert_eq!(*count.borrow(), 2, "Enter and Space both click");
    }

    #[test]
    fn hover_focus_press_follow_the_style_guide() {
        // theme-identity.md §3.2, borderless column: hover = accent INK
        // (bg unchanged), focus = selection pair, press = pair + BOLD.
        let theme = default_theme();
        let t = &theme.tokens;
        let size = Size::new(12, 1);
        let (_root, mut tree) = mount_widget(size, |cx| Button::new("Go").element(cx, t).build());
        let base = render(&mut tree, size);
        assert_eq!(base.cell(Point::new(3, 0)).unwrap().2, t.surface_raised);
        mouse(&mut tree, MouseKind::Move, 3, 0); // hover
        let hovered = render(&mut tree, size);
        let label_x = (12 - 2) / 2; // centered "Go"
        assert_eq!(
            hovered.cell(Point::new(label_x, 0)).unwrap().1,
            t.accent,
            "hover shifts ink to accent"
        );
        assert_eq!(
            hovered.cell(Point::new(label_x, 0)).unwrap().2,
            t.surface_raised,
            "hover never changes the ground (garnish only)"
        );
        mouse(&mut tree, MouseKind::Move, 40, 0); // leave
        key(&mut tree, Key::Tab); // focus
        let focused = render(&mut tree, size);
        assert_eq!(
            focused.cell(Point::new(3, 0)).unwrap().2,
            t.selection_bg,
            "borderless focus wears the selection pair"
        );
        assert_eq!(
            focused.cell(Point::new(label_x, 0)).unwrap().1,
            t.selection_fg
        );
    }

    #[test]
    fn disabled_button_neither_fires_nor_focuses() {
        let t = &default_theme().tokens;
        let (count, on) = clicks();
        let (_root, mut tree) = mount_widget(Size::new(12, 1), |cx| {
            Button::new("Go")
                .disabled(true)
                .on_click(on)
                .element(cx, t)
                .build()
        });
        click(&mut tree, 2, 0);
        key(&mut tree, Key::Tab);
        key(&mut tree, Key::Enter);
        assert_eq!(*count.borrow(), 0);
        assert_eq!(tree.focused(), None, "disabled = not in tab order");
    }

    /// Disposal-safety law (backlog 0297 — the 0250 ruling clause 4
    /// stated engine-wide): Button completes its own bookkeeping (the
    /// `pressed` write) BEFORE `on_click` runs, so a click callback may
    /// dispose the button's scope synchronously — the natural modal
    /// approve/deny close — without a dead-signal write behind it. The
    /// mouse-Up arm is the one that wrote `pressed` AFTER the callback
    /// before the fix; the keyboard arm is exercised too.
    #[test]
    fn on_click_may_dispose_the_buttons_scope() {
        let t = default_theme().tokens;
        // Mouse path (the fixed arm): press + release inside.
        let clicks = Rc::new(RefCell::new(0u32));
        let mut tree = crate::ui::UiTree::new(Size::new(12, 1));
        let (root, ()) = crate::reactive::create_root(|cx| {
            let modal_cx = cx.child();
            let c = clicks.clone();
            let view = Button::new("Go")
                .on_click(move || {
                    *c.borrow_mut() += 1;
                    modal_cx.dispose();
                })
                .element(modal_cx, &t)
                .build();
            tree.mount(modal_cx, view);
        });
        tree.layout();
        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 0);
        mouse(&mut tree, MouseKind::Up(MouseButton::Left), 2, 0);
        assert_eq!(*clicks.borrow(), 1, "the click still fired");
        assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
        root.dispose();

        // Keyboard path (already clean; pinned so it stays that way).
        let clicks = Rc::new(RefCell::new(0u32));
        let mut tree = crate::ui::UiTree::new(Size::new(12, 1));
        let (root, ()) = crate::reactive::create_root(|cx| {
            let modal_cx = cx.child();
            let c = clicks.clone();
            let view = Button::new("Go")
                .on_click(move || {
                    *c.borrow_mut() += 1;
                    modal_cx.dispose();
                })
                .element(modal_cx, &t)
                .build();
            tree.mount(modal_cx, view);
        });
        tree.layout();
        key(&mut tree, Key::Tab); // focus
        key(&mut tree, Key::Enter);
        assert_eq!(*clicks.borrow(), 1, "Enter clicked");
        assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
        root.dispose();
    }

    /// The selection layer's gesture claim (0285): when a passed-through
    /// press becomes a drag, `UiTree::cancel_pointer_press` delivers a
    /// release outside every rect through the capture routing. The
    /// button must un-press WITHOUT firing (Up-inside-rect decides), the
    /// capture must drop, and the NEXT click must work normally — a
    /// stuck capture would route it back to this button.
    #[test]
    fn cancel_pointer_press_unpresses_without_firing() {
        let t = &default_theme().tokens;
        let (count, on) = clicks();
        let (_root, mut tree) = mount_widget(Size::new(12, 1), |cx| {
            Button::new("Go").on_click(on).element(cx, t).build()
        });
        // No press in flight: the cancel is an honest no-op.
        assert!(!tree.cancel_pointer_press(), "nothing to cancel");

        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 0);
        assert!(tree.pointer_capture().is_some(), "press captures");
        assert!(tree.cancel_pointer_press(), "a live press cancels");
        assert_eq!(*count.borrow(), 0, "cancel never clicks");
        assert_eq!(tree.pointer_capture(), None, "capture dropped");
        // The button is fully re-armed: a normal click still fires, and
        // the (already-cancelled) gesture's real Up is a harmless orphan.
        mouse(&mut tree, MouseKind::Up(MouseButton::Left), 2, 0);
        assert_eq!(*count.borrow(), 0, "orphan release after cancel");
        click(&mut tree, 2, 0);
        assert_eq!(*count.borrow(), 1, "next click fires normally");
    }

    /// The capture HEAL (0285's enabling fix for a PRE-EXISTING defect):
    /// the `pressed` write on Down regenerates the button's `dyn_view`
    /// hit leaf, which used to strand the pointer capture — a release
    /// OUTSIDE the button then routed by position, never reached it,
    /// and wedged the pressed visual (selection pair + BOLD) until the
    /// next click. The healed capture routes the outside release back
    /// here: no fire (rect decides), pressed clears.
    #[test]
    fn outside_release_reaches_the_button_and_clears_pressed() {
        let theme = default_theme();
        let t = &theme.tokens;
        let size = Size::new(12, 1);
        let (count, on) = clicks();
        let (_root, mut tree) = mount_widget(size, |cx| {
            Button::new("Go").on_click(on).element(cx, t).build()
        });
        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 0);
        mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 40, 0);
        mouse(&mut tree, MouseKind::Up(MouseButton::Left), 40, 0);
        assert_eq!(*count.borrow(), 0, "release outside never clicks");
        assert_eq!(tree.pointer_capture(), None, "capture released");
        // Click-to-focus keeps the button FOCUSED (selection pair, no
        // bold); a stuck press would render BOLD.
        let frame = render(&mut tree, size);
        assert!(
            !frame.attrs_at(Point::new(3, 0)).contains(Attrs::BOLD),
            "no stuck pressed visual after an outside release"
        );
    }

    #[test]
    fn pressed_state_wears_selection_pair_bold() {
        let theme = default_theme();
        let t = &theme.tokens;
        let size = Size::new(12, 1);
        let (_root, mut tree) = mount_widget(size, |cx| Button::new("Go").element(cx, t).build());
        mouse(&mut tree, MouseKind::Move, 3, 0);
        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 3, 0);
        let pressed = render(&mut tree, size);
        assert_eq!(pressed.cell(Point::new(3, 0)).unwrap().2, t.selection_bg);
        assert!(
            pressed.attrs_at(Point::new(3, 0)).contains(Attrs::BOLD),
            "press = selection pair + BOLD (§3.2)"
        );
    }
}

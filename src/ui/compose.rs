//! Shareable components: the React-shaped composition contract.
//!
//! THE PATTERN (a convention, not a framework type — Rust functions
//! already are components):
//!
//! - A component is a plain function `fn(Scope, Props) -> View` (or a
//!   closure). Props are a plain struct the CALLER builds: data fields,
//!   [`Callback`] fields for events, and `View` fields for slots
//!   (children). No trait to implement, no registry — a component in
//!   one module composes into any other by import.
//! - State lives in signals created on the passed `Scope`; the caller
//!   owns the lifetime (dispose the scope, the component's state dies).
//! - Reactivity comes from `dyn_view`/`dyn_view_scoped` INSIDE the
//!   component — a parent never re-renders a child; the child's own
//!   tracked regions update in place (the fine-grained model; there is
//!   no virtual DOM diff to pay for).
//!
//! ```ignore
//! struct CardProps {
//!     title: String,
//!     on_close: Callback<()>,   // typed event out
//!     children: View,           // slot
//! }
//!
//! fn card(cx: Scope, t: &TokenSet, props: CardProps) -> View {
//!     let close = props.on_close.clone();
//!     Element::new()
//!         .role(Role::Region)
//!         .access_label(props.title.clone())
//!         .style(Style::column())
//!         .child(
//!             Element::new()
//!                 .child(text(props.title))
//!                 .child(
//!                     Button::new("x")
//!                         .on_click(move || close.call(()))
//!                         .element(cx, t)
//!                         .build(),
//!                 )
//!                 .build(),
//!         )
//!         .child(props.children) // slot mounts where the component says
//!         .build()
//! }
//!
//! // Reuse from another module, twice, with different props:
//! let a = card(cx, t, CardProps { title: "Logs".into(), on_close: on_a, children: logs_view });
//! let b = card(cx, t, CardProps { title: "Stats".into(), on_close: on_b, children: stats_view });
//! ```
//!
//! The tested version of exactly this example lives in this module's
//! tests (`card_component_composes_twice_with_props_events_and_slots`),
//! along with the two ERGONOMICS PROOFS: a real 40-line todo app
//! (`todo_app`) and one component mounted into two separate apps.
//!
//! # State at app scale: signals-as-store + context
//!
//! The endorsed pattern for dashboard-scale state is a STORE STRUCT of
//! signals provided as CONTEXT — no reducer framework, no prop
//! drilling:
//!
//! ```ignore
//! #[derive(Clone)]
//! struct AppStore {
//!     user: Signal<Option<User>>,
//!     items: Signal<Vec<Item>>,
//!     filter: Signal<Filter>,
//! }
//!
//! // At the root:
//! let store = AppStore { user: cx.signal(None), items: cx.signal(vec![]), .. };
//! cx.provide_context(store);
//!
//! // In ANY component, however deep:
//! let store: AppStore = cx.use_context().expect("AppStore provided at root");
//! store.items.update(|v| v.push(item));
//! ```
//!
//! Rules that keep it sane: the store struct is `Clone` (signals are
//! `Copy` handles — cloning shares state, exactly right); provide at
//! the root, or provide a NARROWER store on a subtree to shadow (test:
//! `context_flows_down_shadows_and_dies_with_its_scope`); mutate
//! through signal methods so every consumer re-renders fine-grained.
//! Actions-as-functions replace reducers: `fn add_item(store: &AppStore,
//! item: Item)` — plain Rust, testable without any UI.
//!
//! # Derived state cookbook (`memo`)
//!
//! ```ignore
//! // Filtered view: recomputes when EITHER input changes, caches, and
//! // cuts propagation when the OUTPUT is equal (PartialEq).
//! let visible = cx.memo(move || {
//!     items.with(|v| v.iter().filter(|i| filter.get().admits(i)).cloned().collect::<Vec<_>>())
//! });
//! // Chained: memos read memos; only the tail re-renders the UI.
//! let count_line = cx.memo(move || format!("{} shown", visible.with(|v| v.len())));
//! // Expensive + rarely-changing: memo is lazy — nothing recomputes
//! // until something reads it after an input changed.
//! ```
//!
//! # Form validation (the pattern, RT8-5)
//!
//! There is deliberately no `Form` type in v1: validation is a MEMO
//! over the field signals — recomputes when inputs change, renders
//! through a `dyn_view`, and gates submission as plain data:
//!
//! ```ignore
//! let name = cx.signal(String::new());
//! let email = cx.signal(String::new());
//! // One memo per rule; None = valid.
//! let name_err = cx.memo(move || {
//!     name.with(|v| v.trim().is_empty().then(|| "name is required".to_string()))
//! });
//! let email_err = cx.memo(move || {
//!     email.with(|v| (!v.contains('@')).then(|| "not an email".to_string()))
//! });
//! let form_valid = cx.memo(move || name_err.get().is_none() && email_err.get().is_none());
//!
//! // Field + its error line; the error region re-renders per keystroke.
//! Element::new()
//!     .child(TextInput::new().value(name).placeholder("name").view(cx))
//!     .child(dyn_view(LayoutStyle::line(1), move || {
//!         text(name_err.get().unwrap_or_default())
//!     }))
//!     .child(
//!         Button::new("Submit")
//!             .on_click(move || if form_valid.get_untracked() { submit() })
//!             .view(cx),
//!     )
//! # ;
//! ```
//!
//! Disable-on-invalid: rebuild the button in a `dyn_view_scoped`
//! reading `form_valid` and pass `.disabled(!form_valid.get())`.
//!
//! # Navigation (router decision, cycle 7)
//!
//! There is deliberately NO router type. Page switching IS a signal +
//! `Dyn`, and it composes with everything above:
//!
//! ```ignore
//! #[derive(Clone, Copy, PartialEq)]
//! enum Page { Inbox, Settings }
//! let page = cx.signal(Page::Inbox);
//! cx.provide_context(page); // any component can navigate
//! dyn_view_scoped(Style::default().grow(1.0), move |gen_cx| match page.get() {
//!     Page::Inbox => inbox(gen_cx, &store),
//!     Page::Settings => settings(gen_cx, &store),
//! })
//! ```
//!
//! `dyn_view_scoped` gives each page a scope that dies on navigation —
//! page-local state cannot leak. The `Tabs` widget is this same pattern
//! with a bar attached. If a real app demonstrates a need for history/
//! deep-linking, a router earns its keep then, not before.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

/// A clone-cheap, typed event callback — the `on_*` prop currency.
///
/// ```
/// use abstracttui::ui::Callback;
/// use std::{cell::Cell, rc::Rc};
///
/// struct RowProps {
///     on_pick: Callback<usize>, // typed event out of a component
/// }
///
/// let picked = Rc::new(Cell::new(0usize));
/// let p = picked.clone();
/// let props = RowProps { on_pick: Callback::new(move |i| p.set(i)) };
/// let shared = props.on_pick.clone(); // clones share ONE callback
/// shared.call(7);
/// assert_eq!(picked.get(), 7);
/// let optional: Callback<()> = Callback::default(); // noop for unbound events
/// optional.call(());
/// ```
///
/// `Rc<RefCell<FnMut>>` underneath: cloning shares the SAME callback
/// (a component can hand it to a key handler and a click handler), and
/// `call` is safe anywhere user code runs (handlers, effects). One
/// rule, loudly enforced in debug: a callback must not recursively
/// invoke ITSELF (the RefCell is the reentrancy detector — release
/// builds skip the nested call instead of aborting).
pub struct Callback<T = ()> {
    f: Rc<RefCell<dyn FnMut(T)>>,
}

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Callback { f: self.f.clone() }
    }
}

impl<T> Callback<T> {
    pub fn new(f: impl FnMut(T) + 'static) -> Callback<T> {
        Callback {
            f: Rc::new(RefCell::new(f)),
        }
    }

    /// A callback that does nothing — the default for optional events.
    pub fn noop() -> Callback<T> {
        Callback::new(|_| {})
    }

    pub fn call(&self, arg: T) {
        match self.f.try_borrow_mut() {
            Ok(mut f) => f(arg),
            Err(_) => {
                // Self-reentrant call: the callback is already running.
                debug_assert!(
                    false,
                    "abstracttui Callback: re-entrant call of the same callback \
                     (a callback must not invoke itself; post work instead)"
                );
            }
        }
    }
}

impl<T> Default for Callback<T> {
    fn default() -> Self {
        Callback::noop()
    }
}

impl<T, F: FnMut(T) + 'static> From<F> for Callback<T> {
    fn from(f: F) -> Self {
        Callback::new(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::layout::{Dimension, Style};
    use crate::reactive::Scope;
    use crate::theme::{default_theme, TokenSet};
    use crate::ui::{text, Element, Role, View};
    use crate::widgets::itest_util::{click, mount_widget, render};
    use crate::widgets::Button;

    // The doc example, for real: a Card component with a title prop, a
    // typed close event and a children slot — defined ONCE, used twice.
    struct CardProps {
        title: String,
        on_close: Callback<()>,
        children: View,
    }

    fn card(cx: Scope, t: &TokenSet, props: CardProps) -> View {
        let close = props.on_close.clone();
        Element::new()
            .role(Role::Region)
            .access_label(props.title.clone())
            .style(Style::column().width(Dimension::Percent(1.0)))
            .child(
                Element::new()
                    .style(Style::row().height(Dimension::Cells(1)))
                    .child(text(props.title))
                    .child(
                        Button::new("x")
                            .on_click(move || close.call(()))
                            .element(cx, t)
                            .build(),
                    )
                    .build(),
            )
            .child(props.children)
            .build()
    }

    #[test]
    fn card_component_composes_twice_with_props_events_and_slots() {
        let t = default_theme().tokens;
        let closed: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let (c1, c2) = (closed.clone(), closed.clone());
        let (root, mut tree) = mount_widget(Size::new(24, 6), |cx| {
            Element::new()
                .style(Style::column().width(Dimension::Percent(1.0)))
                .child(card(
                    cx,
                    &t,
                    CardProps {
                        title: "Logs".into(),
                        on_close: Callback::new(move |()| c1.borrow_mut().push("logs")),
                        children: text("log body"),
                    },
                ))
                .child(card(
                    cx,
                    &t,
                    CardProps {
                        title: "Stats".into(),
                        on_close: Callback::new(move |()| c2.borrow_mut().push("stats")),
                        children: text("stat body"),
                    },
                ))
                .build()
        });
        let canvas = render(&mut tree, Size::new(24, 6));
        let screen: Vec<String> = (0..6).map(|y| canvas.row_text(y)).collect();
        assert!(screen[0].contains("Logs"), "{screen:?}");
        assert!(screen[1].contains("log body"), "{screen:?}");
        assert!(screen[2].contains("Stats"), "{screen:?}");
        assert!(screen[3].contains("stat body"), "{screen:?}");

        // Events route to the RIGHT instance: click each card's close.
        let x = screen[0].find('x').expect("close button visible") as i32;
        click(&mut tree, x, 0);
        let x2 = screen[2].find('x').expect("second close visible") as i32;
        click(&mut tree, x2, 2);
        assert_eq!(*closed.borrow(), vec!["logs", "stats"]);

        // The semantic tree sees both regions by label.
        let a11y = tree.accessibility_tree();
        let labels: Vec<&str> = a11y
            .entries
            .iter()
            .filter(|e| e.role == Role::Region)
            .map(|e| e.label.as_str())
            .collect();
        assert_eq!(labels, vec!["Logs", "Stats"]);
        root.dispose();
    }

    // -----------------------------------------------------------------
    // ERGONOMICS ACCEPTANCE (cycle 7): the two proofs.
    // -----------------------------------------------------------------

    /// PROOF 1 — a real app in UNDER 60 LINES (this module, imports and
    /// blanks included, is the count: 40 lines from `mod todo_app {` to
    /// its closing brace). A todo list with an input, add-on-Enter, a
    /// selectable list and a live count — state, components, events,
    /// fine-grained updates.
    mod todo_app {
        use crate::layout::{Dimension, Style};
        use crate::prelude::*;
        use crate::reactive::Scope;
        use crate::theme::TokenSet;
        use crate::widgets::{List, TextInput};

        pub fn app(cx: Scope, t: &TokenSet) -> View {
            let items = cx.signal(vec!["learn abstracttui".to_string()]);
            let selected = cx.signal(0usize);
            let draft = cx.signal(String::new());
            let tokens = *t;
            let input = TextInput::new()
                .value(draft)
                .placeholder("add a todo, press Enter")
                .on_submit(move |s: &str| {
                    if !s.is_empty() {
                        let s = s.to_string();
                        items.update(|v| v.push(s));
                        draft.set(String::new());
                    }
                })
                .element(cx, t)
                .build();
            Element::new()
                .style(
                    Style::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .child(input)
                .child(dyn_view_scoped(
                    Style::default().width(Dimension::Percent(1.0)).grow(1.0),
                    move |gen_cx| {
                        List::new(items.get())
                            .selection(selected)
                            .element(gen_cx, &tokens)
                            .build()
                    },
                ))
                .child(dyn_view(
                    Style::default().height(Dimension::Cells(1)),
                    move || text(format!("{} todos", items.with(|v| v.len()))),
                ))
                .build()
        }
    }

    #[test]
    fn sixty_line_app_proof_renders_and_reacts() {
        let t = default_theme().tokens;
        let (root, mut tree) = mount_widget(Size::new(30, 8), |cx| todo_app::app(cx, &t));
        // Type into the input (Tab focuses it first) and submit.
        crate::widgets::itest_util::key(&mut tree, crate::ui::Key::Tab);
        crate::widgets::itest_util::type_str(&mut tree, "write tests");
        crate::widgets::itest_util::key(&mut tree, crate::ui::Key::Enter);
        crate::reactive::flush_effects();
        tree.layout();
        let canvas = render(&mut tree, Size::new(30, 8));
        let screen: String = (0..8).map(|y| canvas.row_text(y) + "\n").collect();
        assert!(screen.contains("learn abstracttui"), "{screen}");
        assert!(screen.contains("write tests"), "{screen}");
        assert!(screen.contains("2 todos"), "{screen}");
        root.dispose();
    }

    /// PROOF 2 — ONE component (the Card above) mounted into TWO
    /// separate apps (independent trees, scopes, prop sets): the
    /// "shareable component" requirement made literal.
    #[test]
    fn one_component_reused_across_two_apps() {
        let t = default_theme().tokens;
        let build = move |title: &str, body: &'static str| {
            let title = title.to_string();
            mount_widget(Size::new(20, 4), move |cx| {
                card(
                    cx,
                    &t,
                    CardProps {
                        title,
                        on_close: Callback::noop(),
                        children: text(body),
                    },
                )
            })
        };
        let (root_a, mut app_a) = build("App A", "alpha body");
        let (root_b, mut app_b) = build("App B", "beta body");
        let a = render(&mut app_a, Size::new(20, 4));
        let b = render(&mut app_b, Size::new(20, 4));
        assert!(a.row_text(0).contains("App A"));
        assert!(a.row_text(1).contains("alpha body"));
        assert!(b.row_text(0).contains("App B"));
        assert!(b.row_text(1).contains("beta body"));
        root_a.dispose();
        root_b.dispose();
    }

    #[test]
    fn callback_is_clone_shared_and_reentrancy_safe() {
        let count: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
        let c = count.clone();
        let cb: Callback<i32> = Callback::new(move |n| *c.borrow_mut() += n as u32);
        let cb2 = cb.clone();
        cb.call(2);
        cb2.call(3);
        assert_eq!(*count.borrow(), 5, "clones share one callback");
        let optional: Callback<()> = Callback::default();
        optional.call(()); // noop default: safe to call unbound events
    }
}

//! The choice-control family (backlog 0500): [`Select`] (closed
//! one-of-N), [`Combobox`] (type-to-filter), [`MultiSelect`]
//! (accumulating set) — three faces over one shared core
//! (select_core.rs) and one popup substrate (`app::anchored`, OWNED
//! mode).
//!
//! They live APP-side, not in `widgets`, for the same reason
//! `Modal`/`Toast`/`Completion` do: opening a popup needs the overlay
//! store, and `widgets` sits below `app` in the layer map (integrator
//! ruling R4-1: no upward imports, even textual ones). They are still
//! plain token-consuming components with no engine privileges — the
//! widget rules (RT1-9b tokens-only, `.view(cx)` canonical build)
//! hold throughout.
//!
//! ## The shared contract (all three faces)
//!
//! - The closed control is ONE focusable row: side strokes carry
//!   focus (`border` -> `border_focus`), the current choice renders as
//!   text (`text_faint` placeholder when nothing is chosen), and a
//!   `▾` affordance sits by the right stroke. Enter/Space or a click
//!   opens; disabled renders faint and leaves the focus order.
//! - The open popup is an OWNED anchored popup: a modal tree above
//!   the whole live stack (select-inside-modal-inside-modal layers
//!   correctly), placed below the trigger or flipped above when
//!   cramped, width-matched to the trigger. Keys go TO the popup:
//!   Up/Down/PageUp/PageDown/Home/End move a HIGHLIGHT (never the
//!   bound value — the 0250 movement-vs-activation split), Enter
//!   commits, Escape abandons, an outside press dismisses WITHOUT
//!   acting on what is below, and the opener's scope dying takes the
//!   popup with it.
//! - `on_change` fires on COMMIT only, and only when the committed
//!   value differs (the 0250 ruling). [`Select::commit_on_move`] is
//!   the opt-in exception for cheap, non-destructive previews (theme
//!   pickers): highlight movement then commits live and Escape
//!   restores the pre-open value.
//! - A11y: the closed control reports `Role::Button` (a select trigger
//!   IS a button that opens a menu) with the current choice as its
//!   access value; the popup reports `Role::Menu` with `Role::MenuItem`
//!   rows. A dedicated `Role::Select` variant is parked in the 0.3
//!   breaking budget (adding it to the published exhaustive enum is a
//!   major break — budget 0002 entry 1).
//!
//! OWNER: SELECT (0500).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{Element, EventCtx, Key, Mods, MouseButton, MouseKind, Phase, Role, UiEvent};

use super::anchored::{DismissReason, PanelAnchor, PanelWidth, Popup};
use super::overlays::Overlays;
use super::viewport::use_viewport;

#[path = "select_core.rs"]
mod core;
use core::{
    first_enabled, last_enabled, option_rows_view, page_highlight, resolve_overlays,
    step_highlight, trigger_view, type_ahead_target, OptionRows, TriggerLabel, TypeAhead,
    DEFAULT_MAX_VISIBLE,
};

#[path = "select_combobox.rs"]
mod combobox;
pub use combobox::Combobox;

#[path = "select_handle.rs"]
mod handle;
use handle::anchor_cell;
pub use handle::SelectHandle;

#[path = "select_multi.rs"]
mod multi;
pub use multi::MultiSelect;

/// One row of a choice control. `key` is the stable identity
/// (MultiSelect accumulates keys; defaults to the label), `hint`
/// renders muted and right-aligned (provider names, shortcuts),
/// `disabled` rows render faint and are skipped by highlight movement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
    pub disabled: bool,
}

impl SelectOption {
    /// An option whose key IS its label (the common case).
    pub fn new(label: impl Into<String>) -> SelectOption {
        let label = label.into();
        SelectOption {
            key: label.clone(),
            label,
            hint: None,
            disabled: false,
        }
    }

    /// An option with a stable key distinct from its display label.
    pub fn keyed(key: impl Into<String>, label: impl Into<String>) -> SelectOption {
        SelectOption {
            key: key.into(),
            label: label.into(),
            hint: None,
            disabled: false,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> SelectOption {
        self.hint = Some(hint.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> SelectOption {
        self.disabled = disabled;
        self
    }
}

/// Session state behind one open Select popup.
struct SelectSession {
    popup: Option<Popup>,
    pre_open: usize,
    type_ahead: TypeAhead,
}

/// Closed single-choice control: a one-row trigger showing the current
/// value; the open popup lists all options. Bound to a
/// `Signal<usize>` (index into the options; out-of-range = nothing
/// chosen yet, the placeholder shows). Type-ahead inside the popup
/// jumps the highlight by label prefix; a repeated char cycles.
///
/// ```ignore
/// let picked = cx.signal(0usize);
/// Select::new(vec![SelectOption::new("stable"), SelectOption::new("beta")])
///     .value(picked)
///     .on_change(|i| apply_channel(i))
///     .view(cx)
/// ```
pub struct Select {
    options: Vec<SelectOption>,
    value: Option<Signal<usize>>,
    placeholder: String,
    disabled: bool,
    commit_on_move: bool,
    max_visible: usize,
    layout: Option<LayoutStyle>,
    overlays: Option<Overlays>,
    on_change: Option<Box<dyn FnMut(usize)>>,
    handle: Option<SelectHandle>,
}

impl Select {
    pub fn new(options: Vec<SelectOption>) -> Select {
        Select {
            options,
            value: None,
            placeholder: String::from("choose…"),
            disabled: false,
            commit_on_move: false,
            max_visible: DEFAULT_MAX_VISIBLE,
            layout: None,
            overlays: None,
            on_change: None,
            handle: None,
        }
    }

    /// Bind the chosen INDEX (external signal; default internal).
    /// Out-of-range (e.g. `usize::MAX`) = nothing chosen yet.
    pub fn value(mut self, value: Signal<usize>) -> Select {
        self.value = Some(value);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Select {
        self.placeholder = text.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Select {
        self.disabled = disabled;
        self
    }

    /// OPT-IN commit-on-move (default OFF — the 0250 ruling): while
    /// the popup is open, highlight movement commits live (value +
    /// `on_change`) for cheap non-destructive previews; Escape then
    /// restores the pre-open value. Every other dismissal keeps the
    /// last previewed value — opting in means moves ARE commits.
    pub fn commit_on_move(mut self, on: bool) -> Select {
        self.commit_on_move = on;
        self
    }

    /// Popup rows shown at once (default 8); longer lists window
    /// around the highlight.
    pub fn max_visible(mut self, n: usize) -> Select {
        self.max_visible = n.max(1);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Select {
        self.layout = Some(layout);
        self
    }

    /// Explicit overlay store (tests, exotic embeddings). Default:
    /// the app-provided reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> Select {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires on COMMIT (Enter/click), and only when the committed
    /// index differs from the bound value. With
    /// [`Select::commit_on_move`], highlight moves commit too.
    pub fn on_change(mut self, f: impl FnMut(usize) + 'static) -> Select {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Programmatic-open wiring (backlog 0296): `handle.open()` opens
    /// this face's popup without a trigger gesture — the command-
    /// summoned picker verb. Anchor and lifecycle contract on
    /// [`SelectHandle`]. Disabled faces refuse programmatic opens too.
    pub fn handle(mut self, handle: &SelectHandle) -> Select {
        self.handle = Some(handle.clone());
        self
    }

    /// Canonical one-call build: tokens resolve from the app's theme
    /// context (tracked — a `dyn_view` host re-renders on switch).
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        // TokenSet is Copy: the open handler captures the resolved
        // palette and the popup builds with it later (§5 discipline —
        // resolved colors, no theme reads at draw time).
        let tokens = *t;
        let options: Rc<Vec<SelectOption>> = Rc::new(self.options);
        let value = self.value.unwrap_or_else(|| cx.signal(usize::MAX));
        let placeholder = self.placeholder;
        let disabled = self.disabled;
        let commit_on_move = self.commit_on_move;
        let max_visible = self.max_visible;
        let overlays = resolve_overlays(cx, self.overlays);
        let viewport = use_viewport(cx);
        let on_change: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_change));

        let focused = cx.signal(false);
        let hovered = cx.signal(false);
        // Display order is the full option list (no filtering face).
        let display: Signal<Vec<usize>> = cx.signal((0..options.len()).collect());
        let highlight: Signal<usize> = cx.signal(0);
        let session: Rc<RefCell<SelectSession>> = Rc::new(RefCell::new(SelectSession {
            popup: None,
            pre_open: usize::MAX,
            type_ahead: TypeAhead::default(),
        }));

        // Commit = write-if-different + notify (0250: never on move
        // unless commit_on_move opted in).
        let write_value = Rc::new({
            let on_change = on_change.clone();
            move |idx: usize| {
                if value.get_untracked() == idx {
                    return;
                }
                value.set(idx);
                if let Some(f) = on_change.borrow_mut().as_mut() {
                    f(idx);
                }
            }
        });

        // Commit the highlighted option and close (Enter / click).
        let commit_and_close = Rc::new({
            let session = session.clone();
            let options = options.clone();
            let write_value = write_value.clone();
            move || {
                let disp = display.get_untracked();
                let pos = highlight.get_untracked().min(disp.len().saturating_sub(1));
                let Some(&idx) = disp.get(pos) else { return };
                if options.get(idx).is_some_and(|o| o.disabled) {
                    return;
                }
                write_value(idx);
                let popup = session.borrow().popup.clone();
                if let Some(popup) = popup {
                    popup.dismiss(DismissReason::Commit);
                }
            }
        });

        let open = Rc::new({
            let session = session.clone();
            let options = options.clone();
            let write_value = write_value.clone();
            let commit_and_close = commit_and_close.clone();
            let overlays = overlays.clone();
            move |anchor: Rect| {
                if session.borrow().popup.is_some() {
                    return;
                }
                let Some(overlays) = overlays.clone() else {
                    debug_assert!(
                        false,
                        "Select: no Overlays available — build inside an App (context) \
                         or pass .overlays(..) explicitly"
                    );
                    return;
                };
                let disp = display.get_untracked();
                if disp.is_empty() {
                    return;
                }
                // Highlight seeds on the current value (when enabled),
                // else the first enabled option.
                let current = value.get_untracked();
                let seed = disp
                    .iter()
                    .position(|&i| i == current)
                    .filter(|&p| options.get(disp[p]).is_some_and(|o| !o.disabled))
                    .or_else(|| first_enabled(&options, &disp))
                    .unwrap_or(0);
                highlight.set(seed);
                {
                    let mut s = session.borrow_mut();
                    s.pre_open = current;
                    s.type_ahead.clear();
                }
                let rows = disp.len().min(max_visible) as i32;
                let build = select_popup_content(
                    tokens,
                    options.clone(),
                    display,
                    highlight,
                    max_visible,
                    commit_on_move,
                    session.clone(),
                    write_value.clone(),
                    commit_and_close.clone(),
                );
                let popup = Popup::open(
                    &overlays,
                    cx,
                    viewport.get_untracked(),
                    PanelAnchor { rect: anchor },
                    PanelWidth::MatchAnchor,
                    Size::new(anchor.w, rows),
                    build,
                );
                let Some(popup) = popup else { return };
                popup.on_dismiss({
                    let session = session.clone();
                    let write_value = write_value.clone();
                    move |reason| {
                        let pre_open = session.borrow().pre_open;
                        if reason == DismissReason::Escape && commit_on_move {
                            // Restore the pre-open value (live preview
                            // abandoned) — write-if-different notifies.
                            write_value(pre_open);
                        }
                        session.borrow_mut().popup = None;
                    }
                });
                session.borrow_mut().popup = Some(popup);
            }
        });

        // Trigger rect recorded at draw time: the anchor source for
        // programmatic opens (0296 — see SelectHandle's contract).
        let last_rect = anchor_cell();
        if let Some(h) = &self.handle {
            if !disabled {
                let open = open.clone();
                let session = session.clone();
                let last_rect = last_rect.clone();
                h.wire(cx, move || {
                    if session.borrow().popup.is_some() {
                        return true; // already open counts as open
                    }
                    let Some(anchor) = last_rect.get() else {
                        return false; // never painted: no honest anchor
                    };
                    open(anchor);
                    session.borrow().popup.is_some()
                });
            }
        }
        let access_options = options.clone();
        let access_placeholder = placeholder.clone();
        let mut el = Element::new()
            .style(self.layout.unwrap_or_else(|| {
                LayoutStyle::default()
                    .height(Dimension::Cells(1))
                    .grow(1.0)
                    .shrink(0.0)
            }))
            // `Button`, not a `Select` role: the trigger IS a button
            // opening a menu, and adding `Role::Select` to the published
            // exhaustive enum is a semver break (0.3 budget entry 1).
            .role(Role::Button)
            .access_label(placeholder.clone())
            .access_value(move || {
                let v = value.get_untracked();
                access_options
                    .get(v)
                    .map(|o| o.label.clone())
                    .unwrap_or_else(|| access_placeholder.clone())
            })
            .draw(move |_canvas, rect| last_rect.set(Some(rect)))
            .hover_signal(hovered)
            .focus_signal(focused);
        if !disabled {
            let open = open.clone();
            el = el.focusable().on(Phase::Bubble, move |ctx, ev| match ev {
                UiEvent::Key(k)
                    if (k.key == Key::Enter || k.key == Key::Char(' ')) && k.mods == Mods::NONE =>
                {
                    if focused.get_untracked() {
                        open(ctx.current_rect());
                        ctx.stop_propagation();
                    }
                }
                UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)) => {
                    open(ctx.current_rect());
                    ctx.stop_propagation();
                }
                _ => {}
            });
        }
        let label_options = options.clone();
        let label_placeholder = placeholder.clone();
        el.child(trigger_view(
            t,
            focused,
            hovered,
            disabled,
            Rc::new(move || {
                let v = value.get();
                match label_options.get(v) {
                    Some(o) => TriggerLabel {
                        text: o.label.clone(),
                        short: None,
                        placeholder: false,
                    },
                    None => TriggerLabel {
                        text: label_placeholder.clone(),
                        short: None,
                        placeholder: true,
                    },
                }
            }),
        ))
    }
}

/// Build the Select popup content: raised ground + `Role::Menu` root
/// owning the navigation keys, option rows inside.
#[allow(clippy::too_many_arguments)]
fn select_popup_content(
    tokens: TokenSet,
    options: Rc<Vec<SelectOption>>,
    display: Signal<Vec<usize>>,
    highlight: Signal<usize>,
    max_visible: usize,
    commit_on_move: bool,
    session: Rc<RefCell<SelectSession>>,
    write_value: Rc<dyn Fn(usize)>,
    commit_and_close: Rc<dyn Fn()>,
) -> impl FnOnce(Scope, bool) -> crate::ui::View {
    move |_pcx: Scope, _flipped: bool| {
        let ground = tokens.surface_raised;
        let ink = tokens.text;
        // Highlight movement, shared by every navigation key: moves the
        // HIGHLIGHT; commits live only under commit_on_move.
        let move_to = Rc::new({
            let options = options.clone();
            move |pos: usize| {
                highlight.set(pos);
                if commit_on_move {
                    if let Some(&idx) = display.get_untracked().get(pos) {
                        if options.get(idx).is_some_and(|o| !o.disabled) {
                            write_value(idx);
                        }
                    }
                }
            }
        });
        let key_handler = {
            let options = options.clone();
            let session = session.clone();
            let commit = commit_and_close.clone();
            let move_to = move_to.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                let UiEvent::Key(k) = ev else { return };
                if k.mods != Mods::NONE {
                    return;
                }
                let disp = display.get_untracked();
                let h = highlight.get_untracked().min(disp.len().saturating_sub(1));
                match k.key {
                    Key::Down => move_to(step_highlight(&options, &disp, h, 1)),
                    Key::Up => move_to(step_highlight(&options, &disp, h, -1)),
                    Key::Home => {
                        if let Some(p) = first_enabled(&options, &disp) {
                            move_to(p);
                        }
                    }
                    Key::End => {
                        if let Some(p) = last_enabled(&options, &disp) {
                            move_to(p);
                        }
                    }
                    Key::PageDown => move_to(page_highlight(&options, &disp, h, 1, max_visible)),
                    Key::PageUp => move_to(page_highlight(&options, &disp, h, -1, max_visible)),
                    Key::Enter => commit(),
                    Key::Char(c) if !c.is_control() => {
                        // Type-ahead: prefix jump / same-char cycle.
                        let target = {
                            let mut s = session.borrow_mut();
                            let buf = s.type_ahead.push(c, std::time::Instant::now());
                            type_ahead_target(&options, &disp, buf, h)
                        };
                        if let Some(p) = target {
                            move_to(p);
                        }
                    }
                    _ => return, // Escape and the rest: substrate's turn
                }
                ctx.stop_propagation();
            }
        };
        let on_activate = Rc::new({
            let commit = commit_and_close.clone();
            move |pos: usize| {
                highlight.set(pos);
                commit();
            }
        });
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .role(Role::Menu)
            .access_label("options")
            .on(Phase::Bubble, key_handler)
            .draw(move |canvas, rect| {
                canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
            })
            .child(option_rows_view(
                &tokens,
                OptionRows {
                    options,
                    display,
                    highlight,
                    checks: None,
                    max_visible,
                    on_activate,
                },
            ))
            .build()
    }
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;

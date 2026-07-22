//! MultiSelect (backlog 0500 face 4): an accumulating set-of-N
//! control. Private sibling of select.rs; public path
//! `app::select::MultiSelect`.
//!
//! The open popup shows checkbox-marked options; Space (or a click)
//! toggles the highlighted option in a WORKING copy without closing;
//! Enter commits the whole set into the bound `Signal<Vec<String>>`
//! (option KEYS, canonicalized to option order) and fires `on_change`
//! once; Escape and outside-press abandon the working copy — the
//! dismiss-without-acting contract, uniform across the family. The
//! collapsed row joins the chosen labels and degrades honestly to
//! "N selected" when they overflow the row.
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

use super::super::anchored::{DismissReason, PanelAnchor, PanelWidth, Popup};
use super::super::overlays::Overlays;
use super::super::viewport::use_viewport;
use super::core::{
    first_enabled, last_enabled, option_rows_view, page_highlight, resolve_overlays,
    step_highlight, trigger_view, OptionRows, TriggerLabel, DEFAULT_MAX_VISIBLE,
};
use super::{anchor_cell, SelectHandle, SelectOption};

struct MultiSession {
    popup: Option<Popup>,
}

/// Keys filtered + ordered by the option list (the canonical set
/// shape: commit comparison and the collapsed label both read it).
fn canonical(options: &[SelectOption], keys: &[String]) -> Vec<String> {
    options
        .iter()
        .filter(|o| keys.contains(&o.key))
        .map(|o| o.key.clone())
        .collect()
}

/// Accumulating multi-choice control bound to a `Signal<Vec<String>>`
/// of option keys. Space toggles inside the popup; Enter commits the
/// set; Escape/outside-press abandon it.
///
/// ```ignore
/// let tags = cx.signal(Vec::<String>::new());
/// MultiSelect::new(tag_options).values(tags).view(cx)
/// ```
pub struct MultiSelect {
    options: Vec<SelectOption>,
    values: Option<Signal<Vec<String>>>,
    placeholder: String,
    disabled: bool,
    max_visible: usize,
    layout: Option<LayoutStyle>,
    overlays: Option<Overlays>,
    on_change: Option<Box<dyn FnMut(Vec<String>)>>,
    handle: Option<SelectHandle>,
}

impl MultiSelect {
    pub fn new(options: Vec<SelectOption>) -> MultiSelect {
        MultiSelect {
            options,
            values: None,
            placeholder: String::from("choose…"),
            disabled: false,
            max_visible: DEFAULT_MAX_VISIBLE,
            layout: None,
            overlays: None,
            on_change: None,
            handle: None,
        }
    }

    /// Bind the committed KEY SET (external signal; default internal).
    pub fn values(mut self, values: Signal<Vec<String>>) -> MultiSelect {
        self.values = Some(values);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> MultiSelect {
        self.placeholder = text.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> MultiSelect {
        self.disabled = disabled;
        self
    }

    /// Popup rows shown at once (default 8).
    pub fn max_visible(mut self, n: usize) -> MultiSelect {
        self.max_visible = n.max(1);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> MultiSelect {
        self.layout = Some(layout);
        self
    }

    /// Explicit overlay store (tests, exotic embeddings). Default:
    /// the app-provided reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> MultiSelect {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires ONCE on Enter-commit, with the canonicalized key set —
    /// and only when the set differs from the bound value. Toggles
    /// never fire it (the 0250 ruling: movement is not activation).
    pub fn on_change(mut self, f: impl FnMut(Vec<String>) + 'static) -> MultiSelect {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Programmatic-open wiring (backlog 0296): `handle.open()` opens
    /// this face's popup without a trigger gesture. Anchor and
    /// lifecycle contract on [`SelectHandle`].
    pub fn handle(mut self, handle: &SelectHandle) -> MultiSelect {
        self.handle = Some(handle.clone());
        self
    }

    /// Canonical one-call build (theme from context).
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let tokens = *t;
        let options: Rc<Vec<SelectOption>> = Rc::new(self.options);
        let values = self.values.unwrap_or_else(|| cx.signal(Vec::new()));
        let placeholder = self.placeholder;
        let disabled = self.disabled;
        let max_visible = self.max_visible;
        let overlays = resolve_overlays(cx, self.overlays);
        let viewport = use_viewport(cx);
        let on_change: crate::widgets::SharedCallback<Vec<String>> =
            Rc::new(RefCell::new(self.on_change));

        let focused = cx.signal(false);
        let hovered = cx.signal(false);
        let display: Signal<Vec<usize>> = cx.signal((0..options.len()).collect());
        let highlight: Signal<usize> = cx.signal(0);
        // The WORKING copy: toggles land here; only Enter commits it.
        let working: Signal<Vec<String>> = cx.signal(Vec::new());
        let session: Rc<RefCell<MultiSession>> =
            Rc::new(RefCell::new(MultiSession { popup: None }));

        let toggle = Rc::new({
            let options = options.clone();
            move |pos: usize| {
                let disp = display.get_untracked();
                let Some(&idx) = disp.get(pos) else { return };
                let Some(opt) = options.get(idx) else { return };
                if opt.disabled {
                    return;
                }
                let key = opt.key.clone();
                working.update(|w| {
                    if let Some(i) = w.iter().position(|k| *k == key) {
                        w.remove(i);
                    } else {
                        w.push(key);
                    }
                });
            }
        });

        let commit_and_close = Rc::new({
            let session = session.clone();
            let options = options.clone();
            let on_change = on_change.clone();
            move || {
                let set = canonical(&options, &working.get_untracked());
                if values.get_untracked() != set {
                    values.set(set.clone());
                    if let Some(f) = on_change.borrow_mut().as_mut() {
                        f(set);
                    }
                }
                let popup = session.borrow().popup.clone();
                if let Some(popup) = popup {
                    popup.dismiss(DismissReason::Commit);
                }
            }
        });

        let open = Rc::new({
            let session = session.clone();
            let options = options.clone();
            let commit_and_close = commit_and_close.clone();
            let toggle = toggle.clone();
            let overlays = overlays.clone();
            move |anchor: Rect| {
                if session.borrow().popup.is_some() {
                    return;
                }
                let Some(overlays) = overlays.clone() else {
                    debug_assert!(
                        false,
                        "MultiSelect: no Overlays available — build inside an App (context) \
                         or pass .overlays(..) explicitly"
                    );
                    return;
                };
                let disp = display.get_untracked();
                if disp.is_empty() {
                    return;
                }
                // Fresh working copy from the committed set.
                working.set(values.get_untracked());
                highlight.set(first_enabled(&options, &disp).unwrap_or(0));
                let rows = disp.len().min(max_visible) as i32;
                let build = multi_popup_content(
                    tokens,
                    options.clone(),
                    display,
                    highlight,
                    working,
                    max_visible,
                    toggle.clone(),
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
                    move |_reason| {
                        // Escape/outside-press abandon the working copy
                        // by doing NOTHING — only Enter wrote the set.
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
        let mut el = Element::new()
            .style(self.layout.unwrap_or_else(|| {
                LayoutStyle::default()
                    .height(Dimension::Cells(1))
                    .grow(1.0)
                    .shrink(0.0)
            }))
            // `Button` until `Role::Select` lands in the 0.3 batch
            // (budget 0002 entry 1); the chosen set rides the access value.
            .role(Role::Button)
            .access_label(placeholder.clone())
            .access_value(move || {
                let keys = values.get_untracked();
                let labels: Vec<&str> = access_options
                    .iter()
                    .filter(|o| keys.contains(&o.key))
                    .map(|o| o.label.as_str())
                    .collect();
                labels.join(", ")
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
                let keys = values.get();
                let picked: Vec<&str> = label_options
                    .iter()
                    .filter(|o| keys.contains(&o.key))
                    .map(|o| o.label.as_str())
                    .collect();
                if picked.is_empty() {
                    TriggerLabel {
                        text: label_placeholder.clone(),
                        short: None,
                        placeholder: true,
                    }
                } else {
                    TriggerLabel {
                        text: picked.join(", "),
                        short: Some(format!("{} selected", picked.len())),
                        placeholder: false,
                    }
                }
            }),
        ))
    }
}

/// Popup content: `Role::Menu` root owning the keys, checkbox-marked
/// option rows inside.
#[allow(clippy::too_many_arguments)]
fn multi_popup_content(
    tokens: TokenSet,
    options: Rc<Vec<SelectOption>>,
    display: Signal<Vec<usize>>,
    highlight: Signal<usize>,
    working: Signal<Vec<String>>,
    max_visible: usize,
    toggle: Rc<dyn Fn(usize)>,
    commit_and_close: Rc<dyn Fn()>,
) -> impl FnOnce(Scope, bool) -> crate::ui::View {
    move |_pcx: Scope, _flipped: bool| {
        let ground = tokens.surface_raised;
        let ink = tokens.text;
        let key_handler = {
            let options = options.clone();
            let toggle = toggle.clone();
            let commit = commit_and_close.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                let UiEvent::Key(k) = ev else { return };
                if k.mods != Mods::NONE {
                    return;
                }
                let disp = display.get_untracked();
                let h = highlight.get_untracked().min(disp.len().saturating_sub(1));
                match k.key {
                    Key::Down => highlight.set(step_highlight(&options, &disp, h, 1)),
                    Key::Up => highlight.set(step_highlight(&options, &disp, h, -1)),
                    Key::Home => {
                        if let Some(p) = first_enabled(&options, &disp) {
                            highlight.set(p);
                        }
                    }
                    Key::End => {
                        if let Some(p) = last_enabled(&options, &disp) {
                            highlight.set(p);
                        }
                    }
                    Key::PageDown => {
                        highlight.set(page_highlight(&options, &disp, h, 1, max_visible))
                    }
                    Key::PageUp => {
                        highlight.set(page_highlight(&options, &disp, h, -1, max_visible))
                    }
                    Key::Char(' ') => toggle(h),
                    Key::Enter => commit(),
                    _ => return, // Escape and the rest: substrate's turn
                }
                ctx.stop_propagation();
            }
        };
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
                    checks: Some(working),
                    max_visible,
                    // A click toggles — same gesture as Space.
                    on_activate: toggle,
                },
            ))
            .build()
    }
}

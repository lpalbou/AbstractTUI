//! Combobox (backlog 0500 face 3): a searchable single-choice control.
//! Private sibling of select.rs; public path `app::select::Combobox`.
//!
//! The open popup INCLUDES the anchor row (`open_including_anchor_row`)
//! and mounts a real [`TextInput`](crate::widgets::TextInput) there —
//! the editor renders at exactly the trigger's screen position (zero
//! visual jump; when the popup flips above, the editor is the LAST
//! row, still over the trigger). Typing filters options
//! (case-insensitive substring); the FILTER TEXT IS NEVER THE VALUE —
//! Enter commits the highlighted match and a non-matching buffer
//! commits nothing. The option count / "no matches" status line is
//! part of the popup.
//!
//! Key ownership (the spec's per-face contract): the popup owns
//! Up/Down/PageUp/PageDown (highlight) and Enter (commit) at capture
//! phase; every other key — printables, Home/End cursor motion,
//! Backspace — belongs to the editor. Escape falls through to the
//! substrate (dismiss without commit).
//!
//! OWNER: SELECT (0500).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, Element, EventCtx, Key, Mods, MouseButton, MouseKind, Phase, Role, UiEvent,
};

use super::super::anchored::{DismissReason, PanelAnchor, PanelWidth, Popup};
use super::super::overlays::Overlays;
use super::super::viewport::use_viewport;
use super::core::{
    first_enabled, option_rows_view, page_highlight, resolve_overlays, step_highlight,
    trigger_view, OptionRows, TriggerLabel, DEFAULT_MAX_VISIBLE,
};
use super::SelectOption;

struct ComboSession {
    popup: Option<Popup>,
}

/// Searchable single-choice control: a one-row trigger; the open popup
/// mounts an editor over the trigger row and filters the options as
/// you type. Bound to a `Signal<usize>` (index into the options;
/// out-of-range = nothing chosen yet). `on_change` fires on commit
/// only, and only when the committed index differs.
///
/// ```ignore
/// let model = cx.signal(usize::MAX); // nothing chosen yet
/// Combobox::new(models).value(model).placeholder("model…").view(cx)
/// ```
pub struct Combobox {
    options: Vec<SelectOption>,
    value: Option<Signal<usize>>,
    placeholder: String,
    disabled: bool,
    max_visible: usize,
    layout: Option<LayoutStyle>,
    overlays: Option<Overlays>,
    on_change: Option<Box<dyn FnMut(usize)>>,
}

impl Combobox {
    pub fn new(options: Vec<SelectOption>) -> Combobox {
        Combobox {
            options,
            value: None,
            placeholder: String::from("search…"),
            disabled: false,
            max_visible: DEFAULT_MAX_VISIBLE,
            layout: None,
            overlays: None,
            on_change: None,
        }
    }

    /// Bind the chosen INDEX (external signal; default internal).
    pub fn value(mut self, value: Signal<usize>) -> Combobox {
        self.value = Some(value);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Combobox {
        self.placeholder = text.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Combobox {
        self.disabled = disabled;
        self
    }

    /// Popup option rows shown at once (default 8).
    pub fn max_visible(mut self, n: usize) -> Combobox {
        self.max_visible = n.max(1);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Combobox {
        self.layout = Some(layout);
        self
    }

    /// Explicit overlay store (tests, exotic embeddings). Default:
    /// the app-provided reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> Combobox {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires on COMMIT (Enter/click on a match), and only when the
    /// committed index differs from the bound value.
    pub fn on_change(mut self, f: impl FnMut(usize) + 'static) -> Combobox {
        self.on_change = Some(Box::new(f));
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
        let value = self.value.unwrap_or_else(|| cx.signal(usize::MAX));
        let placeholder = self.placeholder;
        let disabled = self.disabled;
        let max_visible = self.max_visible;
        let overlays = resolve_overlays(cx, self.overlays);
        let viewport = use_viewport(cx);
        let on_change: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_change));

        let focused = cx.signal(false);
        let hovered = cx.signal(false);
        let filter: Signal<String> = cx.signal(String::new());
        let display: Signal<Vec<usize>> = cx.signal((0..options.len()).collect());
        let highlight: Signal<usize> = cx.signal(0);
        let session: Rc<RefCell<ComboSession>> =
            Rc::new(RefCell::new(ComboSession { popup: None }));

        // ONE refilter authority (used eagerly at open and by the
        // typing effect): empty filter shows everything with the
        // highlight seeded on the committed option; a query filters by
        // case-insensitive substring and highlights the first match.
        let refilter = Rc::new({
            let options = options.clone();
            move || {
                let q = filter.get_untracked().to_lowercase();
                let disp: Vec<usize> = if q.is_empty() {
                    (0..options.len()).collect()
                } else {
                    (0..options.len())
                        .filter(|&i| options[i].label.to_lowercase().contains(&q))
                        .collect()
                };
                let seed = if q.is_empty() {
                    let current = value.get_untracked();
                    disp.iter()
                        .position(|&i| i == current)
                        .filter(|&p| options.get(disp[p]).is_some_and(|o| !o.disabled))
                        .or_else(|| first_enabled(&options, &disp))
                } else {
                    first_enabled(&options, &disp)
                };
                highlight.set(seed.unwrap_or(0));
                display.set(disp);
            }
        });
        {
            let refilter = refilter.clone();
            cx.effect_labeled("combobox-filter", move || {
                let _ = filter.get(); // subscribe to typing
                refilter();
            });
        }

        // Commit = write-if-different + notify; the filter text never
        // becomes the value.
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
            let refilter = refilter.clone();
            let commit_and_close = commit_and_close.clone();
            let overlays = overlays.clone();
            let popup_placeholder = placeholder.clone();
            move |anchor: Rect| {
                if session.borrow().popup.is_some() {
                    return;
                }
                let Some(overlays) = overlays.clone() else {
                    debug_assert!(
                        false,
                        "Combobox: no Overlays available — build inside an App (context) \
                         or pass .overlays(..) explicitly"
                    );
                    return;
                };
                if options.is_empty() {
                    return;
                }
                // Fresh session: empty filter, eager refilter so the
                // popup mounts with truthful rows (the effect flush
                // would land one frame later).
                filter.set(String::new());
                refilter();
                // FIXED extent at open: editor row is the anchor row;
                // list = window rows + one status line. Filtering
                // narrows CONTENT, never remounts the layer.
                let rows = options.len().min(max_visible) as i32 + 1;
                let build = combobox_popup_content(
                    tokens,
                    options.clone(),
                    filter,
                    display,
                    highlight,
                    max_visible,
                    popup_placeholder.clone(),
                    commit_and_close.clone(),
                );
                let popup = Popup::open_including_anchor_row(
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
                        session.borrow_mut().popup = None;
                    }
                });
                session.borrow_mut().popup = Some(popup);
            }
        });

        let access_options = options.clone();
        let access_placeholder = placeholder.clone();
        let mut el = Element::new()
            .style(self.layout.unwrap_or_else(|| {
                LayoutStyle::default()
                    .height(Dimension::Cells(1))
                    .grow(1.0)
                    .shrink(0.0)
            }))
            // `Button` until `Role::Select` lands in the 0.3 batch
            // (budget 0002 entry 1); the choice rides the access value.
            .role(Role::Button)
            .access_label(placeholder.clone())
            .access_value(move || {
                let v = value.get_untracked();
                access_options
                    .get(v)
                    .map(|o| o.label.clone())
                    .unwrap_or_else(|| access_placeholder.clone())
            })
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

/// Popup content: editor over the anchor row, option rows, and the
/// status line — ordered against gravity when flipped (the editor
/// stays on the trigger row either way).
#[allow(clippy::too_many_arguments)]
fn combobox_popup_content(
    tokens: TokenSet,
    options: Rc<Vec<SelectOption>>,
    filter: Signal<String>,
    display: Signal<Vec<usize>>,
    highlight: Signal<usize>,
    max_visible: usize,
    placeholder: String,
    commit_and_close: Rc<dyn Fn()>,
) -> impl FnOnce(Scope, bool) -> crate::ui::View {
    move |pcx: Scope, flipped: bool| {
        let ground = tokens.surface_raised;
        let ink = tokens.text;
        let muted = tokens.text_muted;
        let total = options.len();
        // Capture-phase interception: the popup owns navigation +
        // commit BEFORE the editor sees the key; everything else
        // (printables, Home/End, Backspace) falls through to it.
        let nav_handler = {
            let options = options.clone();
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
                    Key::PageDown => {
                        highlight.set(page_highlight(&options, &disp, h, 1, max_visible))
                    }
                    Key::PageUp => {
                        highlight.set(page_highlight(&options, &disp, h, -1, max_visible))
                    }
                    Key::Enter => {
                        // A non-matching buffer commits NOTHING (the
                        // popup stays; Escape is the way out).
                        if !disp.is_empty() {
                            commit();
                        }
                    }
                    _ => return,
                }
                ctx.stop_propagation();
            }
        };
        let editor = crate::widgets::TextInput::new()
            .value(filter)
            .placeholder(placeholder)
            .layout(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1))
                    .shrink(0.0),
            )
            .element(pcx, &tokens)
            .autofocus()
            .build();
        let rows = option_rows_view(
            &tokens,
            OptionRows {
                options,
                display,
                highlight,
                checks: None,
                max_visible,
                on_activate: Rc::new({
                    let commit = commit_and_close.clone();
                    move |pos: usize| {
                        highlight.set(pos);
                        commit();
                    }
                }),
            },
        );
        let status = dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
            let matches = display.get().len();
            let text = if matches == 0 {
                String::from("no matches")
            } else {
                format!("{matches} of {total}")
            };
            Element::new()
                .style(LayoutStyle::line(1))
                .draw(move |canvas, rect| {
                    let style = Style::new().fg(muted).bg(ground);
                    canvas.fill_styled(rect, ' ', &style);
                    canvas.print_styled(crate::base::Point::new(rect.x + 1, rect.y), &text, &style);
                })
                .build()
        });
        let mut root = Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .role(Role::Menu)
            .access_label("options")
            .on(Phase::Capture, nav_handler)
            .draw(move |canvas, rect| {
                canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
            });
        // The editor sits ON the anchor row: first row below-mode,
        // last row when flipped (the anchor row is the popup's bottom).
        if flipped {
            root = root.child(status).child(rows).child(editor);
        } else {
            root = root.child(editor).child(rows).child(status);
        }
        root.build()
    }
}

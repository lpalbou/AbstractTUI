//! The ReasoningSelect popup half (private sibling of reasoning.rs,
//! split for the file-size discipline — the select.rs/select_core.rs
//! pattern): shared open-state, the per-state row plan, commit
//! semantics, and the open/reopen path the unknown-state override
//! drives. Nothing here is public API; the widget face is.
//!
//! OWNER: REASON (app-kits/1250).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{Element, EventCtx, Key, Mods, Phase, Role, UiEvent, View};

use super::super::anchored::{DismissReason, PanelAnchor, PanelWidth, Popup};
use super::super::overlays::Overlays;
use super::super::select::core::{
    first_enabled, last_enabled, option_rows_view, page_highlight, step_highlight,
    type_ahead_target, OptionRows, TypeAhead,
};
use super::super::select::SelectOption;
use super::{Mode, OVERRIDE_ROW, REASONING_AUTO, REASONING_LADDER};

/// What a popup row does when activated.
#[derive(Clone)]
enum RowPlan {
    /// Commit this value.
    Value(String),
    /// The unknown-state "set anyway" row: unlock the full ladder.
    Override,
}

/// Session state behind one widget instance (open popup, type-ahead
/// buffer, the unknown-state unlock latch, the last open's anchor).
pub(super) struct Session {
    pub(super) popup: Option<Popup>,
    pub(super) type_ahead: TypeAhead,
    /// "set anyway" was activated on this INSTANCE: opens go straight
    /// to the full ladder. Dies with the widget — remounting with
    /// fresh facts (the model-change recipe) relocks.
    pub(super) unlocked: bool,
    /// Anchor of the current open — the override's reopen target.
    pub(super) anchor: Rect,
}

/// The `on_change` callback shape (the argument is `&str`, a
/// higher-ranked borrow the crate-wide `SharedCallback` alias cannot
/// spell — hence a local pair of aliases).
pub(super) type ChangeFn = Box<dyn FnMut(&str)>;

/// One `on_change` slot shared by every commit path.
pub(super) type ChangeSlot = Rc<RefCell<Option<ChangeFn>>>;

/// Everything the popup paths share, behind one `Rc` (the ThemeSwitcher
/// free-function shape, one struct instead of ten arguments — the
/// override row REOPENS the popup, so open must be a plain function
/// over shared state, never a closure that would have to capture
/// itself).
pub(super) struct Shared {
    pub(super) cx: Scope,
    pub(super) tokens: TokenSet,
    pub(super) mode: Mode,
    /// CAPABLE rows: `auto`, `none`, then the declared levels
    /// (verbatim, deduplicated).
    pub(super) offered: Vec<String>,
    pub(super) max_visible: usize,
    pub(super) value: Signal<String>,
    /// A value was committed through the unknown-state ladder: the
    /// lock annotation clears (display only — facts stay unknown).
    pub(super) overridden: Signal<bool>,
    pub(super) display: Signal<Vec<usize>>,
    pub(super) highlight: Signal<usize>,
    pub(super) on_change: ChangeSlot,
    pub(super) session: Rc<RefCell<Session>>,
    pub(super) overlays: Overlays,
    pub(super) viewport: Signal<Size>,
}

impl Shared {
    /// The value the control currently PRESENTS: locked displays pin
    /// to "none" whatever the bound signal holds (the widget never
    /// writes the app's signal uninvited — display truth only).
    fn effective(&self) -> String {
        if self.locked_untracked() {
            "none".into()
        } else {
            self.value.get_untracked()
        }
    }

    fn locked_untracked(&self) -> bool {
        match self.mode {
            Mode::NonReasoning => true,
            Mode::Unknown => !self.overridden.get_untracked(),
            Mode::Capable => false,
        }
    }
}

/// Commit `v`: write-if-different against the EFFECTIVE value (a
/// locked display pins to "none", so committing "auto" from a locked
/// unknown state IS a change even when the signal already said auto),
/// state writes BEFORE the callback (the 0297 disposal law), then
/// close.
fn commit(shared: &Rc<Shared>, v: &str) {
    let changed = shared.effective() != v;
    if changed {
        shared.value.set(v.to_string());
        if shared.mode == Mode::Unknown {
            shared.overridden.set(true);
        }
    }
    let popup = shared.session.borrow().popup.clone();
    if changed {
        if let Some(f) = shared.on_change.borrow_mut().as_mut() {
            f(v);
        }
    }
    if let Some(popup) = popup {
        popup.dismiss(DismissReason::Commit);
    }
}

/// The rows one open shows, per state (and the unknown unlock latch).
fn row_plan(shared: &Shared) -> (Vec<SelectOption>, Vec<RowPlan>, Option<usize>) {
    let values: Vec<String> = match shared.mode {
        Mode::Capable => shared.offered.clone(),
        Mode::Unknown if shared.session.borrow().unlocked => {
            std::iter::once(REASONING_AUTO.to_string())
                .chain(REASONING_LADDER.iter().map(|s| s.to_string()))
                .collect()
        }
        Mode::Unknown => {
            return (
                vec![SelectOption::new(OVERRIDE_ROW)],
                vec![RowPlan::Override],
                Some(0),
            )
        }
        // No open path exists for a non-reasoning model.
        Mode::NonReasoning => Vec::new(),
    };
    let current = shared.effective();
    let mut options = Vec::with_capacity(values.len());
    let mut plan = Vec::with_capacity(values.len());
    let mut seed = None;
    for (i, v) in values.into_iter().enumerate() {
        let mut opt = SelectOption::new(v.clone());
        if v == current {
            // The ● current mark (select-family hint vocabulary).
            opt = opt.hint("●");
            seed = Some(i);
        }
        options.push(opt);
        plan.push(RowPlan::Value(v));
    }
    (options, plan, seed)
}

/// Open (or REOPEN, after the override unlock) the popup at `anchor`.
pub(super) fn open_popup(shared: &Rc<Shared>, anchor: Rect) {
    if shared.session.borrow().popup.is_some() {
        return;
    }
    let (options, plan, seed) = row_plan(shared);
    if options.is_empty() {
        return;
    }
    let options: Rc<Vec<SelectOption>> = Rc::new(options);
    let plan: Rc<Vec<RowPlan>> = Rc::new(plan);
    let disp: Vec<usize> = (0..options.len()).collect();
    let seed = seed.or_else(|| first_enabled(&options, &disp)).unwrap_or(0);
    shared.display.set(disp);
    shared.highlight.set(seed);
    {
        let mut s = shared.session.borrow_mut();
        s.type_ahead.clear();
        s.anchor = anchor;
    }

    // Window cap by the roomier side of the anchor (the ThemeSwitcher
    // rule: the highlight window must live inside the solved rect).
    let viewport = shared.viewport.get_untracked();
    let below = (viewport.h - anchor.bottom()).max(0);
    let above = anchor.y.max(0);
    let room = below.max(above).max(1) as usize;
    let visible = options.len().min(shared.max_visible).min(room);
    // Width: widest label + left pad (1) + gap (2) + ● hint (1) +
    // right pad (1).
    let width = options
        .iter()
        .map(|o| crate::text::width(&o.label))
        .max()
        .unwrap_or(8)
        + 5;

    let activate: Rc<dyn Fn(usize)> = Rc::new({
        let shared = shared.clone();
        let plan = plan.clone();
        move |pos: usize| {
            let disp = shared.display.get_untracked();
            let Some(row) = disp.get(pos).and_then(|&ix| plan.get(ix)) else {
                return;
            };
            match row {
                RowPlan::Value(v) => commit(&shared, &v.clone()),
                RowPlan::Override => {
                    // Unlock, close this one-row popup, reopen with
                    // the full ladder at the same anchor (dismiss is
                    // synchronous — the select substrate's contract).
                    let anchor = {
                        let mut s = shared.session.borrow_mut();
                        s.unlocked = true;
                        s.anchor
                    };
                    let popup = shared.session.borrow().popup.clone();
                    if let Some(popup) = popup {
                        popup.dismiss(DismissReason::Commit);
                    }
                    open_popup(&shared, anchor);
                }
            }
        }
    });

    let build = {
        let shared = shared.clone();
        let options = options.clone();
        let activate = activate.clone();
        move |_pcx: Scope, _flipped: bool| -> View {
            let t = shared.tokens;
            let ink = t.text;
            let ground = t.surface_raised;
            let key_handler = {
                let shared = shared.clone();
                let options = options.clone();
                let activate = activate.clone();
                move |ctx: &mut EventCtx, ev: &UiEvent| {
                    let UiEvent::Key(k) = ev else { return };
                    if k.mods != Mods::NONE {
                        return;
                    }
                    let disp = shared.display.get_untracked();
                    let h = shared
                        .highlight
                        .get_untracked()
                        .min(disp.len().saturating_sub(1));
                    let move_to = |pos: usize| shared.highlight.set(pos);
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
                        Key::PageDown => {
                            move_to(page_highlight(&options, &disp, h, 1, visible));
                        }
                        Key::PageUp => {
                            move_to(page_highlight(&options, &disp, h, -1, visible));
                        }
                        Key::Enter => activate(h),
                        Key::Char(c) if !c.is_control() => {
                            // Type-ahead on the injectable event clock
                            // (the select-core contract).
                            let now =
                                crate::ui::event_time().unwrap_or_else(std::time::Instant::now);
                            let target = {
                                let mut s = shared.session.borrow_mut();
                                let buf = s.type_ahead.push(c, now);
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
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .role(Role::Menu)
                .access_label("reasoning")
                .on(Phase::Bubble, key_handler)
                .draw(move |canvas, rect| {
                    canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
                })
                .child(option_rows_view(
                    &t,
                    OptionRows {
                        options: options.clone(),
                        display: shared.display,
                        highlight: shared.highlight,
                        checks: None,
                        max_visible: visible,
                        on_activate: activate.clone(),
                    },
                ))
                .build()
        }
    };

    let popup = Popup::open(
        &shared.overlays,
        shared.cx,
        viewport,
        PanelAnchor { rect: anchor },
        PanelWidth::Content {
            min: width.min(viewport.w.max(1)),
            max: width,
        },
        Size::new(width, visible as i32),
        build,
    );
    let Some(popup) = popup else { return };
    popup.on_dismiss({
        let session = shared.session.clone();
        move |_reason| {
            session.borrow_mut().popup = None;
        }
    });
    shared.session.borrow_mut().popup = Some(popup);
}

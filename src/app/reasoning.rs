//! [`ReasoningSelect`] — the reasoning-effort control (backlog
//! app-kits/1250): the drop-in footer/settings picker for a model's
//! thinking effort, plus [`reasoning_label`]/[`reasoning_label_glyph`],
//! the ONE label grammar shared by every AbstractFramework console
//! footer (code-tui/code/console parity).
//!
//! ## The contract (shared with the abstractuic kit — see docs/api.md)
//!
//! The effort ladder is `none | minimal | low | medium | high | xhigh`,
//! plus `auto` (provider default). The UI word is "reasoning"; the wire
//! key apps write is `thinking` — THE ENGINE MINTS NO WIRE VOCABULARY:
//! this control renders and emits VALUES (`on_change("high")`), the app
//! does the writing. Capability facts arrive AS DATA ([`ReasoningFacts`],
//! parsed by the app from the gateway-served `reasoning` block); the
//! widget enforces the three-state coupling:
//!
//! - CAPABLE (`thinking_support=true`): the popup offers `auto`, `none`
//!   and the DECLARED levels only — verbatim, deduplicated, in declared
//!   order. Unknown level strings render verbatim (the gateway is the
//!   authority on what a model supports; filtering here would mint
//!   vocabulary the engine does not own). An empty declared list offers
//!   `auto`/`none` alone.
//! - NON-REASONING (`thinking_support=false`): the control renders
//!   LOCKED — `r: none (locked) — model does not reason` — faint, out
//!   of the focus order, and refuses to open (the select-family
//!   disabled convention).
//! - UNKNOWN (block absent): locked to `none` by default, but openable —
//!   the popup holds one row, `set anyway (capability unknown — passed
//!   verbatim)`, whose activation unlocks the FULL ladder for this
//!   widget instance. The lock annotation clears only when a value is
//!   actually committed through that ladder.
//!
//! ## Reset on model change (the recipe)
//!
//! The widget does NOT own provider/model coupling: facts are
//! constructor data, immutable for the instance. When the app's model
//! changes, REMOUNT the control with fresh facts (build it inside a
//! `dyn_view` that reads the model signal — `examples/reasoning.rs`
//! does exactly this). Remounting also resets the unknown-state
//! override: a stale "set anyway" can never leak onto the next model.
//!
//! ## The lock spelling (design note, research recorded in the item)
//!
//! There is no padlock outside Unicode emoji-data (U+1F512 🔒 is
//! Emoji=Yes, double-width — rejected outright). Probed against BOTH
//! unicode-width conventions (`width` vs `width_cjk`, the crate's own
//! ambiguity oracle): `⚿` U+26BF, `×` U+00D7 and `≢` U+2262 are
//! East-Asian-AMBIGUOUS (double-width under ambiguous-wide terminals —
//! the `◐` risk the ThemeSwitcher note names); `⊘` U+2298 CIRCLED
//! DIVISION SLASH measures 1 in both conventions, sits in Mathematical
//! Operators (a block emoji-data never touches), and carries the
//! no-entry mnemonic. The PLAIN annotation `(locked)` remains the
//! CANONICAL grammar — self-describing, ASCII-safe, and what a screen
//! reader hears; the glyph form exists for width-tight footers.
//!
//! ## Zero idle
//!
//! Closed, the control is one dormant element + one label `dyn_view`:
//! no layers, no timers, no per-frame work. The popup and its
//! subscriptions live on a per-open child scope and die at dismissal.
//!
//! OWNER: REASON (composing SELECT's 0500 substrate — the ThemeSwitcher
//! precedent).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::Rect;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{Element, EventCtx, Key, Mods, MouseButton, MouseKind, Phase, Role, UiEvent, View};

use super::overlays::Overlays;
use super::select::core::{
    resolve_overlays, trigger_view, TriggerLabel, TypeAhead, DEFAULT_MAX_VISIBLE,
};
use super::viewport::use_viewport;

// The popup half (shared open-state, row plans, commit, open/reopen) —
// private sibling for the file-size discipline (the select_core.rs
// pattern; nothing in it is public API).
#[path = "reasoning_open.rs"]
mod popup;
use popup::{open_popup, ChangeFn, ChangeSlot, Session, Shared};

/// The effort ladder (shared contract vocabulary, low to high —
/// `auto` is [`REASONING_AUTO`], deliberately separate: it means
/// "provider default", not an effort step).
pub const REASONING_LADDER: [&str; 6] = ["none", "minimal", "low", "medium", "high", "xhigh"];

/// The provider-default value: apps conventionally write NO wire key
/// for it (the provider decides).
pub const REASONING_AUTO: &str = "auto";

/// The locked marker of the glyph-bearing label form — see the module
/// docs' research note (`⊘` U+2298: non-emoji, narrow in both width
/// conventions).
const LOCK_GLYPH: &str = "\u{2298}";

/// The one-row why-line rendered beside the locked label when the row
/// has room (the trigger's SHORT fallback drops it first).
const WHY_NON_REASONING: &str = "model does not reason";
const WHY_UNKNOWN: &str = "capability unknown";

/// The unknown-state override row (contract wording, verbatim).
const OVERRIDE_ROW: &str = "set anyway (capability unknown — passed verbatim)";

/// Lock state of a reasoning display — the second input to the shared
/// label grammar. `Locked` covers both locked shapes (non-reasoning
/// model, unknown capability): the ANNOTATION is the same; the why
/// differs and rides the control's own why-line/a11y value.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LockState {
    Unlocked,
    Locked,
}

/// The parity label grammar, PLAIN (canonical) form: `r: <value>`,
/// locked adds ` (locked)`. One source for every footer — code-tui,
/// code, console — so the spelling can never drift between apps.
pub fn reasoning_label(value: &str, state: LockState) -> String {
    match state {
        LockState::Unlocked => format!("r: {value}"),
        LockState::Locked => format!("r: {value} (locked)"),
    }
}

/// The glyph-bearing form for width-tight footers: `r: <value>`,
/// locked adds ` ⊘` (U+2298 — the research note in the module docs).
/// The plain form is canonical; prefer it wherever the three extra
/// cells fit (it is also what a screen reader hears).
pub fn reasoning_label_glyph(value: &str, state: LockState) -> String {
    match state {
        LockState::Unlocked => format!("r: {value}"),
        LockState::Locked => format!("r: {value} {LOCK_GLYPH}"),
    }
}

/// Capability facts the app feeds the widget — the gateway-served
/// `reasoning{thinking_support, reasoning_levels}` block AS DATA (the
/// thin-client rule: apps parse, the engine never sees wire JSON).
/// Three honest states, one constructor each:
///
/// - [`ReasoningFacts::capable`] — `thinking_support=true` + declared
///   levels (`support: Some(true)`).
/// - [`ReasoningFacts::non_reasoning`] — `thinking_support=false`
///   (`support: Some(false)`).
/// - [`ReasoningFacts::unknown`] — the block was ABSENT
///   (`support: None`) — also `Default`.
///
/// `#[non_exhaustive]`: the shared contract may grow fields (budget
/// hints, per-level costs) without a major version.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReasoningFacts {
    /// `Some(true)` = the model reasons; `Some(false)` = it does not;
    /// `None` = the capability block was absent (unknown).
    pub support: Option<bool>,
    /// Declared effort levels (meaningful with `support == Some(true)`;
    /// rendered verbatim, deduplicated, declared order).
    pub levels: Vec<String>,
}

impl ReasoningFacts {
    /// The capability block was absent — the widget locks to `none`
    /// with the "set anyway" override.
    pub fn unknown() -> ReasoningFacts {
        ReasoningFacts::default()
    }

    /// `thinking_support=true` with the declared levels.
    pub fn capable<I, S>(levels: I) -> ReasoningFacts
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ReasoningFacts {
            support: Some(true),
            levels: levels.into_iter().map(Into::into).collect(),
        }
    }

    /// `thinking_support=false` — the model does not reason.
    pub fn non_reasoning() -> ReasoningFacts {
        ReasoningFacts {
            support: Some(false),
            levels: Vec::new(),
        }
    }
}

/// The three coupling states, resolved once from the facts.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Mode {
    Capable,
    NonReasoning,
    Unknown,
}

/// The reasoning-effort control. `ReasoningSelect::new(facts).view(cx)`
/// in a footer or settings row; see the module docs for the three-state
/// contract and the model-change recipe.
///
/// ```ignore
/// let effort = cx.signal(String::from("auto"));
/// ReasoningSelect::new(ReasoningFacts::capable(["low", "medium", "high"]))
///     .value(effort)
///     .on_change(|v| set_wire_thinking(v)) // the APP writes `thinking`
///     .view(cx)
/// ```
pub struct ReasoningSelect {
    facts: ReasoningFacts,
    value: Option<Signal<String>>,
    max_visible: usize,
    layout: Option<LayoutStyle>,
    overlays: Option<Overlays>,
    on_change: Option<ChangeFn>,
}

impl ReasoningSelect {
    pub fn new(facts: ReasoningFacts) -> ReasoningSelect {
        ReasoningSelect {
            facts,
            value: None,
            max_visible: DEFAULT_MAX_VISIBLE,
            layout: None,
            overlays: None,
            on_change: None,
        }
    }

    /// Bind the chosen VALUE (external signal; default internal,
    /// starting at [`REASONING_AUTO`]). The widget never writes it
    /// uninvited: locked states pin the DISPLAY to "none" and leave
    /// the signal alone.
    pub fn value(mut self, value: Signal<String>) -> ReasoningSelect {
        self.value = Some(value);
        self
    }

    /// Popup rows shown at once (default 8; the full ladder is 7).
    pub fn max_visible(mut self, n: usize) -> ReasoningSelect {
        self.max_visible = n.max(1);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> ReasoningSelect {
        self.layout = Some(layout);
        self
    }

    /// Explicit overlay store (tests, exotic embeddings). Default: the
    /// app-provided reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> ReasoningSelect {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires on COMMIT (Enter/click), once, and only when the
    /// committed value differs from the EFFECTIVE one (the
    /// select-family 0250 rule; a locked display's effective value is
    /// "none", so the first unknown-state override commit of any other
    /// value fires). State is written before the callback — disposing
    /// the widget's scope inside it is safe (backlog 0297).
    pub fn on_change(mut self, f: impl FnMut(&str) + 'static) -> ReasoningSelect {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Canonical one-call build: tokens resolve from the app's theme
    /// context; state lives on `cx`.
    pub fn view(self, cx: Scope) -> View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let mode = match self.facts.support {
            Some(true) => Mode::Capable,
            Some(false) => Mode::NonReasoning,
            None => Mode::Unknown,
        };
        // CAPABLE rows: auto, none, then the declared levels verbatim —
        // deduplicated, structural rows never doubled, empties dropped
        // (a level that renders zero glyphs is not a row).
        let mut offered: Vec<String> = vec![REASONING_AUTO.to_string(), "none".to_string()];
        for level in &self.facts.levels {
            if !level.is_empty() && !offered.iter().any(|v| v == level) {
                offered.push(level.clone());
            }
        }

        let value = self
            .value
            .unwrap_or_else(|| cx.signal(REASONING_AUTO.to_string()));
        let overridden: Signal<bool> = cx.signal(false);
        let display: Signal<Vec<usize>> = cx.signal(Vec::new());
        let highlight: Signal<usize> = cx.signal(0);
        let on_change: ChangeSlot = Rc::new(RefCell::new(self.on_change));
        let session = Rc::new(RefCell::new(Session {
            popup: None,
            type_ahead: TypeAhead::default(),
            unlocked: false,
            anchor: Rect::new(0, 0, 0, 0),
        }));
        let focused = cx.signal(false);
        let hovered = cx.signal(false);

        let shared: Option<Rc<Shared>> = resolve_overlays(cx, self.overlays).map(|overlays| {
            Rc::new(Shared {
                cx,
                tokens: *t,
                mode,
                offered,
                max_visible: self.max_visible,
                value,
                overridden,
                display,
                highlight,
                on_change,
                session,
                overlays,
                viewport: use_viewport(cx),
            })
        });

        let locked = matches!(mode, Mode::NonReasoning);
        let mut el = Element::new()
            .style(self.layout.unwrap_or_else(|| {
                LayoutStyle::default()
                    .height(Dimension::Cells(1))
                    .grow(1.0)
                    .shrink(0.0)
            }))
            // `Button`, not a dedicated role: the Role enum is frozen
            // until 0.3 (the Select precedent).
            .role(Role::Button)
            .access_label("reasoning")
            .access_value(move || match mode {
                Mode::NonReasoning => format!("none (locked — {WHY_NON_REASONING})"),
                Mode::Unknown if !overridden.get_untracked() => {
                    format!("none (locked — {WHY_UNKNOWN})")
                }
                Mode::Unknown => format!(
                    "{} ({WHY_UNKNOWN} — passed verbatim)",
                    value.get_untracked()
                ),
                Mode::Capable => value.get_untracked(),
            })
            .hover_signal(hovered)
            .focus_signal(focused);
        if !locked {
            if let Some(shared) = shared.clone() {
                let open = move |ctx: &mut EventCtx| {
                    let anchor = ctx.current_rect_screen();
                    open_popup(&shared, anchor);
                };
                el = el.focusable().on(Phase::Bubble, move |ctx, ev| match ev {
                    UiEvent::Key(k)
                        if (k.key == Key::Enter || k.key == Key::Char(' '))
                            && k.mods == Mods::NONE =>
                    {
                        // Keyboard activation requires FOCUS (the
                        // Button rule).
                        if focused.get_untracked() {
                            open(ctx);
                            ctx.stop_propagation();
                        }
                    }
                    UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)) => {
                        open(ctx);
                        ctx.stop_propagation();
                    }
                    _ => {}
                });
            } else {
                debug_assert!(
                    false,
                    "ReasoningSelect: no Overlays available — build inside an App \
                     (context) or pass .overlays(..) explicitly"
                );
            }
        }
        // The trigger face: the shared grammar as the label; locked
        // shapes carry the why-line, dropping to the bare grammar when
        // the row is tight (the trigger's SHORT fallback).
        el.child(trigger_view(
            t,
            focused,
            hovered,
            locked,
            Rc::new(move || match mode {
                Mode::NonReasoning => TriggerLabel {
                    text: format!(
                        "{} — {WHY_NON_REASONING}",
                        reasoning_label("none", LockState::Locked)
                    ),
                    short: Some(reasoning_label("none", LockState::Locked)),
                    placeholder: true,
                },
                Mode::Unknown if !overridden.get() => TriggerLabel {
                    text: format!(
                        "{} — {WHY_UNKNOWN}",
                        reasoning_label("none", LockState::Locked)
                    ),
                    short: Some(reasoning_label("none", LockState::Locked)),
                    placeholder: false,
                },
                _ => TriggerLabel {
                    text: reasoning_label(&value.get(), LockState::Unlocked),
                    short: None,
                    placeholder: false,
                },
            }),
        ))
    }
}

#[cfg(test)]
#[path = "reasoning_tests.rs"]
mod tests;

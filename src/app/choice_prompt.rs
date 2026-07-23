//! ChoicePrompt (backlog 0515): the modal DECISION GATE — one
//! structured question (a prompt, N options, optionally multiple
//! answers, optionally an "Other" free-text choice) that blocks the
//! app's flow until it resolves EXACTLY ONCE through `on_resolve`:
//! `Answered(ChoiceAnswer)` or `Cancelled`. Agent-approval dialogs,
//! setup choices, destructive-action confirmations with alternatives.
//!
//! It lives app-side (not `widgets`) for the same reason
//! `Modal`/`KeymapHelp`/the select family do: opening needs the
//! overlay store, and `widgets` sits below `app` in the layer map
//! (integrator ruling R4-1). It is otherwise a plain token-consuming
//! component (RT1-9b: tokens only).
//!
//! ## The gate contract
//!
//! - **Exactly-once, never silent**: every ending — Enter-commit,
//!   click-commit, the Confirm/Cancel buttons, Escape, programmatic
//!   [`ChoicePromptHandle::cancel`] — funnels through one resolve
//!   path. The modal closes (layer removed, state disposed) BEFORE
//!   `on_resolve` runs (the 0297 disposal-safety law), so the callback
//!   may dispose the opener's scope or immediately open the next
//!   prompt ([`ChoiceSequence`] is built on exactly that).
//! - **Outside-press does NOT dismiss**: a decision gate has explicit
//!   endings only (Escape and the Cancel button always exist); a stray
//!   click is swallowed by the modal and acts on nothing.
//! - **Movement is not activation** (the 0250 ruling): arrows move the
//!   candidate/highlight; only Enter, a click on the already-selected
//!   row (single mode), or Confirm (multiple mode) commit.
//! - Degenerate opens (no overlay store; a question with zero options
//!   and no Other) resolve `Cancelled` immediately instead of hanging
//!   the gated flow, with a debug assertion naming the mistake.
//!
//! ```ignore
//! ChoicePrompt::new("Overwrite 3 modified files?")
//!     .option_detail("overwrite", "Overwrite them", "the working copies are lost")
//!     .option("keep", "Keep my copies")
//!     .allow_other("Something else…")
//!     .on_resolve(|outcome| match outcome {
//!         ChoiceOutcome::Answered(a) => apply(a),
//!         ChoiceOutcome::Cancelled => (),
//!     })
//!     .open(cx);
//! ```
//!
//! OWNER: CHOICE (0515).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::reactive::Scope;

use super::overlays::Overlays;
use super::popups::Modal;
use super::viewport::use_viewport;

#[path = "choice_prompt_interact.rs"]
mod interact;
#[path = "choice_prompt_parts.rs"]
mod parts;
#[path = "choice_prompt_view.rs"]
mod view;

/// One selectable alternative. `id` is the stable identity carried in
/// the answer (the caller's vocabulary — never the display label);
/// `detail` renders as its own muted row under the label, so a
/// decision's fine print stays visible where a right-aligned hint
/// would vanish on narrow widths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    /// Per-option shortcut letter (wave-5 F2, the approval consumer's
    /// `a`/`A`/`d` muscle memory). CASE-SENSITIVE ('a' and 'A' are two
    /// keys); rendered dim in the row and named in the hint. Pressing
    /// it selects+commits in single mode, jump-toggles in multiple —
    /// an EXPLICIT activation (a declared shortcut is not movement, so
    /// the 0250 ruling is untouched). Letters route to options only
    /// while the Other editor is NOT focused (its own key consumption
    /// shields them — typing "a" into Other types).
    pub key: Option<char>,
    /// Destructive tint (wave-5 F7): the row's glyph+label ink rides
    /// the `Error` token (contrast-audited per theme). While the row
    /// is highlighted with the list focused it wears the selection
    /// pair like every row — selection affordance outranks the tint
    /// for that instant (the pair is the audited combination; error
    /// ink on the selection ground is not).
    pub danger: bool,
}

impl ChoiceOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> ChoiceOption {
        ChoiceOption {
            id: id.into(),
            label: label.into(),
            detail: None,
            key: None,
            danger: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> ChoiceOption {
        self.detail = Some(detail.into());
        self
    }

    /// Declare the option's shortcut letter (see [`ChoiceOption::key`]).
    pub fn key(mut self, key: char) -> ChoiceOption {
        self.key = Some(key);
        self
    }

    /// Destructive tint (see [`ChoiceOption::danger`]).
    pub fn danger(mut self, danger: bool) -> ChoiceOption {
        self.danger = danger;
        self
    }
}

/// The question as plain DATA — approval questions arrive from
/// elsewhere (an agent loop, a server) and sequences carry several;
/// the [`ChoicePrompt`] builder is sugar over this struct
/// ([`ChoicePrompt::of`] accepts one directly).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceQuestion {
    /// The prompt text (wraps in the panel).
    pub prompt: String,
    pub options: Vec<ChoiceOption>,
    /// false = one answer (Enter commits the candidate); true = a set
    /// (Space toggles, Enter/Confirm commits the whole set).
    pub allow_multiple: bool,
    /// `Some(label)` appends an "Other" row; engaging it reveals an
    /// inline free-text input whose value rides
    /// [`ChoiceAnswer::other`].
    pub other: Option<String>,
}

impl ChoiceQuestion {
    pub fn new(prompt: impl Into<String>) -> ChoiceQuestion {
        ChoiceQuestion {
            prompt: prompt.into(),
            options: Vec::new(),
            allow_multiple: false,
            other: None,
        }
    }
}

/// What the user chose. `selected` holds option IDS canonicalized to
/// option order (the MultiSelect precedent); `other` is the trimmed
/// free text when the Other choice was engaged — never `Some("")`.
/// Single mode with Other chosen yields `selected: []` + `other:
/// Some(text)`. Multiple mode may legally commit an EMPTY answer
/// (nothing checked, no Other): the component is a gate, not a
/// validator — callers wanting an explicit "none" add the option.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChoiceAnswer {
    pub selected: Vec<String>,
    pub other: Option<String>,
}

/// How the gate ended. `Cancelled` is an EXPLICIT outcome (Escape, the
/// Cancel button, `handle.cancel()`) — the blocked flow always hears
/// back; nothing closes silently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceOutcome {
    Answered(ChoiceAnswer),
    Cancelled,
}

/// Boxed one-shot resolution callback.
type ResolveFn = Box<dyn FnOnce(ChoiceOutcome)>;

/// Builder for one decision gate. Terminal verb: [`ChoicePrompt::open`]
/// — the prompt opens over everything immediately (Modal semantics:
/// focus-trapped, input-owning, centered).
pub struct ChoicePrompt {
    question: ChoiceQuestion,
    initial: Option<String>,
    checked: Vec<String>,
    max_visible: i32,
    dismissable: bool,
    overlays: Option<Overlays>,
    on_resolve: Option<ResolveFn>,
}

impl ChoicePrompt {
    pub fn new(prompt: impl Into<String>) -> ChoicePrompt {
        ChoicePrompt::of(ChoiceQuestion::new(prompt))
    }

    /// Build from an existing question (data-driven callers).
    pub fn of(question: ChoiceQuestion) -> ChoicePrompt {
        ChoicePrompt {
            question,
            initial: None,
            checked: Vec::new(),
            max_visible: 10,
            dismissable: true,
            overlays: None,
            on_resolve: None,
        }
    }

    /// Append an option (`id` = stable identity in the answer).
    pub fn option(mut self, id: impl Into<String>, label: impl Into<String>) -> ChoicePrompt {
        self.question.options.push(ChoiceOption::new(id, label));
        self
    }

    /// Append an option with a muted detail row under its label.
    pub fn option_detail(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        detail: impl Into<String>,
    ) -> ChoicePrompt {
        self.question
            .options
            .push(ChoiceOption::new(id, label).detail(detail));
        self
    }

    /// Append an option with a shortcut letter (wave-5 F2 — see
    /// [`ChoiceOption::key`]): the letter renders dim in the row, is
    /// named in the hint, and pressing it commits (single mode) or
    /// jump-toggles (multiple mode).
    pub fn option_key(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        key: char,
    ) -> ChoicePrompt {
        self.question
            .options
            .push(ChoiceOption::new(id, label).key(key));
        self
    }

    /// Append a fully built option — the escape hatch for combinations
    /// the sugar methods don't cover (detail + key + danger on one
    /// option).
    pub fn option_with(mut self, option: ChoiceOption) -> ChoicePrompt {
        self.question.options.push(option);
        self
    }

    /// Mark an already-added option as destructive (wave-5 F7 — see
    /// [`ChoiceOption::danger`]). Unknown ids are a caller bug: loud in
    /// debug, ignored in release.
    pub fn danger(mut self, id: &str) -> ChoicePrompt {
        match self.question.options.iter_mut().find(|o| o.id == id) {
            Some(o) => o.danger = true,
            None => debug_assert!(false, "ChoicePrompt::danger: no option with id {id:?}"),
        }
        self
    }

    /// Must-choose mode (wave-5 F3, charter G3): `dismissable(false)`
    /// removes the Cancel button and makes Esc REFUSE — visibly (a
    /// note in the hint row), never silently — instead of cancelling.
    /// For decisions that must be taken (destructive gates, wizard
    /// validation steps). Programmatic [`ChoicePromptHandle::cancel`]
    /// still resolves `Cancelled` (a timeout/deadline consumer keeps
    /// its lever), as do the degenerate-open paths — dismissability
    /// governs the USER's endings, never the flow's guarantee of an
    /// outcome. Default: dismissable.
    pub fn dismissable(mut self, dismissable: bool) -> ChoicePrompt {
        self.dismissable = dismissable;
        self
    }

    /// Multiple answers: Space/click toggles, Enter or the Confirm
    /// button commits the whole set. Default OFF (one answer; Enter
    /// commits the candidate).
    pub fn allow_multiple(mut self, on: bool) -> ChoicePrompt {
        self.question.allow_multiple = on;
        self
    }

    /// Append the "Other" free-text row (its display label). Engaging
    /// it reveals an inline input; the trimmed text rides
    /// [`ChoiceAnswer::other`]. Committing with Other engaged but no
    /// text is REFUSED (a visible note; the gate keeps waiting).
    pub fn allow_other(mut self, label: impl Into<String>) -> ChoicePrompt {
        self.question.other = Some(label.into());
        self
    }

    /// Seed the candidate/highlight on the option with this id
    /// (default: the first option). Unknown ids fall back to the first
    /// option.
    pub fn initial(mut self, id: impl Into<String>) -> ChoicePrompt {
        self.initial = Some(id.into());
        self
    }

    /// Multiple mode: option ids checked at open (pre-selecting the
    /// current state). Unknown ids are ignored; single mode ignores
    /// the whole seed.
    pub fn checked<I, S>(mut self, ids: I) -> ChoicePrompt
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.checked = ids.into_iter().map(Into::into).collect();
        self
    }

    /// Row budget for the option region (default 10; an option with a
    /// detail line costs two rows). Longer lists window around the
    /// highlight — the hint row shows the `i/N` position.
    pub fn max_visible(mut self, rows: i32) -> ChoicePrompt {
        self.max_visible = rows.max(1);
        self
    }

    /// Explicit overlay store (tests, exotic embeddings). Default: the
    /// app-provided reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> ChoicePrompt {
        self.overlays = Some(overlays.clone());
        self
    }

    /// The gate's continuation: fires EXACTLY ONCE with the outcome.
    /// The modal is already closed when it runs, so it may dispose
    /// anything (including the scope that opened the prompt) or open
    /// the next prompt.
    pub fn on_resolve(mut self, f: impl FnOnce(ChoiceOutcome) + 'static) -> ChoicePrompt {
        self.on_resolve = Some(Box::new(f));
        self
    }

    /// Open the gate over everything. Returns a cloneable handle
    /// (`cancel()` resolves `Cancelled` through the same exactly-once
    /// path; `is_open()` reads whether the prompt is still unresolved).
    pub fn open(self, cx: Scope) -> ChoicePromptHandle {
        let overlays = self
            .overlays
            .clone()
            .or_else(|| cx.use_context::<Overlays>());
        let resolved = Rc::new(Cell::new(false));
        let cb_slot: Rc<RefCell<Option<ResolveFn>>> = Rc::new(RefCell::new(self.on_resolve));
        let modal_slot: Rc<RefCell<Option<Modal>>> = Rc::new(RefCell::new(None));
        let resolve: Rc<dyn Fn(ChoiceOutcome)> = {
            let resolved = resolved.clone();
            let cb_slot = cb_slot.clone();
            let modal_slot = modal_slot.clone();
            Rc::new(move |outcome: ChoiceOutcome| {
                // Exactly-once gate first; then ALL bookkeeping — the
                // modal close (layer removal + state disposal) — lands
                // BEFORE the user callback (the 0297 disposal-safety
                // law): `on_resolve` may dispose everything or open
                // the next prompt. Both borrows end before the call.
                if resolved.replace(true) {
                    return;
                }
                let modal = modal_slot.borrow_mut().take();
                if let Some(modal) = modal {
                    modal.close();
                }
                let cb = cb_slot.borrow_mut().take();
                if let Some(cb) = cb {
                    cb(outcome);
                }
            })
        };
        let handle = ChoicePromptHandle {
            resolved: resolved.clone(),
            resolve: resolve.clone(),
        };

        let Some(overlays) = overlays else {
            debug_assert!(
                false,
                "ChoicePrompt: no Overlays available — open inside an App (context) \
                 or pass .overlays(..) explicitly"
            );
            // A gate that cannot open must not hang the flow it gates.
            resolve(ChoiceOutcome::Cancelled);
            return handle;
        };
        if self.question.options.is_empty() && self.question.other.is_none() {
            debug_assert!(
                false,
                "ChoicePrompt: a question with no options and no Other choice \
                 cannot be answered"
            );
            resolve(ChoiceOutcome::Cancelled);
            return handle;
        }

        let viewport = use_viewport(cx).get_untracked();
        let tokens = crate::widgets::theme_tokens(cx);
        let question = Rc::new(self.question);
        let geo = parts::measure(&question, viewport, self.max_visible, self.dismissable);
        let initial_row = self
            .initial
            .as_deref()
            .and_then(|id| question.options.iter().position(|o| o.id == id))
            .unwrap_or(0);
        let initial_checked: Vec<bool> = if question.allow_multiple {
            question
                .options
                .iter()
                .map(|o| self.checked.contains(&o.id))
                .collect()
        } else {
            vec![false; question.options.len()]
        };
        let panel = geo.panel;
        let spec_resolve = resolve.clone();
        let dismissable = self.dismissable;
        let modal = Modal::open(&overlays, cx, viewport, panel, move |mcx| {
            view::gate_content(
                mcx,
                tokens,
                view::GateSpec {
                    question,
                    geo,
                    initial_row,
                    initial_checked,
                    dismissable,
                    resolve: spec_resolve,
                },
            )
        });
        *modal_slot.borrow_mut() = Some(modal);
        handle
    }
}

/// Cloneable handle to an open (or already resolved) prompt.
#[derive(Clone)]
pub struct ChoicePromptHandle {
    resolved: Rc<Cell<bool>>,
    resolve: Rc<dyn Fn(ChoiceOutcome)>,
}

impl ChoicePromptHandle {
    /// Programmatic cancel: resolves `Cancelled` through the same
    /// exactly-once path as Escape — the gated flow always hears back;
    /// a gate never closes silently. No-op once resolved.
    pub fn cancel(&self) {
        (self.resolve)(ChoiceOutcome::Cancelled);
    }

    /// True until the prompt resolves (either way).
    pub fn is_open(&self) -> bool {
        !self.resolved.get()
    }
}

/// How a [`ChoiceSequence`] ended: every question answered, or
/// cancelled at `index` with the `answers` gathered so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceSequenceOutcome {
    Completed(Vec<ChoiceAnswer>),
    Cancelled {
        index: usize,
        answers: Vec<ChoiceAnswer>,
    },
}

/// The small sequential-flow helper: several questions, one gate after
/// another (setup wizard, multi-step approval). Each question opens as
/// the previous resolves — safe by the gate contract (the modal is
/// closed before `on_resolve` runs). Cancelling any question ends the
/// whole sequence with `Cancelled { index, answers }`.
///
/// An EMPTY question list resolves `Completed(vec![])` synchronously
/// from `open` (documented — there is nothing to wait for).
pub struct ChoiceSequence {
    questions: Vec<ChoiceQuestion>,
    overlays: Option<Overlays>,
    on_resolve: Option<Box<dyn FnOnce(ChoiceSequenceOutcome)>>,
}

impl ChoiceSequence {
    pub fn new(questions: Vec<ChoiceQuestion>) -> ChoiceSequence {
        ChoiceSequence {
            questions,
            overlays: None,
            on_resolve: None,
        }
    }

    /// Explicit overlay store for every prompt in the sequence
    /// (tests, exotic embeddings). Default: reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> ChoiceSequence {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires exactly once with the sequence outcome.
    pub fn on_resolve(mut self, f: impl FnOnce(ChoiceSequenceOutcome) + 'static) -> ChoiceSequence {
        self.on_resolve = Some(Box::new(f));
        self
    }

    /// Open the first gate (or resolve immediately when empty).
    pub fn open(self, cx: Scope) {
        let done = self
            .on_resolve
            .unwrap_or_else(|| Box::new(|_| {}) as Box<dyn FnOnce(ChoiceSequenceOutcome)>);
        if self.questions.is_empty() {
            done(ChoiceSequenceOutcome::Completed(Vec::new()));
            return;
        }
        advance(
            cx,
            self.overlays,
            Rc::new(self.questions),
            0,
            Vec::new(),
            done,
        );
    }
}

/// Open question `index`; recurse from inside `on_resolve` (the modal
/// is already closed there) until answered-through or cancelled.
fn advance(
    cx: Scope,
    overlays: Option<Overlays>,
    questions: Rc<Vec<ChoiceQuestion>>,
    index: usize,
    answers: Vec<ChoiceAnswer>,
    done: Box<dyn FnOnce(ChoiceSequenceOutcome)>,
) {
    let mut prompt = ChoicePrompt::of(questions[index].clone());
    if let Some(o) = &overlays {
        prompt = prompt.overlays(o);
    }
    prompt
        .on_resolve(move |outcome| match outcome {
            ChoiceOutcome::Answered(answer) => {
                let mut answers = answers;
                answers.push(answer);
                if index + 1 == questions.len() {
                    done(ChoiceSequenceOutcome::Completed(answers));
                } else {
                    advance(cx, overlays, questions, index + 1, answers, done);
                }
            }
            ChoiceOutcome::Cancelled => done(ChoiceSequenceOutcome::Cancelled { index, answers }),
        })
        .open(cx);
}

#[cfg(test)]
#[path = "choice_prompt_tests.rs"]
mod tests;

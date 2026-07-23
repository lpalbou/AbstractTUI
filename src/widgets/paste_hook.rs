//! Paste-intercept plumbing shared by the two editors (backlog
//! first-app/0273): [`PasteAction`] + the shared hook slot both
//! `TextInput` and `TextArea` consult BEFORE inserting a paste.
//!
//! WHY A HOOK AND NOT AN EVENT: `UiEvent::Paste` already routes to the
//! focused widget whole (never per-char synthesis — the injection
//! attack bracketed paste exists to prevent). What apps need is a
//! DECISION POINT between "the paste arrived" and "the editor inserted
//! it": a terminal file DROP arrives as a paste of the file's path
//! (see `input::paste` for the cross-terminal spellings), and an
//! attachment-taking composer wants to turn that paste into a chip
//! instead of text. The hook sees the RAW paste text and answers
//! [`PasteAction`]; everything else — classification, fs checks,
//! chips — is app code on top.
//!
//! ORDERING vs the disposal-safety law (backlog 0297): the law says
//! user callbacks run LAST so they may dispose the widget's scope.
//! This hook is a different kind of callback — an INTERCEPTOR whose
//! whole point is to run FIRST (it decides whether widget writes
//! happen at all). The law's guarantee is preserved from both arms:
//! on [`PasteAction::Consume`] the widget performs NO signal writes
//! after the hook; on [`PasteAction::Insert`] the widget re-checks
//! its value signal's liveness and treats a hook that disposed the
//! scope as consumed (nothing left to insert into) instead of
//! panicking on dead signals.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

/// The app's decision for one intercepted paste (see
/// [`TextInput::on_paste`](super::TextInput::on_paste) /
/// [`TextArea::on_paste`](crate::widgets::TextArea::on_paste)).
///
/// `#[non_exhaustive]` per ADR-0003 §3 — an enum the engine may grow
/// (a replace-text variant is a plausible future). Constructing the
/// existing variants is stable downstream; foreign `match`es carry a
/// `_` arm (treat unknown future actions as [`PasteAction::Insert`]
/// if you must map them — never silently drop user text).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PasteAction {
    /// Proceed with today's insertion, byte-identical to an unhooked
    /// widget (TextInput folds line breaks to spaces; TextArea keeps
    /// normalized newlines).
    Insert,
    /// The widget inserts NOTHING and the event is consumed — the app
    /// already acted on the text (attachment chip, path routing).
    /// No `on_change` fires; caret, selection and history are
    /// untouched.
    Consume,
}

/// Boxed intercept callback (`on_paste` builder slots).
pub(crate) type PasteHookFn = Box<dyn FnMut(&str) -> PasteAction>;

/// The shared slot both editors consult from their event handlers.
/// Same shape as `widgets::SharedCallback`; same held-borrow contract
/// (dispatch-only slot — see the `SharedCallback` docs).
pub(crate) type PasteHook = Rc<RefCell<Option<PasteHookFn>>>;

/// Run the intercept hook against the raw paste text. `None` = no hook
/// bound (the caller proceeds with today's insertion, byte-identical);
/// `Some(action)` = the hook ran and the caller must honor the action
/// AND re-check its signals' liveness before writing (the hook may
/// have disposed the widget's scope — module docs).
pub(crate) fn run_paste_hook(hook: &PasteHook, text: &str) -> Option<PasteAction> {
    // Held borrow across user code: safe under the SharedCallback
    // held-borrow contract (fired from event dispatch only).
    hook.borrow_mut().as_mut().map(|f| f(text))
}

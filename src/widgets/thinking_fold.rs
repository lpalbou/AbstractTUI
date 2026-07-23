//! [`ThinkingFold`] — the reasoning-text fold (backlog app-kits/1250):
//! a [`Disclosure`]-based card for a model's thinking, FOLDED by
//! default (operator ruling: reasoning is detail, the answer is the
//! content), with streaming semantics and a data-driven activity
//! indicator.
//!
//! ```ignore
//! let thinking = ThinkingFoldState::new(cx);
//! let card = ThinkingFold::new(&thinking).view(cx);
//! // as reasoning DELTAS arrive from result metadata:
//! thinking.append(fragment);
//! // at completion, streams may deliver a trailing COMPLETE aggregate:
//! thinking.complete(aggregate); // LAST WINS — replaces the fragments
//! ```
//!
//! ## Input is METADATA, never prose
//!
//! Reasoning text arrives from result metadata (the provider's
//! `reasoning`/thinking fields, delivered by the app's transport) —
//! NEVER parsed out of reply prose. The widget renders what it is
//! handed; deciding what counts as reasoning is the app's (and its
//! gateway's) job.
//!
//! ## Streaming semantics (pinned by tests)
//!
//! - [`ThinkingFoldState::append`] folds one fragment into the body —
//!   only the OPEN markdown region re-typesets (the Feed stream
//!   contract), so a long thought costs the same per fragment at its
//!   end as at its start.
//! - [`ThinkingFoldState::complete`] REPLACES the accumulated
//!   fragments with the aggregate (last-wins: providers may stream
//!   fragments and then deliver one complete text — the aggregate is
//!   the truth). A second `complete` replaces again (still last-wins).
//! - Fragments arriving AFTER `complete` are IGNORED (`append` returns
//!   `false`): the aggregate already superseded the fragment stream —
//!   folding a late fragment in would corrupt the completed text.
//!
//! ## Zero idle
//!
//! The folded header shows a dot-spinner frame while streaming — the
//! frame advances PER APPEND (data-driven), never on a timer: between
//! fragments the card schedules nothing, and a parked (quiet or
//! completed) fold is fully idle. The indicator disappears at
//! `complete`.
//!
//! ## Placement honesty
//!
//! The card composes in plain panels, transcript stacks and Scroll
//! columns. It CANNOT live inside a [`Feed`] item: feed blocks are
//! draw-only (widgets cannot ride them — backlog first-app/0280), so a
//! transcript places the fold BESIDE its feed segments, as its own
//! view in the turn column (`examples/reasoning.rs` shows the shape).
//!
//! OWNER: REASON (app-kits/1250, composing Disclosure + Feed).

use std::cell::Cell;
use std::rc::Rc;

use crate::layout::Style as LayoutStyle;
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::View;

use super::disclosure::Disclosure;
use super::feed::{Feed, FeedItem, FeedState};
use super::spinner::SpinnerKind;

/// The one body item's key inside the state's private feed.
const BODY_KEY: &str = "thinking";

/// Cloneable handle to one thinking stream: the app mutates it
/// (`append`/`complete`/`set_detail`), every mounted [`ThinkingFold`]
/// bound to it re-renders fine-grained. Create one per turn — a new
/// thought is a new state (there is deliberately no `reset`: keyed
/// per-turn state keeps last-wins unambiguous).
#[derive(Clone)]
pub struct ThinkingFoldState {
    feed: FeedState,
    /// Appends since creation — the indicator's frame counter
    /// (data-driven animation: no clock anywhere).
    appends: Rc<Cell<u64>>,
    completed: Rc<Cell<bool>>,
    streaming: Signal<bool>,
    /// App-provided trailing detail (token counts: "213 tk").
    detail: Signal<String>,
    /// The composed header slot the card renders (spinner frame +
    /// detail) — the ONE signal the title row subscribes to.
    header: Signal<String>,
}

impl ThinkingFoldState {
    pub fn new(cx: Scope) -> ThinkingFoldState {
        let feed = FeedState::new(cx);
        // The body exists from birth as a STREAMING markdown item, so
        // the first fragment needs no special case.
        feed.push_stream(BODY_KEY);
        ThinkingFoldState {
            feed,
            appends: Rc::new(Cell::new(0)),
            completed: Rc::new(Cell::new(false)),
            streaming: cx.signal(false),
            detail: cx.signal(String::new()),
            header: cx.signal(String::new()),
        }
    }

    /// Fold one reasoning fragment in (result-metadata deltas). Only
    /// the open markdown tail re-typesets; the indicator frame
    /// advances. Returns `false` — and changes NOTHING — once
    /// [`ThinkingFoldState::complete`] has run (late fragments are
    /// stale by the last-wins contract).
    pub fn append(&self, fragment: &str) -> bool {
        if self.completed.get() {
            return false;
        }
        self.appends.set(self.appends.get() + 1);
        if !self.streaming.get_untracked() {
            self.streaming.set(true);
        }
        self.feed.stream_append(BODY_KEY, fragment);
        self.recompose_header();
        true
    }

    /// Replace the accumulated fragments with the COMPLETE aggregate
    /// (last-wins) and end streaming. Idempotent in shape: a second
    /// aggregate replaces again — the latest complete text wins.
    pub fn complete(&self, aggregate: &str) {
        self.completed.set(true);
        self.feed.update(BODY_KEY, FeedItem::markdown(aggregate));
        if self.streaming.get_untracked() {
            self.streaming.set(false);
        }
        self.recompose_header();
    }

    /// Set the trailing header detail (the token-count slot). Empty
    /// clears it.
    pub fn set_detail(&self, detail: impl Into<String>) {
        self.detail.set(detail.into());
        self.recompose_header();
    }

    /// Fragments are arriving and no aggregate has landed yet
    /// (reactive read — status chrome can subscribe).
    pub fn is_streaming(&self) -> bool {
        self.streaming.get()
    }

    /// [`ThinkingFoldState::complete`] has run.
    pub fn is_completed(&self) -> bool {
        self.completed.get()
    }

    /// Compose the header slot: `⠐ 213 tk` while streaming (frame
    /// advances per append), just `213 tk` after, empty when idle with
    /// no detail. Write-if-different keeps quiet turns damage-free.
    fn recompose_header(&self) {
        let detail = self.detail.get_untracked();
        let composed = if self.streaming.get_untracked() {
            let frames = SpinnerKind::Dots.frames();
            let frame = frames[(self.appends.get() % frames.len() as u64) as usize];
            if detail.is_empty() {
                frame.to_string()
            } else {
                format!("{frame} {detail}")
            }
        } else {
            detail
        };
        if self.header.get_untracked() != composed {
            self.header.set(composed);
        }
    }
}

/// The card: `ThinkingFold::new(&state).view(cx)` wherever the turn
/// renders. A muted "Thinking" [`Disclosure`] header (folded by
/// default) over the streaming markdown body — tables and fences tint
/// through the same typeset recipe as `MarkdownView` (one recipe, no
/// drift), and bodies taller than the cap scroll inside it.
pub struct ThinkingFold {
    state: ThinkingFoldState,
    title: String,
    max_body_rows: i32,
    folded: Option<Signal<bool>>,
    layout: Option<LayoutStyle>,
}

impl ThinkingFold {
    pub fn new(state: &ThinkingFoldState) -> ThinkingFold {
        ThinkingFold {
            state: state.clone(),
            title: String::from("Thinking"),
            max_body_rows: 8,
            folded: None,
            layout: None,
        }
    }

    /// Override the header word (localization; "Reasoning").
    pub fn title(mut self, title: impl Into<String>) -> ThinkingFold {
        self.title = title.into();
        self
    }

    /// Cap the unfolded body at `rows` (default 8); taller reasoning
    /// scrolls inside the cap. `rows <= 0` removes the cap (the
    /// Disclosure contract).
    pub fn max_body_rows(mut self, rows: i32) -> ThinkingFold {
        self.max_body_rows = rows;
        self
    }

    /// Controlled fold state (the 0850 policy hook — "newest expanded,
    /// rest folded" lives app-side). Default: an internal signal
    /// starting FOLDED (the operator ruling), surviving fold cycles
    /// and streaming alike.
    pub fn folded(mut self, folded: Signal<bool>) -> ThinkingFold {
        self.folded = Some(folded);
        self
    }

    /// Layout for the card root (a column: header + body region).
    pub fn layout(mut self, layout: LayoutStyle) -> ThinkingFold {
        self.layout = Some(layout);
        self
    }

    /// Canonical one-call build: tokens resolve from the app's theme
    /// context; state lives on `cx`.
    pub fn view(self, cx: Scope) -> View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> crate::ui::Element {
        let tokens = *t;
        let folded = self.folded.unwrap_or_else(|| cx.signal(true));
        let feed = self.state.feed.clone();
        let mut card = Disclosure::new(self.title)
            .title_muted(true)
            .detail_signal(self.state.header)
            .folded(folded)
            .max_body_rows(self.max_body_rows)
            // The body is the state's one-item feed, rebuilt per
            // expansion over durable typeset state (the
            // Disclosure::markdown recipe — a re-expand costs no
            // re-typeset, and streaming appends while unfolded update
            // the feed's own dyn region without touching the header's
            // focus).
            .body(move |gcx: Scope| Feed::new(&feed).gap(0).element(gcx, &tokens).build());
        if let Some(layout) = self.layout {
            card = card.layout(layout);
        }
        card.element(cx, t)
    }
}

#[cfg(test)]
#[path = "thinking_fold_tests.rs"]
mod tests;

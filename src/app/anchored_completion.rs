//! The completion controller (backlog 0120 §7) — the passive panel's
//! first consumer, designed jointly with 0500. Private sibling of
//! anchored.rs (file-size split, the feed_typeset.rs pattern); the
//! public path stays `app::anchored::{Completion, CompletionCandidate}`
//! through the parent's re-export.
//!
//! One live token at a time: an effect watches the composer's
//! value/caret/focus/caret-cell signals, scans the token behind the
//! caret, asks the matching provider, and keeps a passive
//! [`AnchoredPanel`] placed at the caret cell. A capture-phase wrapper
//! element intercepts Down/Up/Enter/Tab/Escape while the dropdown is
//! open; everything else falls through to the composer untouched.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::ui::{dyn_view, Element, EventCtx, Key, Mods, Phase, Role, UiEvent, View};
use crate::widgets::TextAreaState;

use super::super::overlays::Overlays;
use super::super::viewport::use_viewport;
use super::{AnchoredPanel, PanelAnchor, PanelWidth};

/// One completion row. `label` renders; `detail` renders muted after
/// it; `insert` replaces the whole token (trigger char included) on
/// accept — include a trailing space there if you want one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub label: String,
    pub detail: Option<String>,
    pub insert: String,
}

impl CompletionCandidate {
    pub fn new(label: impl Into<String>, insert: impl Into<String>) -> CompletionCandidate {
        CompletionCandidate {
            label: label.into(),
            detail: None,
            insert: insert.into(),
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> CompletionCandidate {
        self.detail = Some(detail.into());
        self
    }
}

type Provider = Rc<dyn Fn(&str) -> Vec<CompletionCandidate>>;

/// Session state behind the controller (one live token at a time).
struct Session {
    open: Option<OpenSession>,
    /// Escape'd token start: no reopen while the caret stays inside
    /// this token (calm dismissal; leaving the token re-arms).
    muted_at: Option<usize>,
    /// One-shot: the accept edit's own effect run must not reopen.
    skip_next: bool,
}

struct OpenSession {
    panel: AnchoredPanel,
    scope: Scope,
    token_start: usize,
    trigger_idx: usize,
    last_query: String,
}

fn close_open(s: &mut Session) {
    if let Some(open) = s.open.take() {
        open.panel.close();
        open.scope.dispose();
    }
}

/// Trigger-character completion over a [`TextAreaState`]-bound
/// composer. Register providers per trigger char, then `attach` — the
/// returned view wraps the composer with the key interception layer and
/// owns the dropdown lifecycle. Providers run synchronously (v1) with
/// the query typed after the trigger; returning an empty Vec closes the
/// dropdown.
pub struct Completion {
    triggers: Vec<(char, Provider)>,
    max_visible: usize,
}

impl Default for Completion {
    fn default() -> Self {
        Completion::new()
    }
}

impl Completion {
    pub fn new() -> Completion {
        Completion {
            triggers: Vec::new(),
            max_visible: 6,
        }
    }

    /// Register a provider for tokens starting with `trigger` (at the
    /// start of the text or after whitespace). The query excludes the
    /// trigger char and never contains whitespace.
    pub fn trigger(
        mut self,
        trigger: char,
        provider: impl Fn(&str) -> Vec<CompletionCandidate> + 'static,
    ) -> Completion {
        self.triggers.push((trigger, Rc::new(provider)));
        self
    }

    /// Dropdown rows shown at once (default 6); longer candidate lists
    /// window around the highlight.
    pub fn max_visible(mut self, n: usize) -> Completion {
        self.max_visible = n.max(1);
        self
    }

    /// Wire the controller around a built composer view. Wrap ONLY the
    /// composer: the capture-phase handler assumes keys routed through
    /// this subtree belong to it while the dropdown is open.
    pub fn attach(
        self,
        cx: Scope,
        overlays: &Overlays,
        state: &TextAreaState,
        composer: View,
    ) -> View {
        let t = crate::widgets::theme_tokens(cx);
        let triggers = Rc::new(self.triggers);
        let max_visible = self.max_visible;
        let overlays = overlays.clone();
        let state = state.clone();
        let viewport = use_viewport(cx);
        let session: Rc<RefCell<Session>> = Rc::new(RefCell::new(Session {
            open: None,
            muted_at: None,
            skip_next: false,
        }));
        // Reused across sessions: zero per-session signal accumulation.
        let candidates: Signal<Vec<CompletionCandidate>> = cx.signal(Vec::new());
        let highlight: Signal<usize> = cx.signal(0usize);

        // Accept: replace the token (trigger included) with the pick.
        let accept: Rc<dyn Fn(usize)> = Rc::new({
            let session = session.clone();
            let state = state.clone();
            move |idx: usize| {
                let picked = {
                    let s = session.borrow();
                    let Some(open) = &s.open else { return };
                    candidates
                        .get_untracked()
                        .get(idx)
                        .map(|c| (open.token_start, c.insert.clone()))
                };
                let Some((start, insert)) = picked else {
                    return;
                };
                {
                    let mut s = session.borrow_mut();
                    s.skip_next = true;
                    close_open(&mut s);
                }
                let caret = state.caret_byte();
                state.replace_range(start..caret, &insert);
            }
        });

        // The panel tree: raised ground + a reactive row window. Rows
        // are NOT focusable (passive-panel contract); clicking one
        // accepts it (presses inside the panel are the panel's own).
        let build: Rc<dyn Fn(Scope) -> View> = Rc::new({
            let accept = accept.clone();
            move |_pcx: Scope| {
                let accept = accept.clone();
                let ground = t.surface_raised;
                let ink = t.text;
                let muted = t.text_muted;
                let sel_bg = t.selection_bg;
                let sel_fg = t.selection_fg;
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .role(Role::Menu)
                    .access_label("completion")
                    .access_value(move || {
                        let cands = candidates.get_untracked();
                        let h = highlight.get_untracked().min(cands.len().saturating_sub(1));
                        cands.get(h).map(|c| c.label.clone()).unwrap_or_default()
                    })
                    .draw(move |canvas, rect| {
                        canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
                    })
                    .child(dyn_view(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                        move || {
                            let cands = candidates.get();
                            let n = cands.len();
                            let h = highlight.get().min(n.saturating_sub(1));
                            let vis = max_visible.min(n.max(1));
                            let start =
                                (if h < vis { 0 } else { h + 1 - vis }).min(n.saturating_sub(vis));
                            let mut col = Element::new()
                                .style(LayoutStyle::column().width(Dimension::Percent(1.0)));
                            for (i, cand) in cands.iter().enumerate().skip(start).take(vis) {
                                let selected = i == h;
                                let label = cand.label.clone();
                                let detail = cand.detail.clone();
                                let accept = accept.clone();
                                col = col.child(
                                    Element::new()
                                        .style(LayoutStyle::line(1).shrink(0.0))
                                        .role(Role::MenuItem)
                                        .access_label(label.clone())
                                        .on(Phase::Bubble, move |ctx, ev| {
                                            if let UiEvent::Mouse(m) = ev {
                                                if matches!(m.kind, crate::ui::MouseKind::Down(_)) {
                                                    accept(i);
                                                    ctx.stop_propagation();
                                                }
                                            }
                                        })
                                        .draw(move |canvas, rect| {
                                            let (fg, bg) = if selected {
                                                (sel_fg, sel_bg)
                                            } else {
                                                (ink, ground)
                                            };
                                            let style = Style::new().fg(fg).bg(bg);
                                            canvas.fill_styled(rect, ' ', &style);
                                            canvas.print_styled(
                                                Point::new(rect.x + 1, rect.y),
                                                &label,
                                                &style,
                                            );
                                            if let Some(d) = &detail {
                                                let lx =
                                                    rect.x + 1 + crate::text::width(&label) + 2;
                                                let dstyle = if selected {
                                                    Style::new().fg(sel_fg).bg(bg)
                                                } else {
                                                    Style::new().fg(muted).bg(bg)
                                                };
                                                canvas.print_styled(
                                                    Point::new(lx, rect.y),
                                                    d,
                                                    &dstyle,
                                                );
                                            }
                                        })
                                        .build(),
                                );
                            }
                            col.build()
                        },
                    ))
                    .build()
            }
        });

        // The controller effect: token scan + provider + panel geometry.
        {
            let session = session.clone();
            let state = state.clone();
            let triggers = triggers.clone();
            let overlays = overlays.clone();
            let build = build.clone();
            cx.effect_labeled("completion-controller", move || {
                let text = state.value().get();
                let caret = state.caret_byte();
                let focused = state.focused().get();
                let cell = state.caret_cell().get();
                let vp = viewport.get();
                if session.borrow().skip_next {
                    let mut s = session.borrow_mut();
                    s.skip_next = false;
                    close_open(&mut s);
                    return;
                }
                if !focused || cell.is_none() {
                    let mut s = session.borrow_mut();
                    s.muted_at = None;
                    close_open(&mut s);
                    return;
                }
                let token = find_token(&text, caret, &triggers);
                let Some((start, trigger_idx, query)) = token else {
                    let mut s = session.borrow_mut();
                    s.muted_at = None;
                    close_open(&mut s);
                    return;
                };
                if session.borrow().muted_at == Some(start) {
                    let mut s = session.borrow_mut();
                    close_open(&mut s);
                    return;
                }
                session.borrow_mut().muted_at = None;
                // Provider runs OUTSIDE any session borrow (user code).
                let cands = (triggers[trigger_idx].1)(&query);
                if cands.is_empty() {
                    let mut s = session.borrow_mut();
                    close_open(&mut s);
                    return;
                }
                let content = measure_candidates(&cands, max_visible);
                let anchor = PanelAnchor::cell(cell.expect("checked above"));
                let mut s = session.borrow_mut();
                let same_token = matches!(
                    &s.open,
                    Some(o) if o.token_start == start && o.trigger_idx == trigger_idx
                );
                if !same_token || s.open.as_ref().is_some_and(|o| o.last_query != query) {
                    highlight.set(0);
                } else {
                    let top = cands.len() - 1;
                    highlight.update(|h| *h = (*h).min(top));
                }
                candidates.set(cands);
                if same_token {
                    let open = s.open.as_mut().expect("same_token implies open");
                    open.last_query = query;
                    open.panel.update(vp, anchor, content);
                } else {
                    close_open(&mut s);
                    let scope = cx.child();
                    let build = build.clone();
                    let panel = AnchoredPanel::open_passive(
                        &overlays,
                        scope,
                        vp,
                        anchor,
                        PanelWidth::Content { min: 8, max: 44 },
                        content,
                        move |pcx| (build)(pcx),
                    );
                    s.open = Some(OpenSession {
                        panel,
                        scope,
                        token_start: start,
                        trigger_idx,
                        last_query: query,
                    });
                }
            });
        }

        // Whatever session is live dies with the composer's scope.
        {
            let session = session.clone();
            cx.on_cleanup(move || {
                let mut s = session.borrow_mut();
                close_open(&mut s);
            });
        }

        // Capture-phase interception: runs BEFORE the composer's own
        // bubble handler, only while the dropdown is open, only on
        // modless keys — everything else falls through untouched
        // (Alt+Enter still inserts a newline mid-completion).
        let handler = {
            let session = session.clone();
            let accept = accept.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                let UiEvent::Key(k) = ev else { return };
                if k.mods != Mods::NONE {
                    return;
                }
                let token_start = {
                    let s = session.borrow();
                    s.open.as_ref().map(|o| o.token_start)
                };
                let Some(token_start) = token_start else {
                    return;
                };
                match k.key {
                    Key::Down => {
                        let top = candidates.get_untracked().len().saturating_sub(1);
                        highlight.update(|h| *h = (*h + 1).min(top));
                    }
                    Key::Up => highlight.update(|h| *h = h.saturating_sub(1)),
                    Key::Enter | Key::Tab => accept(highlight.get_untracked()),
                    Key::Escape => {
                        let mut s = session.borrow_mut();
                        s.muted_at = Some(token_start);
                        close_open(&mut s);
                    }
                    _ => return,
                }
                ctx.stop_propagation();
            }
        };

        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .shrink(0.0),
            )
            .on(Phase::Capture, handler)
            .child(composer)
            .build()
    }
}

/// Natural panel size: rows to show + widest row (1-cell side padding,
/// 2-cell label/detail gap) — clamped later by the width policy.
fn measure_candidates(cands: &[CompletionCandidate], max_visible: usize) -> Size {
    let w = cands
        .iter()
        .map(|c| {
            1 + crate::text::width(&c.label)
                + c.detail
                    .as_ref()
                    .map(|d| 2 + crate::text::width(d))
                    .unwrap_or(0)
                + 1
        })
        .max()
        .unwrap_or(1);
    Size::new(w, cands.len().min(max_visible) as i32)
}

/// The token behind the caret: scan back to the nearest whitespace (or
/// the text start); the token completes when its FIRST cluster is a
/// registered trigger char. Returns (token start byte, trigger index,
/// query after the trigger).
fn find_token(
    text: &str,
    caret: usize,
    triggers: &[(char, Provider)],
) -> Option<(usize, usize, String)> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let mut start = 0usize;
    for seg in crate::text::segments(&text[..caret]) {
        if seg.cluster.chars().next().is_some_and(char::is_whitespace) {
            start = seg.offset + seg.cluster.len();
        }
    }
    let token = &text[start..caret];
    let first = token.chars().next()?;
    let idx = triggers.iter().position(|(c, _)| *c == first)?;
    Some((start, idx, token[first.len_utf8()..].to_string()))
}

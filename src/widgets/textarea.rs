//! TextArea: multiline composer (backlog 0120).
//!
//! Editing model: logical lines soft-wrapped at the widget width into a
//! byte-tiling `RowMap` (textarea_model.rs — every byte has exactly one
//! visual home); the caret is a byte offset that ALWAYS sits on a
//! grapheme cluster boundary (the same `text::segments` authority as
//! `TextInput`, RT3-2). Horizontal motion steps clusters, vertical
//! motion steps visual rows with a remembered goal column, Home/End are
//! per visual row (Ctrl+Home/End for the document), selection rides an
//! anchor byte, and `Paste` inserts whole — newlines included, never a
//! submit.
//!
//! Submit vs newline is builder policy (`SubmitPolicy`): the default is
//! Enter-submits + Alt+Enter-inserts (works on every wire), Ctrl+J
//! always inserts (0x0a IS Ctrl+J on the legacy wire — the universal
//! fallback, backlog 0295), and Shift+Enter also inserts where the
//! kitty protocol reports it — chords the classic wire cannot carry are
//! never the only path (docs/faq.md). History recall is edge-triggered
//! with the reference
//! console's semantics: arrows navigate the buffer first, reach for
//! history only at the start/end edges, and the in-progress draft
//! survives a round trip.
//!
//! The widget grows with its content between `min_rows` and `max_rows`,
//! then scrolls internally. [`TextAreaState`] is the app-facing wire
//! (FeedState pattern): the value signal, caret byte, focus, the
//! caret's SCREEN CELL (`caret_cell` — the completion-dropdown anchor,
//! backlog 0120 §6), programmatic edits and the history store.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::Point;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, Phase, UiEvent};
use crate::widgets::input::{notify, BoxedTextFn, TextCallback};

#[path = "textarea_model.rs"]
mod model;

pub use model::SubmitPolicy;
pub(crate) use model::{adjust_top, Caret, RowMap};
use model::{insert_at_caret, selection_range, snap_boundary, EditOutcome, History};

/// Default history capacity (entries beyond it drop oldest-first).
const HISTORY_CAP: usize = 128;

struct StateInner {
    value: Signal<String>,
    caret: Signal<Caret>,
    focused: Signal<bool>,
    caret_cell: Signal<Option<Point>>,
    history: RefCell<History>,
}

/// Durable, cloneable composer state — create once on a stable scope,
/// bind with [`TextArea::state`], mutate from anywhere (the FeedState
/// pattern). Everything the completion controller consumes lives here.
#[derive(Clone)]
pub struct TextAreaState {
    inner: Rc<StateInner>,
}

impl TextAreaState {
    pub fn new(cx: Scope) -> TextAreaState {
        TextAreaState {
            inner: Rc::new(StateInner {
                value: cx.signal(String::new()),
                caret: cx.signal(Caret::origin()),
                focused: cx.signal(false),
                caret_cell: cx.signal(None),
                history: RefCell::new(History::new(HISTORY_CAP)),
            }),
        }
    }

    /// The buffer signal (tracked reads re-render/re-run readers).
    pub fn value(&self) -> Signal<String> {
        self.inner.value
    }

    /// Current buffer text (untracked snapshot).
    pub fn text(&self) -> String {
        self.inner.value.get_untracked()
    }

    /// Replace the whole buffer; caret lands at the end.
    pub fn set_text(&self, text: impl Into<String>) {
        let text = text.into();
        let len = text.len();
        crate::reactive::batch(|| {
            self.inner.value.set(text);
            self.inner.caret.update(|c| {
                *c = Caret {
                    byte: len,
                    ..Caret::origin()
                };
            });
        });
    }

    pub fn clear(&self) {
        self.set_text(String::new());
    }

    /// Caret byte offset (a cluster boundary). Tracked: reading inside
    /// an effect subscribes it to caret movement.
    pub fn caret_byte(&self) -> usize {
        self.inner.caret.get().byte
    }

    /// Focus state of the bound widget (true while it owns keys).
    pub fn focused(&self) -> Signal<bool> {
        self.inner.focused
    }

    /// The caret's solved SCREEN CELL — the anchor a completion
    /// dropdown places itself against (backlog 0120 §6). Updated from
    /// inside the widget's event handlers (`EventCtx::current_rect`,
    /// the engine's only rect source today — 0500 records the general
    /// rect-query gap); `None` while unfocused. After a pure resize the
    /// cell can be stale until the next key event.
    pub fn caret_cell(&self) -> Signal<Option<Point>> {
        self.inner.caret_cell
    }

    /// Replace `range` (byte offsets, snapped to cluster boundaries)
    /// with `insert`; the caret lands after the insertion. The
    /// completion controller's accept path.
    pub fn replace_range(&self, range: std::ops::Range<usize>, insert: &str) {
        crate::reactive::batch(|| {
            let mut caret_byte = 0usize;
            self.inner.value.update(|text| {
                let lo = snap_boundary(text, range.start.min(text.len()));
                let hi = snap_boundary(text, range.end.clamp(lo, text.len()));
                text.replace_range(lo..hi, insert);
                caret_byte = snap_boundary(text, lo + insert.len());
            });
            self.inner.caret.update(|c| {
                *c = Caret {
                    byte: caret_byte,
                    top: c.top,
                    ..Caret::origin()
                };
            });
        });
    }

    /// Record a submitted entry for Up/Down recall (empty entries and
    /// consecutive duplicates are skipped). Pushing is the app's call —
    /// typically inside `on_submit`.
    pub fn push_history(&self, entry: &str) {
        self.inner.history.borrow_mut().push(entry);
    }

    /// Entries currently recallable.
    pub fn history_len(&self) -> usize {
        self.inner.history.borrow().len()
    }
}

/// The multiline composer widget. `TextInput`'s big sibling: same
/// cluster-atomic editing contract, plus soft wrap, vertical caret
/// movement, grow-to-content, history recall and the caret-cell anchor.
pub struct TextArea {
    state: Option<TextAreaState>,
    placeholder: String,
    placeholder_while_focused: bool,
    min_rows: i32,
    max_rows: i32,
    policy: SubmitPolicy,
    disabled: bool,
    layout: Option<LayoutStyle>,
    on_change: Option<BoxedTextFn>,
    on_submit: Option<BoxedTextFn>,
}

impl TextArea {
    pub fn new() -> TextArea {
        TextArea {
            state: None,
            placeholder: String::new(),
            placeholder_while_focused: false,
            min_rows: 1,
            max_rows: 6,
            policy: SubmitPolicy::default(),
            disabled: false,
            layout: None,
            on_change: None,
            on_submit: None,
        }
    }

    /// Bind durable state (value/caret/focus/history) owned elsewhere.
    pub fn state(mut self, state: &TextAreaState) -> TextArea {
        self.state = Some(state.clone());
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> TextArea {
        self.placeholder = text.into();
        self
    }

    /// Paint the placeholder while the field is focused-and-empty too,
    /// beside the caret (backlog first-app/0291). By default the
    /// placeholder yields to the caret — classic form UX — which means
    /// an `.autofocus()`ed composer NEVER renders its hint (it is
    /// focused from boot). Opting in follows the convention every
    /// modern toolkit ships (VS Code, browsers' `::placeholder`,
    /// iTerm2's palette): the hint paints one cell PAST the caret cell
    /// in the same `text_faint` ink, so the caret block stays visible.
    /// Default OFF deliberately: existing apps stay byte-identical,
    /// and the field consumer's own overlay workaround would
    /// double-paint under a silent default flip.
    pub fn placeholder_while_focused(mut self, on: bool) -> TextArea {
        self.placeholder_while_focused = on;
        self
    }

    /// Growth band: the widget occupies `min..=max` rows, tracking its
    /// wrapped content height, then scrolls internally past `max`.
    pub fn rows(mut self, min: i32, max: i32) -> TextArea {
        self.min_rows = min.max(1);
        self.max_rows = max.max(self.min_rows);
        self
    }

    /// Enter-key policy (see [`SubmitPolicy`]; default Enter-submits).
    pub fn submit_policy(mut self, policy: SubmitPolicy) -> TextArea {
        self.policy = policy;
        self
    }

    /// Disabled: not focusable, inert to input, `text_faint` ink.
    pub fn disabled(mut self, disabled: bool) -> TextArea {
        self.disabled = disabled;
        self
    }

    /// Layout override. Replaces the grow-to-content band entirely —
    /// height becomes the caller's own business.
    pub fn layout(mut self, layout: LayoutStyle) -> TextArea {
        self.layout = Some(layout);
        self
    }

    /// After every buffer edit (typing, paste, backspace…).
    pub fn on_change(mut self, f: impl FnMut(&str) + 'static) -> TextArea {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Plain Enter under `SubmitPolicy::EnterSubmits`. The buffer is
    /// NOT cleared automatically — clear + `push_history` from here.
    pub fn on_submit(mut self, f: impl FnMut(&str) + 'static) -> TextArea {
        self.on_submit = Some(Box::new(f));
        self
    }

    /// One-call build with tokens from the app's theme context.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        // Style guide §3.3, TextInput parity: framed widget — side
        // strokes border -> border_focus on focus, placeholder
        // text_faint, caret = the cursor token, selection pair for
        // selected clusters.
        let text_fg = if self.disabled { t.text_faint } else { t.text };
        let ground = t.surface;
        let stroke = t.border;
        let stroke_focus = t.border_focus;
        let placeholder_fg = t.text_faint;
        let sel_bg = t.selection_bg;
        let sel_fg = t.selection_fg;
        let cursor_bg = t.cursor;

        let state = self.state.unwrap_or_else(|| TextAreaState::new(cx));
        let value = state.inner.value;
        let caret = state.inner.caret;
        let focused = state.inner.focused;
        let caret_cell = state.inner.caret_cell;
        let placeholder = self.placeholder;
        let placeholder_while_focused = self.placeholder_while_focused;
        let (min_rows, max_rows) = (self.min_rows, self.max_rows);
        let policy = self.policy;
        let disabled = self.disabled;
        let on_change: TextCallback = Rc::new(RefCell::new(self.on_change));
        let on_submit: TextCallback = Rc::new(RefCell::new(self.on_submit));

        // Wrap width last seen by an event handler (rect.w - 2 strokes).
        // Read untracked by the growth style below: exact after any
        // event; before the first one (or right after a pure resize) the
        // logical-line count stands in until the next event re-measures.
        let width_hint: Rc<std::cell::Cell<i32>> = Rc::new(std::cell::Cell::new(0));

        let handler = {
            let state = state.clone();
            let on_change = on_change.clone();
            let on_submit = on_submit.clone();
            let width_hint = width_hint.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                let width = (ctx.current_rect().w - 2).max(1);
                width_hint.set(width);
                let (consumed, owed) = match ev {
                    UiEvent::Key(k) => {
                        handle_key(&state, k.key, k.mods, width, min_rows, max_rows, policy)
                    }
                    UiEvent::Paste(s) => {
                        // Block paste: whole insertion, newlines KEPT
                        // (normalized), never a submit (0120 §5).
                        let clean = normalize_newlines(s);
                        let mut c = caret.get_untracked();
                        value.update(|text| insert_at_caret(text, &mut c, &clean));
                        finish_edit(&state, &mut c, width, min_rows, max_rows);
                        caret.set(c);
                        (true, Owed::Change)
                    }
                    UiEvent::FocusIn | UiEvent::FocusOut => {
                        // Keep the anchor honest: present while focused,
                        // gone the moment keys leave (owner-driven panel
                        // dismissal keys off this).
                        let mut c = caret.get_untracked();
                        if matches!(ev, UiEvent::FocusIn) {
                            publish_caret_cell(&state, &mut c, ctx.current_rect(), width);
                            caret.set(c);
                        } else {
                            caret_cell.set(None);
                        }
                        (false, Owed::Nothing)
                    }
                    _ => (false, Owed::Nothing),
                };
                if consumed {
                    let mut c = caret.get_untracked();
                    publish_caret_cell(&state, &mut c, ctx.current_rect(), width);
                    caret.set(c);
                    ctx.stop_propagation();
                    // Disposal-safety law (backlog 0297, the 0250 ruling
                    // engine-wide): every widget signal write above —
                    // buffer, caret, caret-cell publish — is DONE, so
                    // the user callback runs LAST and may dispose the
                    // TextArea's scope synchronously (submit-and-close
                    // composers). One deliberate divergence from the
                    // pre-law order: a callback that mutates the buffer
                    // (submit-and-clear) leaves the published caret
                    // cell one event stale — the next event or focus
                    // change re-publishes; anchor consumers (the
                    // completion controller) key off caret MOVEMENT,
                    // not clear-time geometry.
                    match owed {
                        Owed::Change => notify(&on_change, value),
                        Owed::Submit => notify(&on_submit, value),
                        Owed::Nothing => {}
                    }
                }
            }
        };

        // Grow-to-content: height tracks the wrapped row count inside
        // [min_rows, max_rows]; shrink 0 so an overflowing sibling can
        // never crush the composer (0240 #2).
        let layout = self.layout.clone();
        let growth = {
            move || {
                if let Some(fixed) = layout.clone() {
                    return fixed;
                }
                let text = value.get();
                let w = width_hint.get();
                let rows = if w > 0 {
                    RowMap::build(&text, w).len() as i32
                } else {
                    // Pre-layout estimate: logical lines (no wrap yet).
                    text.split('\n').count() as i32
                };
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(rows.clamp(min_rows, max_rows)))
                    .shrink(0.0)
            }
        };

        let mut el = Element::new()
            .style_signal(growth)
            .role(crate::ui::Role::TextArea)
            .access_label(placeholder.clone())
            .access_value(move || value.get_untracked());
        if !disabled {
            el = el
                .focusable()
                .focus_signal(focused)
                .on(Phase::Bubble, handler);
        }
        el.child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let text = value.get();
                let c = caret.get();
                let focused = focused.get() && !disabled;
                let placeholder = placeholder.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .draw(move |canvas, rect| {
                        if rect.is_empty() || rect.w < 3 {
                            return;
                        }
                        let bg = ground;
                        canvas.fill_styled(rect, ' ', &Style::new().fg(text_fg).bg(bg));
                        let stroke_style = Style::new()
                            .fg(if focused { stroke_focus } else { stroke })
                            .bg(bg);
                        for y in rect.y..rect.bottom() {
                            canvas.print_styled(Point::new(rect.x, y), "▐", &stroke_style);
                            canvas.print_styled(
                                Point::new(rect.right() - 1, y),
                                "▌",
                                &stroke_style,
                            );
                        }
                        let tx = rect.x + 1;
                        let tw = rect.w - 2;
                        // Placeholder branches clip to the INTERIOR
                        // (first-app/0284): draw closures clip to damage
                        // regions, not element rects, so an unbounded
                        // print overwrote the widget's own right stroke
                        // and escaped the rect at narrow widths.
                        // `truncate_ellipsis` keeps the hint honest about
                        // being cut (same discipline as List/Table cells).
                        if text.is_empty() && !focused {
                            canvas.print_styled(
                                Point::new(tx, rect.y),
                                &crate::text::truncate_ellipsis(&placeholder, tw),
                                &Style::new().fg(placeholder_fg).bg(bg),
                            );
                            return;
                        }
                        // Focused-and-empty opt-in (backlog first-app/0291):
                        // the hint paints one cell PAST the caret cell —
                        // "where am I typing" beats one hint word — in the
                        // same placeholder ink; the caret block itself
                        // paints below via the normal path, over column 0.
                        // `tw > 1` guards the degenerate one-column field
                        // (only the caret cell exists there).
                        if text.is_empty() && focused && placeholder_while_focused && tw > 1 {
                            canvas.print_styled(
                                Point::new(tx + 1, rect.y),
                                &crate::text::truncate_ellipsis(&placeholder, tw - 1),
                                &Style::new().fg(placeholder_fg).bg(bg),
                            );
                        }
                        let rows = RowMap::build(&text, tw);
                        let total = rows.len() as i32;
                        let top = c.top.clamp(0, (total - rect.h).max(0));
                        let (crow, ccol) = rows.visual(&text, c.byte, c.sticky);
                        let (sel_lo, sel_hi) = selection_range(&c);
                        for vis in 0..rect.h {
                            let row_idx = (top + vis) as usize;
                            if row_idx >= rows.len() {
                                break;
                            }
                            let row = rows.rows[row_idx];
                            let slice = &text[row.start..row.text_end];
                            let mut col = 0i32;
                            for seg in crate::text::segments(slice) {
                                if seg.width == 0 {
                                    continue; // control/zero-width: invisible
                                }
                                if col + seg.width > tw {
                                    break; // hanging whitespace clips here
                                }
                                let byte = row.start + seg.offset;
                                let selected = byte >= sel_lo && byte < sel_hi;
                                let at_cursor = focused && row_idx == crow && byte == c.byte;
                                let style = if at_cursor {
                                    Style::new().fg(bg).bg(cursor_bg)
                                } else if selected {
                                    Style::new().fg(sel_fg).bg(sel_bg)
                                } else {
                                    Style::new().fg(text_fg).bg(bg)
                                };
                                canvas.print_styled(
                                    Point::new(tx + col, rect.y + vis),
                                    seg.cluster,
                                    &style,
                                );
                                col += seg.width;
                            }
                        }
                        // Caret past its row's content (end of row/doc,
                        // or sticky at a soft boundary): a styled blank
                        // in the row's spare tail. `ccol < tw` holds by
                        // the model's room rule (phantom row + sticky
                        // guard); the check stays as a paint guard so a
                        // margin caret can never stomp the last glyph.
                        let cursor_on_cluster = !c.sticky && c.byte < rows.rows[crow].text_end;
                        let vis_row = crow as i32 - top;
                        if focused
                            && !cursor_on_cluster
                            && (0..rect.h).contains(&vis_row)
                            && ccol < tw
                        {
                            canvas.print_styled(
                                Point::new(tx + ccol, rect.y + vis_row),
                                " ",
                                &Style::new().fg(bg).bg(cursor_bg),
                            );
                        }
                    })
                    .build()
            },
        ))
    }
}

impl Default for TextArea {
    fn default() -> Self {
        TextArea::new()
    }
}

/// `\r\n`/`\r` -> `\n` (bracketed paste arrives with whatever line
/// endings the clipboard held; the model speaks `\n`).
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Post-edit bookkeeping shared by every mutating path: clamp the
/// scroll window so the caret row stays visible.
fn finish_edit(state: &TextAreaState, c: &mut Caret, width: i32, min_rows: i32, max_rows: i32) {
    let text = state.inner.value.get_untracked();
    let rows = RowMap::build(&text, width);
    let (crow, _) = rows.visual(&text, c.byte, c.sticky);
    let visible = (rows.len() as i32).clamp(min_rows, max_rows);
    adjust_top(c, crow as i32, rows.len() as i32, visible);
}

/// Publish the caret's screen cell (the completion anchor) from the
/// solved rect available inside handlers.
fn publish_caret_cell(state: &TextAreaState, c: &mut Caret, rect: crate::base::Rect, width: i32) {
    let text = state.inner.value.get_untracked();
    let rows = RowMap::build(&text, width);
    let (crow, ccol) = rows.visual(&text, c.byte, c.sticky);
    let visible = (rows.len() as i32).clamp(1, rect.h.max(1));
    adjust_top(c, crow as i32, rows.len() as i32, visible);
    let cell = Point::new(
        rect.x + 1 + ccol.min((rect.w - 3).max(0)),
        rect.y + (crow as i32 - c.top).clamp(0, (rect.h - 1).max(0)),
    );
    state.inner.caret_cell.set(Some(cell));
}

/// Which user callback one consumed event owes. Owed callbacks fire
/// LAST — after every widget signal write including the caret-cell
/// publish (disposal-safety law, backlog 0297): `handle_key` itself
/// never runs user code, it only reports the debt.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Owed {
    Nothing,
    Change,
    Submit,
}

/// One key against the state. Returns (consumed, owed callback); ALL
/// widget bookkeeping happens here, no user code does.
fn handle_key(
    state: &TextAreaState,
    key: Key,
    mods: crate::ui::Mods,
    width: i32,
    min_rows: i32,
    max_rows: i32,
    policy: SubmitPolicy,
) -> (bool, Owed) {
    let value = state.inner.value;
    let mut c = state.inner.caret.get_untracked();
    let mut text = value.get_untracked();
    let outcome = model::apply_key(&mut text, &mut c, key, mods, width, policy);
    match outcome {
        EditOutcome::Handled { edited } => {
            if edited {
                value.set(text);
            }
            finish_edit(state, &mut c, width, min_rows, max_rows);
            state.inner.caret.set(c);
            (true, if edited { Owed::Change } else { Owed::Nothing })
        }
        EditOutcome::Submit => (true, Owed::Submit),
        EditOutcome::HistoryBack => {
            let recalled = state.inner.history.borrow_mut().back(&text);
            let owed = if let Some(entry) = recalled {
                state.set_text(entry);
                let mut c = state.inner.caret.get_untracked();
                finish_edit(state, &mut c, width, min_rows, max_rows);
                state.inner.caret.set(c);
                Owed::Change
            } else {
                Owed::Nothing
            };
            (true, owed) // the edge arrow is ours even when history is empty
        }
        EditOutcome::HistoryForward => {
            let recalled = state.inner.history.borrow_mut().forward();
            let owed = if let Some(entry) = recalled {
                state.set_text(entry);
                let mut c = state.inner.caret.get_untracked();
                finish_edit(state, &mut c, width, min_rows, max_rows);
                state.inner.caret.set(c);
                Owed::Change
            } else {
                Owed::Nothing
            };
            (true, owed)
        }
        EditOutcome::Ignored => (false, Owed::Nothing),
    }
}

#[cfg(test)]
#[path = "textarea_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "textarea_disposal_tests.rs"]
mod disposal_tests;

#[cfg(test)]
#[path = "textarea_placeholder_tests.rs"]
mod placeholder_tests;

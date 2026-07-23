//! TextInput: single-line editable text field.
//!
//! Editing model: insert/overwrite at a cursor, selection via
//! Shift+arrows (anchor + cursor), word jumps via Alt+arrows, Home/End,
//! Backspace/Delete, whole-`Paste` insertion (never per-char synthesis),
//! horizontal scroll keeps the cursor visible, `on_change` after every
//! edit, `on_submit` on Enter.
//!
//! CLUSTER-ATOMIC EDITING (RT3-2 closed): cursor positions index
//! GRAPHEME CLUSTERS via `text::segments` — a combining sequence or ZWJ
//! emoji family is ONE cursor stop, one Backspace, one selection step;
//! widths come from the same authority, so columns and rendering agree.
//! There is no IME path: composed input arrives as whatever the terminal
//! sends (kitty text events land as chars); dead-key composition happens
//! terminal-side or not at all.
//!
//! MASKED MODE (0510 §masked): `.masked(true)` substitutes one `•` per
//! grapheme cluster in BOTH the draw and the `access_value` export —
//! the accessibility snapshot is shipped off-process by the
//! control-plane band, so a masked field must never export plaintext
//! through the semantic tree (PLATFORM cycle-2 F2). Editing, selection,
//! cursor math, and paste are untouched: masking is a presentation +
//! export substitution, never a second editing model. One deliberate
//! editing-model exception (cycle-3 review F7): WORD JUMPS degrade to
//! whole-field jumps — `word_step` over the real text would park the
//! caret on the secret's word boundaries, telegraphing word count and
//! word lengths to a shoulder-surfer through cursor motion alone, so a
//! masked field treats the whole value as ONE word (the native
//! password-field convention).
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, Mods, Phase, UiEvent};

/// Boxed text callback (`on_change`/`on_submit` builder slots).
pub(crate) type BoxedTextFn = Box<dyn FnMut(&str)>;
/// The same callback SHARED between the key handler and Enter
/// submission (RT4-2 hygiene aliases; HRTB over the borrowed argument
/// comes from the inner `dyn FnMut(&str)`). `pub(crate)`: `TextArea`
/// reuses the alias + `notify` for the same borrow discipline.
pub(crate) type TextCallback = Rc<RefCell<Option<BoxedTextFn>>>;

pub struct TextInput {
    value: Option<Signal<String>>,
    placeholder: String,
    placeholder_while_focused: bool,
    masked: bool,
    layout: Option<LayoutStyle>,
    on_change: Option<BoxedTextFn>,
    on_submit: Option<BoxedTextFn>,
}

/// Editing state shared between the key handler and the renderer.
#[derive(Copy, Clone)]
struct Caret {
    /// CLUSTER index of the cursor (0..=cluster_count).
    cursor: usize,
    /// Selection anchor (cluster index); None = no selection.
    anchor: Option<usize>,
    /// First visible COLUMN (horizontal scroll).
    scroll: i32,
}

/// Cluster geometry of a string, computed once per edit/draw from
/// `text::segments` (THE cluster/width authority): byte boundaries
/// (`len+1` entries) and per-cluster display widths.
///
/// `pub(crate)`: `TextArea` (textarea.rs) reuses this map per visual
/// row — one cluster-math authority for both editors (backlog 0120's
/// "reuse, not re-derive" requirement).
pub(crate) struct ClusterMap {
    bounds: Vec<usize>,
    widths: Vec<i32>,
}

impl ClusterMap {
    pub(crate) fn of(text: &str) -> ClusterMap {
        let mut bounds = Vec::new();
        let mut widths = Vec::new();
        for seg in crate::text::segments(text) {
            bounds.push(seg.offset);
            widths.push(seg.width);
        }
        bounds.push(text.len());
        ClusterMap { bounds, widths }
    }

    /// Cluster count.
    pub(crate) fn len(&self) -> usize {
        self.widths.len()
    }

    /// Byte offset of cluster index `idx` (== text.len() at the end).
    pub(crate) fn byte(&self, idx: usize) -> usize {
        self.bounds[idx.min(self.len())]
    }

    /// Display column where cluster `idx` starts.
    pub(crate) fn col(&self, idx: usize) -> i32 {
        self.widths[..idx.min(self.len())].iter().sum()
    }

    /// Cursor position after the content ending at byte `byte_end`. When
    /// the byte lands MID-cluster (an inserted scalar merged into its
    /// neighbor — ZWJ, combining mark), the cursor snaps past the whole
    /// merged cluster: positions are cluster boundaries, never interiors.
    pub(crate) fn cluster_after(&self, byte_end: usize) -> usize {
        self.bounds.partition_point(|&b| b < byte_end)
    }
}

impl TextInput {
    pub fn new() -> TextInput {
        TextInput {
            value: None,
            placeholder: String::new(),
            placeholder_while_focused: false,
            masked: false,
            layout: None,
            on_change: None,
            on_submit: None,
        }
    }

    /// Bind an external value signal (owned elsewhere); default is an
    /// internal one.
    pub fn value(mut self, value: Signal<String>) -> TextInput {
        self.value = Some(value);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> TextInput {
        self.placeholder = text.into();
        self
    }

    /// Paint the placeholder while focused-and-empty too, beside the
    /// caret (backlog first-app/0291 — `TextArea` parity). Default OFF:
    /// the classic yield-to-caret rule keeps existing apps
    /// byte-identical; see
    /// [`TextArea::placeholder_while_focused`](crate::widgets::TextArea::placeholder_while_focused)
    /// for the full rationale.
    pub fn placeholder_while_focused(mut self, on: bool) -> TextInput {
        self.placeholder_while_focused = on;
        self
    }

    /// Secret/password mode: the DRAW substitutes one `•` per grapheme
    /// cluster (count-honest: a ZWJ family is one bullet; each bullet
    /// occupies its cluster's own width so scroll/cursor geometry is
    /// byte-identical to the unmasked field), and `access_value` — the
    /// accessibility/automation export, a leak surface shipped
    /// off-process — exports the same bullets, never the plaintext.
    /// Editing, selection, cursor math, and paste are untouched; the
    /// bound value signal still holds the real text (the app owns it).
    /// One exception: Alt+arrow WORD jumps treat the whole value as a
    /// single word (they go to start/end, like Home/End, including
    /// Shift extension) — real word boundaries would reveal the
    /// secret's word structure through caret positions.
    /// The placeholder is not secret and renders as usual. A reveal
    /// toggle is a rebuild with `masked(false)` (wrap the field in your
    /// `dyn_view_scoped` on the reveal signal).
    pub fn masked(mut self, masked: bool) -> TextInput {
        self.masked = masked;
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> TextInput {
        self.layout = Some(layout);
        self
    }

    pub fn on_change(mut self, f: impl FnMut(&str) + 'static) -> TextInput {
        self.on_change = Some(Box::new(f));
        self
    }

    pub fn on_submit(mut self, f: impl FnMut(&str) + 'static) -> TextInput {
        self.on_submit = Some(Box::new(f));
        self
    }

    /// Canonical one-call build (cycle 8): tokens resolve from the
    /// app's THEME CONTEXT (a tracked read — building inside a
    /// `dyn_view` re-renders on theme switch) and the finished `View`
    /// comes back ready for `.child(..)`. Use `element(cx, &tokens)`
    /// when you need explicit theming or extra Element customization.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        // Style guide §3.3: the input is a FRAMED widget — side strokes
        // `border` -> `border_focus` on focus (the frame carries focus;
        // the ground never doubles as a focus signal), placeholder
        // `text_faint`, caret = the `cursor` token.
        let text_fg = t.text;
        let ground = t.surface;
        let stroke = t.border;
        let stroke_focus = t.border_focus;
        let placeholder_fg = t.text_faint;
        let sel_bg = t.selection_bg;
        let sel_fg = t.selection_fg;
        let cursor_bg = t.cursor;

        let value = self.value.unwrap_or_else(|| cx.signal(String::new()));
        let caret = cx.signal(Caret {
            cursor: 0,
            anchor: None,
            scroll: 0,
        });
        let focused = cx.signal(false);
        let placeholder = self.placeholder;
        let placeholder_while_focused = self.placeholder_while_focused;
        let masked = self.masked;
        let on_change: TextCallback = Rc::new(RefCell::new(self.on_change));
        let on_submit: TextCallback = Rc::new(RefCell::new(self.on_submit));

        // shrink 0: the input's one row never vanishes under column
        // overflow (0240 #2); width stays flexible through grow.
        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .height(Dimension::Cells(1))
                .grow(1.0)
                .shrink(0.0)
        });

        let handler = {
            let on_change = on_change.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                // Text area = rect minus the two stroke columns.
                let width = (ctx.current_rect().w - 2).max(1);
                match ev {
                    UiEvent::Key(k) => {
                        if edit_key(
                            k.key, k.mods, value, caret, width, masked, &on_change, &on_submit,
                        ) {
                            ctx.stop_propagation();
                        }
                    }
                    UiEvent::Paste(s) => {
                        // Single-line field: fold line breaks to spaces.
                        let clean: String = s
                            .chars()
                            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                            .collect();
                        insert_text(&clean, value, caret, width);
                        notify(&on_change, value);
                        ctx.stop_propagation();
                    }
                    _ => {}
                }
            }
        };

        Element::new()
            .style(layout)
            .role(crate::ui::Role::Input)
            .access_label(placeholder.clone())
            .access_value(move || {
                // Masked fields redact AT THE WIDGET (0510 §masked):
                // this closure is the semantic-tree export the
                // control-plane band ships off-process — plaintext must
                // never leave through it. Bullets keep the
                // grapheme-cluster count (and nothing else).
                if masked {
                    value.with_untracked(|v| "•".repeat(crate::text::segments(v).count()))
                } else {
                    value.get_untracked()
                }
            })
            .focusable()
            .focus_signal(focused)
            .on(Phase::Bubble, handler)
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
                move || {
                    let text = value.get();
                    let caret_now = caret.get();
                    let focused = focused.get();
                    let placeholder = placeholder.clone();
                    Element::new()
                        .style(LayoutStyle::default().width(Dimension::Percent(1.0)))
                        .draw(move |canvas, rect| {
                            if rect.is_empty() || rect.w < 3 {
                                return;
                            }
                            let bg = ground;
                            canvas.fill_styled(rect, ' ', &Style::new().fg(text_fg).bg(bg));
                            // Frame strokes (§3.2 bordered row): border ->
                            // border_focus on focus. One glyph per side in
                            // a 1-row field.
                            let stroke_style = Style::new()
                                .fg(if focused { stroke_focus } else { stroke })
                                .bg(bg);
                            canvas.print_styled(rect.origin(), "▐", &stroke_style);
                            canvas.print_styled(
                                crate::base::Point::new(rect.right() - 1, rect.y),
                                "▌",
                                &stroke_style,
                            );
                            let tx = rect.x + 1; // text area start
                            let tw = rect.w - 2;
                            if text.is_empty() && !focused {
                                canvas.print_styled(
                                    crate::base::Point::new(tx, rect.y),
                                    &placeholder,
                                    &Style::new().fg(placeholder_fg).bg(bg),
                                );
                                return;
                            }
                            // Focused-and-empty opt-in (first-app/0291):
                            // hint one cell PAST the caret cell, same ink;
                            // the trailing-cursor paint below keeps the
                            // caret block visible at column 0. `tw > 1`
                            // guards the one-column degenerate field.
                            if text.is_empty() && focused && placeholder_while_focused && tw > 1 {
                                canvas.print_styled(
                                    crate::base::Point::new(tx + 1, rect.y),
                                    &placeholder,
                                    &Style::new().fg(placeholder_fg).bg(bg),
                                );
                            }
                            // Cluster-indexed paint: selection and cursor
                            // highlight whole clusters — a wide emoji
                            // cursor is a two-cell block, never half.
                            let (sel_lo, sel_hi) = selection_range(&caret_now);
                            let mut col = -caret_now.scroll;
                            let mut count = 0usize;
                            for (i, seg) in crate::text::segments(&text).enumerate() {
                                count = i + 1;
                                let w = seg.width;
                                if w > 0 && col + w > 0 && col + w <= tw {
                                    let selected = i >= sel_lo && i < sel_hi;
                                    let at_cursor = focused && i == caret_now.cursor;
                                    let style = if at_cursor {
                                        Style::new().fg(bg).bg(cursor_bg)
                                    } else if selected {
                                        Style::new().fg(sel_fg).bg(sel_bg)
                                    } else {
                                        Style::new().fg(text_fg).bg(bg)
                                    };
                                    if masked {
                                        // One bullet per cluster in the
                                        // cluster's own width slot —
                                        // count-honest, geometry
                                        // identical to the plain draw
                                        // (padding cells carry the same
                                        // style so selection/cursor
                                        // blocks stay whole).
                                        canvas.print_styled(
                                            crate::base::Point::new(tx + col, rect.y),
                                            "•",
                                            &style,
                                        );
                                        for pad in 1..w {
                                            canvas.print_styled(
                                                crate::base::Point::new(tx + col + pad, rect.y),
                                                " ",
                                                &style,
                                            );
                                        }
                                    } else {
                                        canvas.print_styled(
                                            crate::base::Point::new(tx + col, rect.y),
                                            seg.cluster,
                                            &style,
                                        );
                                    }
                                }
                                col += w;
                            }
                            // Cursor past the last cluster: a styled blank.
                            if focused && caret_now.cursor >= count && col < tw && col >= 0 {
                                canvas.print_styled(
                                    crate::base::Point::new(tx + col, rect.y),
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

impl Default for TextInput {
    fn default() -> Self {
        TextInput::new()
    }
}

/// Run a text callback with the CURRENT value. The value is cloned OUT
/// first — running user code inside `with_untracked` holds the cell
/// borrow, so a handler that writes the same signal (clear-on-submit,
/// input masks) would hit "RefCell already borrowed". Found by the
/// cycle-7 sixty-line-app proof; a String clone is the honest price.
pub(crate) fn notify(cb: &TextCallback, value: Signal<String>) {
    let snapshot = value.get_untracked();
    if let Some(f) = cb.borrow_mut().as_mut() {
        f(&snapshot);
    }
}

fn selection_range(c: &Caret) -> (usize, usize) {
    match c.anchor {
        Some(a) if a != c.cursor => (a.min(c.cursor), a.max(c.cursor)),
        _ => (usize::MAX, usize::MAX), // empty range
    }
}

/// Keep the cursor column visible inside `width` (1-cell margin).
fn adjust_scroll(caret: &mut Caret, map: &ClusterMap, width: i32) {
    let col = map.col(caret.cursor);
    if col < caret.scroll {
        caret.scroll = col;
    }
    if col >= caret.scroll + width {
        caret.scroll = col - width + 1;
    }
    caret.scroll = caret.scroll.max(0);
}

/// Delete the selection if any; returns true when something was removed.
/// Cluster indices convert to byte ranges through the map — the removal
/// is cluster-atomic by construction.
fn delete_selection(text: &mut String, caret: &mut Caret) -> bool {
    let (lo, hi) = selection_range(caret);
    if lo == usize::MAX {
        return false;
    }
    let map = ClusterMap::of(text);
    text.replace_range(map.byte(lo)..map.byte(hi), "");
    caret.cursor = lo;
    caret.anchor = None;
    true
}

fn insert_text(s: &str, value: Signal<String>, caret: Signal<Caret>, width: i32) {
    let mut c = caret.get_untracked();
    value.update(|text| {
        delete_selection(text, &mut c);
        let map = ClusterMap::of(text);
        let insert_at = c.cursor.min(map.len());
        let insert_byte = map.byte(insert_at);
        text.insert_str(insert_byte, s);
        // Re-anchor on the POST-insert map: an inserted ZWJ/combining
        // scalar can MERGE clusters, so `old index + inserted clusters`
        // may not exist — the byte end always does.
        let map = ClusterMap::of(text);
        c.cursor = map.cluster_after(insert_byte + s.len());
        adjust_scroll(&mut c, &map, width);
    });
    caret.set(c);
}

/// Word-ness of the cluster starting at byte `at` (first scalar decides —
/// a ZWJ family is "not word", which groups emoji runs like separators).
/// `pub(crate)`: shared with `TextArea` (one word-jump policy).
pub(crate) fn cluster_is_word(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Next word boundary in CLUSTER indices (alt+arrows): skip separators,
/// then a word run. `pub(crate)`: shared with `TextArea`.
pub(crate) fn word_step(text: &str, map: &ClusterMap, from: usize, dir: i32) -> usize {
    let n = map.len();
    let is_word = |i: usize| cluster_is_word(text, map.byte(i));
    if dir > 0 {
        let mut i = from;
        while i < n && !is_word(i) {
            i += 1;
        }
        while i < n && is_word(i) {
            i += 1;
        }
        i
    } else {
        let mut i = from;
        while i > 0 && !is_word(i - 1) {
            i -= 1;
        }
        while i > 0 && is_word(i - 1) {
            i -= 1;
        }
        i
    }
}

/// Apply one key to the editing state. Returns true when consumed.
/// `masked` degrades word jumps to whole-field jumps (F7: word
/// boundaries over the real text would leak the secret's word
/// structure through caret positions).
#[allow(clippy::too_many_arguments)]
fn edit_key(
    key: Key,
    mods: Mods,
    value: Signal<String>,
    caret: Signal<Caret>,
    width: i32,
    masked: bool,
    on_change: &TextCallback,
    on_submit: &TextCallback,
) -> bool {
    let shift = mods.contains(Mods::SHIFT);
    let alt = mods.contains(Mods::ALT);
    let ctrl = mods.contains(Mods::CTRL);
    let mut c = caret.get_untracked();
    let (map, text_snapshot) = value.with_untracked(|v| (ClusterMap::of(v), v.clone()));
    let len = map.len();
    // Defensive re-clamp: external value.set / merge-on-insert can leave
    // a stale index past the cluster count.
    c.cursor = c.cursor.min(len);
    if let Some(a) = c.anchor {
        c.anchor = Some(a.min(len));
    }

    // Cursor motion (with optional selection extension) --------------------
    let move_to = |c: &mut Caret, target: usize| {
        if shift {
            if c.anchor.is_none() {
                c.anchor = Some(c.cursor);
            }
        } else {
            c.anchor = None;
        }
        c.cursor = target.min(len);
    };
    match key {
        Key::Left => {
            let target = if alt && masked {
                0 // masked: the whole value is ONE word (F7)
            } else if alt {
                word_step(&text_snapshot, &map, c.cursor, -1)
            } else {
                c.cursor.saturating_sub(1)
            };
            move_to(&mut c, target);
            adjust_scroll(&mut c, &map, width);
            caret.set(c);
            return true;
        }
        Key::Right => {
            let target = if alt && masked {
                len // masked: the whole value is ONE word (F7)
            } else if alt {
                word_step(&text_snapshot, &map, c.cursor, 1)
            } else {
                c.cursor + 1
            };
            move_to(&mut c, target);
            adjust_scroll(&mut c, &map, width);
            caret.set(c);
            return true;
        }
        Key::Home => {
            move_to(&mut c, 0);
            adjust_scroll(&mut c, &map, width);
            caret.set(c);
            return true;
        }
        Key::End => {
            move_to(&mut c, len);
            adjust_scroll(&mut c, &map, width);
            caret.set(c);
            return true;
        }
        _ => {}
    }

    // Edits (all cluster-atomic: byte ranges come from the map) -----------
    match key {
        Key::Char(ch) if !ctrl && !alt => {
            let mut buf = [0u8; 4];
            insert_text(ch.encode_utf8(&mut buf), value, caret, width);
            notify(on_change, value);
            true
        }
        Key::Backspace => {
            value.update(|text| {
                if !delete_selection(text, &mut c) && c.cursor > 0 {
                    let map = ClusterMap::of(text);
                    // ONE cluster gone — a ZWJ family or combining
                    // sequence deletes whole (RT3-2).
                    text.replace_range(map.byte(c.cursor - 1)..map.byte(c.cursor), "");
                    c.cursor -= 1;
                }
                let map = ClusterMap::of(text);
                adjust_scroll(&mut c, &map, width);
            });
            caret.set(c);
            notify(on_change, value);
            true
        }
        Key::Delete => {
            value.update(|text| {
                if !delete_selection(text, &mut c) {
                    let map = ClusterMap::of(text);
                    if c.cursor < map.len() {
                        text.replace_range(map.byte(c.cursor)..map.byte(c.cursor + 1), "");
                    }
                }
            });
            caret.set(c);
            notify(on_change, value);
            true
        }
        Key::Enter => {
            // Same borrow rule as `notify`: clone out, then call — a
            // submit handler clearing the input must not deadlock.
            notify(on_submit, value);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;

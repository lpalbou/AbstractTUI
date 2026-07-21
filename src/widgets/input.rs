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
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, Mods, Phase, UiEvent};

/// Boxed text callback (`on_change`/`on_submit` builder slots).
type BoxedTextFn = Box<dyn FnMut(&str)>;
/// The same callback SHARED between the key handler and Enter
/// submission (RT4-2 hygiene aliases; HRTB over the borrowed argument
/// comes from the inner `dyn FnMut(&str)`).
type TextCallback = Rc<RefCell<Option<BoxedTextFn>>>;

pub struct TextInput {
    value: Option<Signal<String>>,
    placeholder: String,
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
struct ClusterMap {
    bounds: Vec<usize>,
    widths: Vec<i32>,
}

impl ClusterMap {
    fn of(text: &str) -> ClusterMap {
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
    fn len(&self) -> usize {
        self.widths.len()
    }

    /// Byte offset of cluster index `idx` (== text.len() at the end).
    fn byte(&self, idx: usize) -> usize {
        self.bounds[idx.min(self.len())]
    }

    /// Display column where cluster `idx` starts.
    fn col(&self, idx: usize) -> i32 {
        self.widths[..idx.min(self.len())].iter().sum()
    }

    /// Cursor position after the content ending at byte `byte_end`. When
    /// the byte lands MID-cluster (an inserted scalar merged into its
    /// neighbor — ZWJ, combining mark), the cursor snaps past the whole
    /// merged cluster: positions are cluster boundaries, never interiors.
    fn cluster_after(&self, byte_end: usize) -> usize {
        self.bounds.partition_point(|&b| b < byte_end)
    }
}

impl TextInput {
    pub fn new() -> TextInput {
        TextInput {
            value: None,
            placeholder: String::new(),
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
        let on_change: TextCallback = Rc::new(RefCell::new(self.on_change));
        let on_submit: TextCallback = Rc::new(RefCell::new(self.on_submit));

        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().height(Dimension::Cells(1)).grow(1.0));

        let handler = {
            let on_change = on_change.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                // Text area = rect minus the two stroke columns.
                let width = (ctx.current_rect().w - 2).max(1);
                match ev {
                    UiEvent::Key(k) => {
                        if edit_key(k.key, k.mods, value, caret, width, &on_change, &on_submit) {
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
            .access_value(move || value.get_untracked())
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
                                    canvas.print_styled(
                                        crate::base::Point::new(tx + col, rect.y),
                                        seg.cluster,
                                        &style,
                                    );
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
fn notify(cb: &TextCallback, value: Signal<String>) {
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
fn cluster_is_word(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Next word boundary in CLUSTER indices (alt+arrows): skip separators,
/// then a word run.
fn word_step(text: &str, map: &ClusterMap, from: usize, dir: i32) -> usize {
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
#[allow(clippy::too_many_arguments)]
fn edit_key(
    key: Key,
    mods: Mods,
    value: Signal<String>,
    caret: Signal<Caret>,
    width: i32,
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
            let target = if alt {
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
            let target = if alt {
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
mod tests {
    use super::*;
    use crate::base::{Point, Size};
    use crate::theme::default_theme;
    use crate::ui::{Key, Mods, UiEvent, UiTree};
    use crate::widgets::itest_util::{key, key_mod, mount_widget, render, type_str};

    fn focused_input(size: Size) -> (crate::reactive::RootScope, UiTree, Signal<String>) {
        let t = &default_theme().tokens;
        let holder: Rc<RefCell<Option<Signal<String>>>> = Rc::new(RefCell::new(None));
        let h = holder.clone();
        let (root, mut tree) = mount_widget(size, move |cx| {
            let value = cx.signal(String::new());
            *h.borrow_mut() = Some(value);
            TextInput::new()
                .value(value)
                .placeholder("type here")
                .element(cx, t)
                .build()
        });
        key(&mut tree, Key::Tab); // focus the field
        let sig = holder.borrow().expect("value signal");
        (root, tree, sig)
    }

    #[test]
    fn typing_inserts_and_renders_inside_the_frame() {
        let theme = default_theme();
        let size = Size::new(16, 1);
        let (_root, mut tree, value) = focused_input(size);
        type_str(&mut tree, "hello");
        assert_eq!(value.get_untracked(), "hello");
        let canvas = render(&mut tree, size);
        assert_eq!(
            canvas.cell(Point::new(1, 0)).unwrap().0,
            'h',
            "text starts after the stroke"
        );
        assert!(canvas.row_text(0).contains("hello"));
        // Focused frame wears border_focus (§3.2 bordered row).
        assert_eq!(
            canvas.cell(Point::new(0, 0)).unwrap().1,
            theme.tokens.border_focus
        );
    }

    #[test]
    fn backspace_delete_home_end_word_jump() {
        let size = Size::new(20, 1);
        let (_root, mut tree, value) = focused_input(size);
        type_str(&mut tree, "one two three");
        key(&mut tree, Key::Backspace); // "one two thre"
        assert_eq!(value.get_untracked(), "one two thre");
        key_mod(&mut tree, Key::Left, Mods::ALT); // to start of "thre"
        key(&mut tree, Key::Delete); // "one two hre"
        assert_eq!(value.get_untracked(), "one two hre");
        key(&mut tree, Key::Home);
        key(&mut tree, Key::Delete);
        assert_eq!(value.get_untracked(), "ne two hre");
        key(&mut tree, Key::End);
        type_str(&mut tree, "!");
        assert_eq!(value.get_untracked(), "ne two hre!");
    }

    #[test]
    fn selection_replaces_on_type_and_renders_selected_style() {
        let theme = default_theme();
        let t = &theme.tokens;
        let size = Size::new(20, 1);
        let (_root, mut tree, value) = focused_input(size);
        type_str(&mut tree, "abcdef");
        key(&mut tree, Key::Home);
        key_mod(&mut tree, Key::Right, Mods::SHIFT);
        key_mod(&mut tree, Key::Right, Mods::SHIFT); // select "ab"
        let canvas = render(&mut tree, size);
        assert_eq!(canvas.cell(Point::new(1, 0)).unwrap().2, t.selection_bg);
        type_str(&mut tree, "X"); // replaces the selection
        assert_eq!(value.get_untracked(), "Xcdef");
    }

    #[test]
    fn paste_goes_in_whole_and_scrolls_to_cursor() {
        let size = Size::new(8, 1);
        let (_root, mut tree, value) = focused_input(size);
        tree.dispatch(&UiEvent::Paste("pasted line\nwith break".into()));
        assert_eq!(value.get_untracked(), "pasted line with break");
        // Cursor is at the end; the visible window must include it: the
        // start of the text is scrolled out of the frame.
        let canvas = render(&mut tree, size);
        assert_ne!(
            canvas.cell(Point::new(1, 0)).unwrap().0,
            'p',
            "scrolled: {:?}",
            canvas.row_text(0)
        );
    }

    #[test]
    fn submit_fires_with_current_value() {
        let t = &default_theme().tokens;
        let submitted: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let s2 = submitted.clone();
        let (_root, mut tree) = mount_widget(Size::new(16, 1), move |cx| {
            TextInput::new()
                .on_submit(move |v| s2.borrow_mut().push(v.to_string()))
                .element(cx, t)
                .build()
        });
        key(&mut tree, Key::Tab);
        type_str(&mut tree, "ok");
        key(&mut tree, Key::Enter);
        assert_eq!(*submitted.borrow(), vec!["ok".to_string()]);
    }

    #[test]
    fn placeholder_shows_only_unfocused_empty() {
        let size = Size::new(16, 1);
        let t = &default_theme().tokens;
        let (_root, mut tree) = mount_widget(size, |cx| {
            TextInput::new()
                .placeholder("type here")
                .element(cx, t)
                .build()
        });
        let theme = default_theme();
        let canvas = render(&mut tree, size);
        assert!(canvas.row_text(0).contains("type here"));
        assert_eq!(
            canvas.cell(Point::new(1, 0)).unwrap().1,
            theme.tokens.text_faint,
            "placeholder ink is text_faint (§3)"
        );
        key(&mut tree, Key::Tab); // focus hides placeholder, shows cursor
        let canvas = render(&mut tree, size);
        assert!(!canvas.row_text(0).contains("type here"));
    }
}

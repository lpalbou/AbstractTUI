//! `RowSelect`: [`List`](crate::widgets::List)'s keyboard selection,
//! sticky-selection-by-key and ensure-visible — over rows this widget
//! does NOT render.
//!
//! `List` renders its own single-line rows, and that is the whole of
//! what it costs you: an item's extra rows reserve SPACE, and wrapped
//! multi-row item CONTENT is not planned (`list.rs`). A surface whose
//! rows are genuinely two lines — a roster with a name above a mission,
//! a result with a title above a path — has to be a
//! [`Scroll`](crate::widgets::Scroll) of arbitrary `Element`s, and that
//! trade used to cost the keyboard: arrows, Home/End, Page keys, and
//! selection that survives a data mutation.
//!
//! Those are separable from rendering, so they are separated. `RowSelect`
//! WRAPS your content and drives the same `SelectionModel` `List` drives
//! — the extraction is shared code, not a parallel implementation, so
//! `List`'s own guards are this module's regression net too.
//!
//! ```
//! use abstracttui::base::Size;
//! use abstracttui::layout::{Dimension, Style as LayoutStyle};
//! use abstracttui::reactive::create_root;
//! use abstracttui::ui::{text, BufferCanvas, Element, UiTree};
//! use abstracttui::widgets::{RowSelect, Scroll};
//!
//! let members = ["ada", "grace"];
//! let mut tree = UiTree::new(Size::new(16, 4));
//! let (root, ()) = create_root(|cx| {
//!     let sel = cx.signal(0usize);
//!     let offset = cx.signal(0i32);
//!     // Two-line rows: the thing a List cannot render.
//!     let mut rows = Element::new().style(LayoutStyle::column());
//!     for (i, name) in members.iter().enumerate() {
//!         rows = rows.child(
//!             Element::new()
//!                 .style(LayoutStyle::column().height(Dimension::Cells(2)))
//!                 .child(text(if i == 0 { format!("> {name}") } else { format!("  {name}") }))
//!                 .child(text("  owns the hub"))
//!                 .build(),
//!         );
//!     }
//!     let scroll = Scroll::new(rows.build()).offset_y(offset).view(cx);
//!     let view = RowSelect::new(members)
//!         .row_heights(|_| 2)
//!         .selection(sel)
//!         .offset_y(offset)
//!         .wrap(cx, scroll)
//!         .build();
//!     tree.mount(cx, view);
//! });
//! let mut canvas = BufferCanvas::new(Size::new(16, 4));
//! tree.draw(&mut canvas);
//! assert!(canvas.row_text(0).contains("ada"));
//! assert!(canvas.row_text(1).contains("owns the hub"));
//! root.dispose();
//! ```
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::ui::{Element, EventCtx, Key, Mods, MouseButton, MouseKind, Phase, UiEvent, View};

type HeightFn = Box<dyn Fn(usize) -> i32>;

/// Prefix sums over per-row heights: `out[i]` is the first CONTENT ROW
/// of row `i`, `out[len]` the total. A uniform list gets the identity
/// prefix, so windowing has ONE code path.
pub(crate) fn prefix_sums(len: usize, height: impl Fn(usize) -> i32) -> Vec<i32> {
    let mut out = Vec::with_capacity(len + 1);
    let mut acc = 0i32;
    out.push(0);
    for i in 0..len {
        acc += height(i).max(1);
        out.push(acc);
    }
    out
}

/// The navigation keys a selectable list owns, mapped to a target index.
/// `None` for anything else — the caller must NOT consume those.
pub(crate) fn nav_target(key: Key, cur: usize, len: usize, page: usize) -> Option<usize> {
    Some(match key {
        Key::Up => cur.saturating_sub(1),
        Key::Down => cur + 1,
        Key::PageUp => cur.saturating_sub(page),
        Key::PageDown => cur + page,
        Key::Home => 0,
        Key::End => len.saturating_sub(1),
        _ => return None,
    })
}

/// Selection over a windowed row list, independent of how the rows are
/// DRAWN: the shared core behind [`List`](crate::widgets::List) and
/// [`RowSelect`].
///
/// Offsets are CONTENT CELL ROWS (the `Scroll::offset_y` convention) and
/// item lookup is a binary search over [`prefix_sums`], so variable row
/// heights cost nothing extra.
#[derive(Clone)]
pub(crate) struct SelectionModel {
    pub(crate) len: usize,
    /// Stable per-row identity; `None` = index-only selection.
    pub(crate) keys: Option<Rc<Vec<String>>>,
    pub(crate) prefix: Rc<Vec<i32>>,
    pub(crate) selection: Signal<usize>,
    pub(crate) selection_key: Option<Signal<String>>,
    pub(crate) offset: Signal<i32>,
}

impl SelectionModel {
    pub(crate) fn total_rows(&self) -> i32 {
        *self.prefix.last().unwrap_or(&0)
    }

    /// The row index owning CONTENT ROW `row`, or `None` past the end.
    pub(crate) fn row_at(&self, row: i32) -> Option<usize> {
        if row < 0 || row >= self.total_rows() {
            return None;
        }
        let idx = self.prefix.partition_point(|&p| p <= row).saturating_sub(1);
        (idx < self.len).then_some(idx)
    }

    /// Scroll `offset` the least amount that brings row `idx` fully into
    /// a `view_h`-row viewport. Shared by keyboard/click selection and
    /// the `scroll_to` command so the two can never drift apart.
    pub(crate) fn ensure_visible(&self, idx: usize, view_h: i32) {
        let total_rows = self.total_rows();
        let top = self.prefix[idx];
        let bottom = self.prefix[idx + 1];
        self.offset.update(|o| {
            if top < *o {
                *o = top;
            }
            if view_h > 0 && bottom > *o + view_h {
                *o = bottom - view_h;
            }
            *o = (*o).clamp(0, (total_rows - view_h.max(1)).max(0));
        });
    }

    /// Settle the selection against THIS build's rows. Rows can vanish
    /// between builds (a dismiss, a filter, a server push), and a
    /// selection naming a row that no longer exists is a real defect:
    /// nothing highlights, `access_value` announces a phantom row to a
    /// screen reader, and the first arrow key moves the wrong way. So
    /// the index is always re-derived and always in range.
    ///
    /// With a key bound this is STICKY SELECTION: the selected key's
    /// CURRENT index wins, so a mutation that MOVES the selected row
    /// keeps the same LOGICAL row selected. When the key is gone the
    /// SLOT is held, clamped — removing a row leaves the next one
    /// selected, and removing the last leaves the new last.
    pub(crate) fn settle(&self) {
        if self.len == 0 {
            return;
        }
        let by_key = self
            .selection_key
            .zip(self.keys.as_ref())
            .and_then(|(sig, keys)| {
                let wanted = sig.get_untracked();
                keys.iter().position(|k| *k == wanted)
            });
        let idx = by_key.unwrap_or_else(|| self.selection.get_untracked().min(self.len - 1));
        self.selection.set_if_changed(idx);
        if let (Some(sig), Some(keys)) = (self.selection_key, self.keys.as_ref()) {
            sig.set_if_changed(keys[idx].clone());
        }
    }

    /// Move the selection to `target` (clamped) and scroll it into view.
    /// Returns whether the selection actually MOVED — the caller fires
    /// its `on_select` notification only then.
    ///
    /// ALL bookkeeping lands here, BEFORE any user callback (the 0250
    /// disposal-safety law): a callback that disposes the owning scope
    /// must find no widget code left to run on dead signals.
    pub(crate) fn select(&self, target: usize, view_h: i32) -> bool {
        if self.len == 0 {
            return false; // nothing to select (prefix has no row span)
        }
        let target = target.min(self.len - 1);
        let changed = self.selection.get_untracked() != target;
        if changed {
            self.selection.set(target);
            if let (Some(key_sig), Some(keys)) = (self.selection_key, self.keys.as_ref()) {
                if let Some(k) = keys.get(target) {
                    key_sig.set(k.clone());
                }
            }
        }
        self.ensure_visible(target, view_h);
        changed
    }
}

/// Keyboard selection for rows YOU render: wrap a
/// [`Scroll`](crate::widgets::Scroll) of arbitrary `Element`s and it
/// gains [`List`](crate::widgets::List)'s arrows, Home/End, Page keys,
/// click-to-select, activation, ensure-visible, and sticky selection by
/// key — while the rows stay yours, multi-line and all.
///
/// Bind [`selection`](RowSelect::selection) to read which row is
/// selected while you build the rows, and share one
/// [`offset_y`](RowSelect::offset_y) signal with the `Scroll` so
/// ensure-visible can move the viewport.
///
/// # What it assumes about your rows
///
/// Row `i` occupies content rows `[prefix[i], prefix[i+1])`, where the
/// heights come from [`row_heights`](RowSelect::row_heights) (default
/// 1). That is exactly what a column of fixed-height children inside a
/// `Scroll` lays out, and it is what ensure-visible and click hit-testing
/// both key off — so declare the height you actually gave the row.
///
/// # What it takes over
///
/// The navigation keys are claimed in the CAPTURE phase, before the
/// content sees them, because the `Scroll` inside would otherwise scroll
/// on the same arrows. Unmodified `Up`/`Down`/`PageUp`/`PageDown`/
/// `Home`/`End` and (when [`on_activate`](RowSelect::on_activate) is
/// bound) `Enter`/`Space` therefore belong to the RowSelect for its whole
/// subtree — do not put a text editor inside a selectable row. Modified
/// chords are never touched, and the wheel and the scrollbar stay the
/// `Scroll`'s.
///
/// # Tab stops
///
/// The wrapper is focusable by default so the keyboard reaches it even
/// when nothing inside can hold focus. A `Scroll` is ALSO focusable, so
/// the canonical composition has two tab stops that behave identically
/// (the RowSelect intercepts either way). Call
/// [`focusable(false)`](RowSelect::focusable) when the content already
/// carries the tab stop and you want exactly one.
pub struct RowSelect {
    keys: Vec<String>,
    heights: Option<HeightFn>,
    selection: Option<Signal<usize>>,
    selection_key: Option<Signal<String>>,
    offset_y: Option<Signal<i32>>,
    scroll_to: Option<Signal<Option<usize>>>,
    focused: Option<Signal<bool>>,
    focusable: bool,
    layout: Option<LayoutStyle>,
    on_select: Option<Box<dyn FnMut(usize)>>,
    on_activate: Option<Box<dyn FnMut(usize)>>,
}

impl RowSelect {
    /// One stable KEY per row, in row order — the identity sticky
    /// selection re-finds after a data mutation. Rows with no natural
    /// identity can pass their index as a string; bind no
    /// [`selection_key`](RowSelect::selection_key) and selection is
    /// index-only, exactly as an unkeyed `List`.
    pub fn new<I, S>(keys: I) -> RowSelect
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        RowSelect {
            keys: keys.into_iter().map(Into::into).collect(),
            heights: None,
            selection: None,
            selection_key: None,
            offset_y: None,
            scroll_to: None,
            focused: None,
            focusable: true,
            layout: None,
            on_select: None,
            on_activate: None,
        }
    }

    /// Per-row height in cell rows (min 1); without it every row is one
    /// row tall. This is what makes multi-line rows work — it is how
    /// ensure-visible and click hit-testing find a row.
    pub fn row_heights(mut self, f: impl Fn(usize) -> i32 + 'static) -> RowSelect {
        self.heights = Some(Box::new(f));
        self
    }

    /// Bind the selected INDEX. Bind it: your row builder reads this to
    /// know which row to draw selected.
    pub fn selection(mut self, selection: Signal<usize>) -> RowSelect {
        self.selection = Some(selection);
        self
    }

    /// Bind the selected row's KEY — the sticky half. With it bound, a
    /// rebuild re-finds the key's current index, so a mutation that
    /// moves the selected row keeps that row selected.
    pub fn selection_key(mut self, key: Signal<String>) -> RowSelect {
        self.selection_key = Some(key);
        self
    }

    /// The first visible CONTENT ROW — share this signal with the
    /// wrapped [`Scroll`](crate::widgets::Scroll)'s
    /// [`offset_y`](crate::widgets::Scroll::offset_y), or ensure-visible
    /// moves a viewport nobody is reading.
    pub fn offset_y(mut self, offset: Signal<i32>) -> RowSelect {
        self.offset_y = Some(offset);
        self
    }

    /// Command signal: set `Some(index)` to scroll that row into view;
    /// the request is consumed (reset to `None`).
    pub fn scroll_to(mut self, request: Signal<Option<usize>>) -> RowSelect {
        self.scroll_to = Some(request);
        self
    }

    /// Bind an external focus signal: true while the wrapper holds
    /// keyboard focus.
    pub fn focus_signal(mut self, focused: Signal<bool>) -> RowSelect {
        self.focused = Some(focused);
        self
    }

    /// Whether the wrapper is itself a tab stop (default `true`). Turn
    /// it off when the content already carries one — see the type docs.
    pub fn focusable(mut self, focusable: bool) -> RowSelect {
        self.focusable = focusable;
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> RowSelect {
        self.layout = Some(layout);
        self
    }

    /// Selection-changed NOTIFICATION — fires when the selected index
    /// MOVES. It is not a commitment; see
    /// [`on_activate`](RowSelect::on_activate) (the 0250 ruling, the same
    /// contract `List` follows).
    pub fn on_select(mut self, f: impl FnMut(usize) + 'static) -> RowSelect {
        self.on_select = Some(Box::new(f));
        self
    }

    /// ACTIVATION: the user committed the selected row — Enter, Space,
    /// or a press on the ALREADY-selected row. Unbound, Enter and Space
    /// pass straight through to the app's own shortcuts.
    pub fn on_activate(mut self, f: impl FnMut(usize) + 'static) -> RowSelect {
        self.on_activate = Some(Box::new(f));
        self
    }

    /// Wrap `content` (normally a [`Scroll`](crate::widgets::Scroll) of
    /// your rows) and return the element to mount.
    pub fn wrap(self, cx: Scope, content: View) -> Element {
        let len = self.keys.len();
        let keys = Rc::new(self.keys);
        let heights = self.heights;
        let prefix = Rc::new(prefix_sums(len, |i| {
            heights.as_ref().map(|f| f(i)).unwrap_or(1)
        }));
        let selection = self.selection.unwrap_or_else(|| cx.signal(0usize));
        let offset = self.offset_y.unwrap_or_else(|| cx.signal(0i32));
        let model = SelectionModel {
            len,
            keys: Some(keys),
            prefix,
            selection,
            selection_key: self.selection_key,
            offset,
        };
        let total_rows = model.total_rows();
        // Same two settles `List` does at build, for the same reasons:
        // a selection naming a vanished row, and a bound offset pointing
        // past a list that shrank under it.
        model.settle();
        if self.offset_y.is_some() {
            offset.update(|o| *o = (*o).clamp(0, (total_rows - 1).max(0)));
        }

        let on_select: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_select));
        let on_activate: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_activate));

        let select = {
            let model = model.clone();
            let on_select = on_select.clone();
            move |target: usize, view_h: i32| {
                if model.select(target, view_h) {
                    // Held borrow across `f`: safe — dispatch-only slot
                    // (the SharedCallback held-borrow contract).
                    if let Some(f) = on_select.borrow_mut().as_mut() {
                        f(target);
                    }
                }
            }
        };

        // Solved viewport size, published one turn after a resize by the
        // shared probe (paint never writes signals). Only `scroll_to`
        // reads it; event handlers measure from their own rect.
        let view_box = cx.signal((0i32, 0i32));
        if let Some(request) = self.scroll_to {
            let model = model.clone();
            cx.effect_labeled("row-select-scroll-to", move || {
                let Some(idx) = request.get() else {
                    return;
                };
                if model.len == 0 {
                    request.set(None);
                    return;
                }
                let vh = view_box.get().1;
                if vh <= 0 {
                    return; // hold the request until the probe measures
                }
                model.ensure_visible(idx.min(model.len - 1), vh);
                request.set(None); // consumed (one extra no-op run)
            });
        }

        // CAPTURE: the wrapped `Scroll` scrolls on the same arrows, and a
        // bubble handler would never see them. Modified chords and every
        // other key are left alone — a capture handler that swallowed
        // more than it owns is how a subtree goes deaf.
        let keys_model = model.clone();
        let key_activate = on_activate.clone();
        let key_select = select.clone();
        let key_handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let UiEvent::Key(k) = ev else { return };
            if len == 0 || k.mods != Mods::NONE {
                return;
            }
            if matches!(k.key, Key::Enter | Key::Char(' ')) {
                // Consumed ONLY when bound, so an unwired RowSelect
                // leaves Enter/Space to the app's shortcuts.
                if let Some(f) = key_activate.borrow_mut().as_mut() {
                    f(keys_model.selection.get_untracked().min(len - 1));
                    ctx.stop_propagation();
                }
                return;
            }
            let h = ctx.current_rect().h.max(1);
            let cur = keys_model.selection.get_untracked();
            let Some(target) = nav_target(k.key, cur, len, (h as usize).max(1)) else {
                return;
            };
            key_select(target, h);
            ctx.stop_propagation();
        };

        // BUBBLE: a press inside a row belongs to whatever the row put
        // there first (a button, a link). Only an unclaimed press
        // selects.
        let mouse_model = model.clone();
        let mouse_activate = on_activate;
        let mouse_handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let UiEvent::Mouse(m) = ev else { return };
            if !matches!(m.kind, MouseKind::Down(MouseButton::Left)) {
                return;
            }
            let rect = ctx.current_rect();
            let row = m.pos.y - rect.y + mouse_model.offset.get_untracked();
            let Some(idx) = mouse_model.row_at(row) else {
                return;
            };
            let was_selected = mouse_model.selection.get_untracked() == idx;
            select(idx, rect.h.max(1));
            if was_selected {
                if let Some(f) = mouse_activate.borrow_mut().as_mut() {
                    f(idx);
                }
            }
            ctx.stop_propagation();
        };

        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));
        let mut el = Element::new()
            .style(layout)
            .role(crate::ui::Role::List)
            .access_value(move || {
                if len == 0 {
                    return "0 items".into();
                }
                format!("{} items, selected {}", len, selection.get_untracked() + 1)
            })
            .draw(super::scroll::size_probe(cx, view_box));
        if self.focusable {
            el = el.focusable();
        }
        if let Some(focused) = self.focused {
            el = el.focus_signal(focused);
        }
        el.on(Phase::Capture, key_handler)
            .on(Phase::Bubble, mouse_handler)
            .child(
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .child(content)
                    .build(),
            )
    }
}

#[cfg(test)]
#[path = "row_select_tests.rs"]
mod tests;

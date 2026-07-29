//! List: virtualized, selectable, keyboard+mouse vertical list.
//!
//! SELECTION vs ACTIVATION (0250 ruling, recorded in
//! reviews/study/platform-on-appkits.md §"The 0250 ruling"): selection
//! FOLLOWS MOVEMENT — arrows/Home/End/Page keys/click move it and
//! `on_select` is the selection-changed NOTIFICATION; activation is the
//! EXPLICIT "user chose this row" event — `on_activate` fires on Enter
//! (always), on Space (List has no toggle meaning, so Space aliases
//! Enter here), and on a click on the ALREADY-selected row. Never wire
//! commitment, navigation, or destruction to `on_select`.
//!
//! Double-click (app-kits 0535): by default List's click-on-selected
//! rule SUBSUMES it — click 1 selects, click 2 on the already-selected
//! row activates via [`List::on_activate`], timing-free (the picker
//! gesture, deliberately broader than a timed double-click). For
//! browsing surfaces that need strict SGR double-click (open-on-double,
//! slow re-click only re-selects), bind [`List::on_row_double_click`]
//! instead — it fires only when `EventCtx::click_count() >= 2` on the
//! row body and takes precedence over `on_activate` for that press.
//! `Table` uses the same timed convention.
//!
//! Row accessories (field-agora 0810): optional trailing column via
//! [`List::row_accessory`] + [`List::on_accessory_click`]. The engine
//! owns body/accessory/scrollbar column widths — no app-side X math.
//! Accessory clicks do not change selection. Rich labels ride
//! [`List::rich_items`] (styled spans on the body column only).
//!
//! Disposal-safety law (ruling clause 4): the List completes ALL of its
//! own bookkeeping (selection write, sticky-key write, ensure-visible
//! scrolling) BEFORE any user callback runs, so a callback may dispose
//! the List's scope synchronously (the modal-picker close) without
//! tripping over dead signals.
//!
//! Cycle-7 hardening: VARIABLE-HEIGHT items (per-item height callback,
//! prefix-sum windowing — offsets are CONTENT CELL ROWS, item lookup is
//! a binary search), STICKY SELECTION BY KEY (`key_fn` +
//! `selection_key`: rebuilds re-find the selected key's new index, so
//! data mutations keep the same LOGICAL item selected), and
//! `scroll_to` (a command signal: set `Some(index)`, the list scrolls
//! it into view and consumes the request).
//!
//! Variable-height v1 honesty: an item's extra rows reserve SPACE (for
//! spacing/grouping); the label renders on the item's first row only —
//! wrapped multi-row item CONTENT is a later decision.
//!
//! ```
//! use abstracttui::base::Size;
//! use abstracttui::reactive::create_root;
//! use abstracttui::ui::{BufferCanvas, Element, UiTree};
//! use abstracttui::widgets::List;
//!
//! let mut tree = UiTree::new(Size::new(12, 3));
//! let (root, ()) = create_root(|cx| {
//!     let sel_key = cx.signal(String::from("beta"));
//!     let view = Element::new()
//!         .child(
//!             List::of(["alpha", "beta", "gamma"])
//!                 .key_fn(|_, item| item.to_string())
//!                 .selection_key(sel_key) // sticky across data changes
//!                 .view(cx),
//!         )
//!         .build();
//!     tree.mount(cx, view);
//! });
//! let mut canvas = BufferCanvas::new(Size::new(12, 3));
//! tree.draw(&mut canvas);
//! assert!(canvas.row_text(1).contains("beta"));
//! root.dispose();
//! ```
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::rich::RichText;
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent};
use crate::widgets::richtext::draw_rich_lines;

type HeightFn = Box<dyn Fn(usize, &str) -> i32>;
type KeyFn = Box<dyn Fn(usize, &str) -> String>;
type AccessoryFn = Box<dyn Fn(usize, &str) -> Option<String>>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ListHitZone {
    Body,
    Accessory,
    Scrollbar,
}

/// Column widths for one list viewport (content coordinates).
struct ListColumns {
    body_w: i32,
    accessory_w: i32,
    bar_w: i32,
}

fn list_columns(viewport_w: i32, show_bar: bool, accessory_w: i32) -> ListColumns {
    let bar_w = i32::from(show_bar);
    let accessory_w = accessory_w.max(0);
    let body_w = (viewport_w - bar_w - accessory_w).max(0);
    ListColumns {
        body_w,
        accessory_w,
        bar_w,
    }
}

fn list_hit_zone(local_x: i32, cols: ListColumns) -> ListHitZone {
    if cols.bar_w > 0 && local_x >= cols.body_w + cols.accessory_w {
        ListHitZone::Scrollbar
    } else if cols.accessory_w > 0 && local_x >= cols.body_w {
        ListHitZone::Accessory
    } else {
        ListHitZone::Body
    }
}

fn accessory_column_width(items: &[String], accessory: &AccessoryFn) -> i32 {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| accessory(i, s))
        .map(|label| unicode_width::UnicodeWidthStr::width(label.as_str()) as i32 + 1)
        .max()
        .unwrap_or(0)
}

/// A virtualized, selectable vertical list — the picker surface.
///
/// Bind [`selection`](List::selection) to a `Signal<usize>`; selection
/// follows movement ([`on_select`](List::on_select) is the
/// notification), and [`on_activate`](List::on_activate) is the commit:
/// Enter, Space, or a click on the already-selected row (which is why a
/// double-click activates with no timer). The canonical build is
/// `.view(cx)`. See the [module docs](crate::widgets::list) for the
/// full selection-vs-activation contract.
pub struct List {
    items: Vec<String>,
    selection: Option<Signal<usize>>,
    selection_key: Option<Signal<String>>,
    key_fn: Option<KeyFn>,
    heights: Option<HeightFn>,
    scroll_to: Option<Signal<Option<usize>>>,
    focused: Option<Signal<bool>>,
    layout: Option<LayoutStyle>,
    on_select: Option<Box<dyn FnMut(usize)>>,
    on_activate: Option<Box<dyn FnMut(usize)>>,
    accessory_fn: Option<AccessoryFn>,
    accessory_width: Option<i32>,
    on_accessory_click: Option<Box<dyn FnMut(usize)>>,
    on_row_double_click: Option<Box<dyn FnMut(usize)>>,
    rich_items: Option<Vec<RichText>>,
}

impl List {
    /// Ergonomic constructor: anything iterable into strings —
    /// `List::of(["a", "b"])`, an iterator chain, string slices.
    /// (`new` keeps the plain `Vec<String>` signature so existing
    /// `.collect()` call sites stay inferable.)
    pub fn of<I, S>(items: I) -> List
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        List::new(items.into_iter().map(Into::into).collect())
    }

    pub fn new(items: Vec<String>) -> List {
        List {
            items,
            selection: None,
            selection_key: None,
            key_fn: None,
            heights: None,
            scroll_to: None,
            focused: None,
            layout: None,
            on_select: None,
            on_activate: None,
            accessory_fn: None,
            accessory_width: None,
            on_accessory_click: None,
            on_row_double_click: None,
            rich_items: None,
        }
    }

    /// Bind an external selection signal (index); default is internal.
    pub fn selection(mut self, selection: Signal<usize>) -> List {
        self.selection = Some(selection);
        self
    }

    /// Stable item identity for sticky selection: with `selection_key`
    /// bound, rebuilds re-find the key's CURRENT index (data mutations
    /// keep the logical item selected) and selecting writes the key.
    pub fn key_fn(mut self, f: impl Fn(usize, &str) -> String + 'static) -> List {
        self.key_fn = Some(Box::new(f));
        self
    }

    /// The selected item's KEY (see [`List::key_fn`]).
    pub fn selection_key(mut self, key: Signal<String>) -> List {
        self.selection_key = Some(key);
        self
    }

    /// Per-item height in cell rows (min 1). Enables variable-height
    /// virtualization; without it every item is one row.
    pub fn item_heights(mut self, f: impl Fn(usize, &str) -> i32 + 'static) -> List {
        self.heights = Some(Box::new(f));
        self
    }

    /// Command signal: set `Some(index)` to scroll that item into view;
    /// the list consumes the request (resets to `None`).
    pub fn scroll_to(mut self, request: Signal<Option<usize>>) -> List {
        self.scroll_to = Some(request);
        self
    }

    /// Bind an external focus signal (D4-2): true while the list holds
    /// keyboard focus — panes wire their stroke color to it (§3.2).
    pub fn focus_signal(mut self, focused: Signal<bool>) -> List {
        self.focused = Some(focused);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> List {
        self.layout = Some(layout);
        self
    }

    /// Selection-changed NOTIFICATION: fires whenever the highlighted
    /// index MOVES (arrows, Page keys, Home/End, a click on a different
    /// row). It is not a commitment — for "the user chose this row",
    /// bind [`List::on_activate`]. All List bookkeeping (selection
    /// write, ensure-visible) completes before this runs, so the
    /// callback may dispose the List's scope synchronously.
    pub fn on_select(mut self, f: impl FnMut(usize) + 'static) -> List {
        self.on_select = Some(Box::new(f));
        self
    }

    /// ACTIVATION: the user committed the selected row (0250 ruling).
    /// Fires with the current index on Enter, on Space (no toggle
    /// meaning in a List), and on a click on the ALREADY-selected row —
    /// a click on an unselected row only selects. Double-clicks work by
    /// subsumption when [`List::on_row_double_click`] is unbound: click 1
    /// selects, click 2 is a click on the selected row (no timing
    /// requirement — the picker gesture). When `on_row_double_click` IS
    /// bound, a timed double-click (`click_count() >= 2`) fires that
    /// callback instead of this one for the second press. When unbound,
    /// Enter/Space pass through to app shortcuts exactly as before this
    /// event existed. The callback may dispose the List's scope
    /// synchronously (close-the-picker is the intended use).
    pub fn on_activate(mut self, f: impl FnMut(usize) + 'static) -> List {
        self.on_activate = Some(Box::new(f));
        self
    }

    /// Trailing column label per row (`None` = no accessory cell).
    /// Width is the max label width unless [`List::accessory_width`]
    /// pins it. Clicks in the accessory column route to
    /// [`List::on_accessory_click`] and do not move selection.
    pub fn row_accessory(mut self, f: impl Fn(usize, &str) -> Option<String> + 'static) -> List {
        self.accessory_fn = Some(Box::new(f));
        self
    }

    /// Fixed accessory column width in cells (default: max label width).
    pub fn accessory_width(mut self, cells: i32) -> List {
        self.accessory_width = Some(cells.max(1));
        self
    }

    /// Fires when the user clicks the trailing accessory column.
    pub fn on_accessory_click(mut self, f: impl FnMut(usize) + 'static) -> List {
        self.on_accessory_click = Some(Box::new(f));
        self
    }

    /// Browsing-surface double-click: fires on the row BODY when the
    /// second press of a click chain lands on the already-selected row
    /// (`EventCtx::click_count() >= 2`). Supersedes [`List::on_activate`]
    /// for that press. Accessory and scrollbar columns are excluded.
    pub fn on_row_double_click(mut self, f: impl FnMut(usize) + 'static) -> List {
        self.on_row_double_click = Some(Box::new(f));
        self
    }

    /// Per-row rich labels (same length as `items`). Body column only;
    /// accessories stay plain text. Replaces the plain string on the
    /// first visible row of each item.
    pub fn rich_items(mut self, items: Vec<RichText>) -> List {
        self.rich_items = Some(items);
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
        let text_fg = t.text;
        let ground = t.surface;
        let sel_bg = t.selection_bg;
        let sel_fg = t.selection_fg;
        let track = t.border;
        let thumb = t.text_muted;

        let items = Rc::new(self.items);
        let len = items.len();
        // Prefix sums over item heights: prefix[i] = first content row
        // of item i; prefix[len] = total rows. Uniform lists get the
        // identity prefix — ONE windowing code path.
        let prefix: Rc<Vec<i32>> = Rc::new({
            let mut out = Vec::with_capacity(len + 1);
            let mut acc = 0i32;
            out.push(0);
            for (i, item) in items.iter().enumerate() {
                let h = self
                    .heights
                    .as_ref()
                    .map(|f| f(i, item).max(1))
                    .unwrap_or(1);
                acc += h;
                out.push(acc);
            }
            out
        });
        let total_rows = *prefix.last().unwrap_or(&0);

        let selection = self.selection.unwrap_or_else(|| cx.signal(0usize));
        // Sticky selection: the KEY re-finds its index at build time —
        // this is what survives data mutations (each mutation rebuilds
        // through the caller's Dyn).
        let keys: Option<Rc<Vec<String>>> = self.key_fn.map(|f| {
            Rc::new(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, s)| f(i, s))
                    .collect::<Vec<_>>(),
            )
        });
        if let (Some(key_sig), Some(keys)) = (self.selection_key, keys.as_ref()) {
            let wanted = key_sig.get_untracked();
            if let Some(idx) = keys.iter().position(|k| *k == wanted) {
                if selection.get_untracked() != idx {
                    selection.set(idx);
                }
            }
        }
        let selection_key = self.selection_key;
        let keys_for_select = keys.clone();

        let offset = cx.signal(0i32); // first visible CONTENT ROW
        let on_select: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_select));
        let on_activate: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_activate));
        let on_accessory_click: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_accessory_click));
        let on_row_double_click: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_row_double_click));
        let accessory_fn = self.accessory_fn.map(Rc::new);
        let accessory_w = accessory_fn.as_ref().map_or(0, |f| {
            self.accessory_width
                .unwrap_or_else(|| accessory_column_width(&items, f))
        });
        let rich_items = self.rich_items.map(Rc::new);
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));

        let prefix_for_select = prefix.clone();
        let select = {
            let on_select = on_select.clone();
            move |target: usize, view_h: i32| {
                if len == 0 {
                    return; // nothing to select (prefix has no item span)
                }
                let target = target.min(len - 1);
                let changed = selection.get_untracked() != target;
                if changed {
                    selection.set(target);
                    if let (Some(key_sig), Some(keys)) = (selection_key, keys_for_select.as_ref()) {
                        if let Some(k) = keys.get(target) {
                            key_sig.set(k.clone());
                        }
                    }
                }
                // ensure-visible on CONTENT ROWS (variable heights).
                // ALL widget bookkeeping lands BEFORE the user callback
                // (0250 ruling clause 4, disposal-safety law): a
                // callback that disposes this List's scope must find no
                // widget code left to run on dead signals.
                let top = prefix_for_select[target];
                let bottom = prefix_for_select[target + 1];
                offset.update(|o| {
                    if top < *o {
                        *o = top;
                    }
                    if view_h > 0 && bottom > *o + view_h {
                        *o = bottom - view_h;
                    }
                    *o = (*o).clamp(0, (total_rows - view_h.max(1)).max(0));
                });
                if changed {
                    // Held borrow across `f`: safe — dispatch-only slot
                    // (the SharedCallback held-borrow contract).
                    if let Some(f) = on_select.borrow_mut().as_mut() {
                        f(target);
                    }
                }
            }
        };

        // scroll_to command signal: consume Some(idx) into an offset.
        if let Some(request) = self.scroll_to {
            let prefix_for_scroll = prefix.clone();
            cx.effect_labeled("list-scroll-to", move || {
                if let Some(idx) = request.get() {
                    let idx = idx.min(len.saturating_sub(1));
                    let top = prefix_for_scroll[idx];
                    offset.update(|o| {
                        *o = top.clamp(0, (total_rows - 1).max(0));
                    });
                    request.set(None); // consumed (one extra no-op run)
                }
            });
        }

        let prefix_for_handler = prefix.clone();
        let activate = on_activate;
        let accessory_click = on_accessory_click;
        let row_double_click = on_row_double_click;
        let accessory_w_handler = accessory_w;
        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let rect = ctx.current_rect();
            let h = rect.h.max(1);
            match ev {
                UiEvent::Key(k) => {
                    // Activation keys (0250 ruling clause 2): Enter
                    // always; Space too, because a List has no toggle
                    // meaning. Consumed ONLY when a callback is bound —
                    // an unbound List leaves Enter/Space to the app's
                    // own shortcuts (pre-0250 behavior, kept).
                    if matches!(k.key, Key::Enter | Key::Char(' ')) {
                        if len > 0 {
                            // Held borrow: safe — dispatch-only slot (the
                            // SharedCallback held-borrow contract).
                            if let Some(f) = activate.borrow_mut().as_mut() {
                                f(selection.get_untracked().min(len - 1));
                                ctx.stop_propagation();
                            }
                        }
                        return;
                    }
                    let cur = selection.get_untracked();
                    let page = (h as usize).max(1);
                    let target = match k.key {
                        Key::Up => cur.saturating_sub(1),
                        Key::Down => cur + 1,
                        Key::PageUp => cur.saturating_sub(page),
                        Key::PageDown => cur + page,
                        Key::Home => 0,
                        Key::End => len.saturating_sub(1),
                        _ => return,
                    };
                    select(target, h);
                    ctx.stop_propagation();
                }
                UiEvent::Mouse(m) => match m.kind {
                    MouseKind::ScrollUp | MouseKind::ScrollDown => {
                        let delta = if m.kind == MouseKind::ScrollUp { -3 } else { 3 };
                        offset.update(|o| {
                            *o = (*o + delta).clamp(0, (total_rows - h).max(0));
                        });
                        ctx.stop_propagation();
                    }
                    MouseKind::Down(MouseButton::Left) => {
                        let local_x = m.pos.x - rect.x;
                        let show_bar = total_rows > h;
                        let cols = list_columns(rect.w, show_bar, accessory_w_handler);
                        match list_hit_zone(local_x, cols) {
                            ListHitZone::Scrollbar => {
                                ctx.stop_propagation();
                                return;
                            }
                            ListHitZone::Accessory => {
                                let row = (m.pos.y - rect.y) + offset.get_untracked();
                                if row >= 0 && row < total_rows {
                                    let idx = prefix_for_handler
                                        .partition_point(|&p| p <= row)
                                        .saturating_sub(1);
                                    if idx < len {
                                        if let Some(f) = accessory_click.borrow_mut().as_mut() {
                                            f(idx);
                                        }
                                    }
                                }
                                ctx.stop_propagation();
                                return;
                            }
                            ListHitZone::Body => {}
                        }
                        // Content row -> item index (binary search on
                        // the prefix; the row belongs to the item whose
                        // span contains it).
                        let row = (m.pos.y - rect.y) + offset.get_untracked();
                        if row >= 0 && row < total_rows {
                            let idx = prefix_for_handler
                                .partition_point(|&p| p <= row)
                                .saturating_sub(1);
                            if idx < len {
                                let was_selected = selection.get_untracked() == idx;
                                select(idx, h);
                                if was_selected {
                                    if ctx.click_count() >= 2 {
                                        if let Some(f) = row_double_click.borrow_mut().as_mut() {
                                            f(idx);
                                        } else if let Some(f) = activate.borrow_mut().as_mut() {
                                            f(idx);
                                        }
                                    } else if let Some(f) = activate.borrow_mut().as_mut() {
                                        f(idx);
                                    }
                                }
                            }
                        }
                        ctx.stop_propagation();
                    }
                    _ => {}
                },
                _ => {}
            }
        };

        let mut el = Element::new()
            .style(layout)
            .role(crate::ui::Role::List)
            .access_value(move || {
                format!("{} items, selected {}", len, selection.get_untracked() + 1)
            })
            .focusable();
        if let Some(focused) = self.focused {
            el = el.focus_signal(focused);
        }
        let prefix_for_draw = prefix;
        let accessory_fn_draw = accessory_fn;
        let accessory_w_draw = accessory_w;
        let rich_items_draw = rich_items;
        el.on(Phase::Bubble, handler).child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let sel = selection.get();
                let first_row = offset.get().max(0);
                let items = items.clone();
                let prefix = prefix_for_draw.clone();
                let accessory_fn_inner = accessory_fn_draw.clone();
                let rich_items_inner = rich_items_draw.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .draw(move |canvas, rect| {
                        if rect.is_empty() || items.is_empty() {
                            return;
                        }
                        let base = Style::new().fg(text_fg).bg(ground);
                        canvas.fill_styled(rect, ' ', &base);
                        let total = *prefix.last().unwrap_or(&0);
                        let show_bar = total > rect.h;
                        let cols = list_columns(rect.w, show_bar, accessory_w_draw);
                        let text_w = cols.body_w;
                        // Virtualization: first visible item by
                        // binary search, walk until off-screen.
                        let mut idx = prefix
                            .partition_point(|&p| p <= first_row)
                            .saturating_sub(1);
                        while idx < items.len() {
                            let top = prefix[idx] - first_row;
                            if top >= rect.h {
                                break;
                            }
                            let item_h = prefix[idx + 1] - prefix[idx];
                            let selected = idx == sel;
                            let style = if selected {
                                Style::new().fg(sel_fg).bg(sel_bg)
                            } else {
                                base
                            };
                            if selected {
                                // Body + accessory wear the selection pair.
                                let row_w = cols.body_w + cols.accessory_w;
                                for r in 0..item_h {
                                    let y = rect.y + top + r;
                                    if y >= rect.y && y < rect.bottom() {
                                        canvas.fill_styled(
                                            crate::base::Rect::new(rect.x, y, row_w, 1),
                                            ' ',
                                            &style,
                                        );
                                    }
                                }
                            }
                            let y = rect.y + top;
                            if y >= rect.y && y < rect.bottom() {
                                if let Some(rich) =
                                    rich_items_inner.as_ref().and_then(|v| v.get(idx))
                                {
                                    let shaped = rich.wrap(text_w.max(0));
                                    draw_rich_lines(
                                        canvas,
                                        crate::base::Rect::new(rect.x, y, text_w, 1),
                                        shaped.lines.iter().take(1),
                                        if selected { sel_fg } else { text_fg },
                                        crate::render::rich::HAlign::Left,
                                    );
                                } else {
                                    let line =
                                        crate::text::truncate_ellipsis(&items[idx], text_w.max(0));
                                    canvas.print_styled(
                                        crate::base::Point::new(rect.x, y),
                                        &line,
                                        &style,
                                    );
                                }
                            }
                            if cols.accessory_w > 0 {
                                if let Some(label) = accessory_fn_inner
                                    .as_ref()
                                    .and_then(|f| f(idx, &items[idx]))
                                {
                                    let acc = crate::text::truncate_ellipsis(
                                        &label,
                                        cols.accessory_w.max(0),
                                    );
                                    let acc_w =
                                        unicode_width::UnicodeWidthStr::width(acc.as_str()) as i32;
                                    let acc_x =
                                        rect.x + cols.body_w + (cols.accessory_w - acc_w).max(0);
                                    if y >= rect.y && y < rect.bottom() {
                                        canvas.print_styled(
                                            crate::base::Point::new(acc_x, y),
                                            &acc,
                                            &style,
                                        );
                                    }
                                }
                            }
                            idx += 1;
                        }
                        if show_bar {
                            draw_scrollbar(canvas, rect, first_row, total, track, thumb, ground);
                        }
                    })
                    .build()
            },
        ))
    }
}

/// Token-styled vertical scrollbar in the rightmost column. `first` and
/// `total` are content ROWS (shared by List/Table; Table passes item
/// counts, which are rows there).
pub(crate) fn draw_scrollbar(
    canvas: &mut dyn crate::ui::StyledCanvas,
    rect: crate::base::Rect,
    first: i32,
    total: i32,
    track: crate::base::Rgba,
    thumb: crate::base::Rgba,
    ground: crate::base::Rgba,
) {
    let x = rect.right() - 1;
    let h = rect.h.max(1);
    let track_style = Style::new().fg(track).bg(ground);
    for y in rect.y..rect.bottom() {
        canvas.print_styled(crate::base::Point::new(x, y), "│", &track_style);
    }
    let thumb_h = ((h * h) / total.max(1)).clamp(1, h);
    let denom = (total - h).max(1);
    let thumb_y = rect.y + ((first.min(denom) * (h - thumb_h)) / denom).max(0);
    let thumb_style = Style::new().fg(thumb).bg(ground);
    for y in thumb_y..(thumb_y + thumb_h).min(rect.bottom()) {
        canvas.print_styled(crate::base::Point::new(x, y), "┃", &thumb_style);
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;

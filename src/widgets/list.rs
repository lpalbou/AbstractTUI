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

use crate::base::Point;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::rich::RichText;
use crate::render::{Attrs, Style};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent};
use crate::widgets::richtext::draw_rich_lines;

type HeightFn = Box<dyn Fn(usize, &str) -> i32>;
type KeyFn = Box<dyn Fn(usize, &str) -> String>;
type AccessoryFn = Box<dyn Fn(usize, &str) -> Option<String>>;

/// The dismiss glyph [`List::on_remove`] draws — `✕` U+2715, the same
/// spelling the block close affordance uses. East-Asian-NARROW and
/// absent from emoji-data, so it is single-width under every terminal
/// convention; `×` U+00D7 is East-Asian-AMBIGUOUS and was rejected (the
/// 0595 glyph-research method). Width 1 is test-pinned.
const REMOVE_GLYPH: &str = "✕";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ListHitZone {
    Body,
    Accessory,
    Scrollbar,
}

/// Column widths for one list viewport (content coordinates).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ListColumns {
    body_w: i32,
    accessory_w: i32,
    bar_w: i32,
}

/// Where a pointer landed: an item row plus its zone, or the scrollbar
/// strip (which belongs to no row).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ListHit {
    Row(usize, ListHitZone),
    Scrollbar,
}

/// Accessory labels, resolved ONCE per build: the caller's closure runs
/// exactly once per item, and the text is pre-truncated and pre-measured
/// against the (build-fixed) accessory width. Hit-testing and painting
/// then borrow — neither allocates, so hover motion is free.
type AccessoryCells = Rc<Vec<Option<(String, i32)>>>;

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

/// The ONE hit resolver: hover ink and clicks both route through it, so
/// what lights up under the pointer is exactly what a press will act on.
///
/// The accessory zone is the whole column width on the row where the
/// accessory is DRAWN (an item's first row) — a forgiving target — and
/// only for rows that actually have one. Everything else is body.
fn resolve_hit(
    local: Point,
    cols: ListColumns,
    offset: i32,
    prefix: &[i32],
    len: usize,
    cells: Option<&AccessoryCells>,
) -> Option<ListHit> {
    let zone = list_hit_zone(local.x, cols);
    if zone == ListHitZone::Scrollbar {
        return Some(ListHit::Scrollbar);
    }
    let row = local.y + offset;
    let total_rows = *prefix.last()?;
    if row < 0 || row >= total_rows {
        return None;
    }
    let idx = prefix.partition_point(|&p| p <= row).saturating_sub(1);
    if idx >= len {
        return None;
    }
    let on_accessory = zone == ListHitZone::Accessory
        && row == prefix[idx]
        && cells.is_some_and(|c| c[idx].is_some());
    Some(ListHit::Row(
        idx,
        if on_accessory {
            ListHitZone::Accessory
        } else {
            ListHitZone::Body
        },
    ))
}

/// Scroll `offset` the least amount that brings item `idx` fully into a
/// `view_h`-row viewport. Shared by keyboard/click selection and the
/// `scroll_to` command so the two can never drift apart.
fn ensure_visible(offset: Signal<i32>, prefix: &[i32], idx: usize, view_h: i32, total_rows: i32) {
    let top = prefix[idx];
    let bottom = prefix[idx + 1];
    offset.update(|o| {
        if top < *o {
            *o = top;
        }
        if view_h > 0 && bottom > *o + view_h {
            *o = bottom - view_h;
        }
        *o = (*o).clamp(0, (total_rows - view_h.max(1)).max(0));
    });
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
    offset_y: Option<Signal<i32>>,
    focused: Option<Signal<bool>>,
    layout: Option<LayoutStyle>,
    on_select: Option<Box<dyn FnMut(usize)>>,
    on_activate: Option<Box<dyn FnMut(usize)>>,
    accessory_fn: Option<AccessoryFn>,
    accessory_width: Option<i32>,
    on_accessory_click: Option<Box<dyn FnMut(usize)>>,
    on_remove: Option<Box<dyn FnMut(usize)>>,
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
            offset_y: None,
            focused: None,
            layout: None,
            on_select: None,
            on_activate: None,
            accessory_fn: None,
            accessory_width: None,
            on_accessory_click: None,
            on_remove: None,
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

    /// Bind the first visible CONTENT ROW (the [`Scroll::offset_y`]
    /// convention). Default is internal, which means a rebuild starts
    /// back at the top — bind this whenever the caller's data changes
    /// under a `Dyn`, so dismissing a row with [`List::on_remove`] does
    /// not scroll the reader to the head of the list.
    ///
    /// The List clamps a bound offset into range on every build, so a
    /// list that shrinks can never strand the viewport past its end.
    ///
    /// [`Scroll::offset_y`]: crate::widgets::Scroll::offset_y
    pub fn offset_y(mut self, offset: Signal<i32>) -> List {
        self.offset_y = Some(offset);
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

    /// Removable rows: draws a trailing `✕` on every row and fires `f`
    /// with the row index when it is clicked. Remove that index from
    /// YOUR data and let your `Dyn` rebuild — the List owns the rest.
    ///
    /// On the rebuild the List re-settles selection so it can never name
    /// a row that no longer exists: with [`List::key_fn`] +
    /// [`List::selection_key`] bound the selected item is re-found by
    /// key, and when the selected item was the one removed (its key is
    /// gone) selection falls to the same slot clamped into the shorter
    /// list — the next row down, or the new last row.
    ///
    /// This is the one-call form of [`List::row_accessory`] +
    /// [`List::accessory_width`] + [`List::on_accessory_click`]. Bind
    /// those directly instead when the trailing column is a badge or a
    /// per-row action rather than a dismiss.
    pub fn on_remove(mut self, f: impl FnMut(usize) + 'static) -> List {
        self.on_remove = Some(Box::new(f));
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
        let accent = t.accent;

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
        let hover = cx.signal(None::<ListHit>);
        // Solved viewport size, published by the root's `size_probe` one
        // turn after a resize (RT1-2: paint never writes signals — the
        // probe latches an `after(0)` instead). The `scroll_to` command
        // is the only reader; event handlers measure from their own
        // `ctx.current_rect()`, which is already authoritative.
        let view_box = cx.signal((0i32, 0i32));
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
        // Settle selection against THIS build's items. Rows can vanish
        // between builds (a dismiss ✕, a filter, a server push), and a
        // selection naming a row that no longer exists is a real defect:
        // nothing highlights, `access_value` announces a phantom row to
        // a screen reader, and the first arrow key moves the wrong way.
        // So the index is always re-derived and always in range.
        if len > 0 {
            let by_key = self
                .selection_key
                .zip(keys.as_ref())
                .and_then(|(sig, keys)| {
                    let wanted = sig.get_untracked();
                    keys.iter().position(|k| *k == wanted)
                });
            // The key is gone (or there is none): hold the SLOT, clamped.
            // Removing a row leaves the next one selected, and removing
            // the last leaves the new last — the expected list behavior.
            let idx = by_key.unwrap_or_else(|| selection.get_untracked().min(len - 1));
            selection.set_if_changed(idx);
            if let (Some(sig), Some(keys)) = (self.selection_key, keys.as_ref()) {
                sig.set_if_changed(keys[idx].clone());
            }
        }
        let selection_key = self.selection_key;
        let keys_for_select = keys.clone();

        // First visible CONTENT ROW. A bound offset survives rebuilds
        // (see `offset_y`) — but the items it points into may have
        // shrunk since it was written, so clamp it here for the same
        // reason selection is settled above: no viewport past the end.
        let offset = self.offset_y.unwrap_or_else(|| cx.signal(0i32));
        if self.offset_y.is_some() {
            offset.update(|o| *o = (*o).clamp(0, (total_rows - 1).max(0)));
        }
        let on_select: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_select));
        let on_activate: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_activate));
        let on_row_double_click: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_row_double_click));

        // `on_remove` is `row_accessory` + `accessory_width` +
        // `on_accessory_click` with the dismiss glyph filled in; an
        // explicit `row_accessory` still wins, so a caller can keep a
        // custom label and still get the removal bookkeeping.
        let removable = self.on_remove.is_some();
        // A dismiss is consequence-bearing, so its hot ink is `error` —
        // the same ruling the Block close affordance follows. A generic
        // `row_accessory` is a badge or a neutral per-row action, and an
        // unread count painted red would be a lie, so it takes the
        // ordinary hover accent. The CONSEQUENCE decides, not the label.
        let acc_hot_ink = if removable { t.error } else { t.accent };
        let accessory_fn: Option<AccessoryFn> = self.accessory_fn.or_else(|| {
            removable.then(|| {
                Box::new(|_: usize, _: &str| Some(REMOVE_GLYPH.to_string())) as AccessoryFn
            })
        });
        let on_accessory_click: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_accessory_click.or(self.on_remove)));

        // ONE pass: the caller's closure runs exactly once per item and
        // the label is truncated and measured against the accessory
        // width, which is fixed for this build. Painting and hit-testing
        // then borrow — no allocation on the hover or click path.
        let accessory_w = accessory_fn.as_ref().map_or(0, |f| {
            self.accessory_width.unwrap_or_else(|| {
                items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| f(i, s))
                    .map(|l| unicode_width::UnicodeWidthStr::width(l.as_str()) as i32 + 1)
                    .max()
                    .unwrap_or(0)
            })
        });
        let accessory_cells: Option<AccessoryCells> = accessory_fn.map(|f| {
            Rc::new(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        f(i, s).map(|label| {
                            let text = crate::text::truncate_ellipsis(&label, accessory_w.max(0));
                            let w = unicode_width::UnicodeWidthStr::width(text.as_str()) as i32;
                            (text, w)
                        })
                    })
                    .collect::<Vec<_>>(),
            )
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
                ensure_visible(offset, &prefix_for_select, target, view_h, total_rows);
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
                let Some(idx) = request.get() else {
                    return;
                };
                if len == 0 {
                    request.set(None);
                    return;
                }
                let vh = view_box.get().1;
                if vh <= 0 {
                    return; // hold the request until the probe measures
                }
                let idx = idx.min(len - 1);
                ensure_visible(offset, &prefix_for_scroll, idx, vh, total_rows);
                request.set(None); // consumed (one extra no-op run)
            });
        }

        let prefix_for_handler = prefix.clone();
        let activate = on_activate;
        let accessory_click = on_accessory_click;
        let row_double_click = on_row_double_click;
        let accessory_w_handler = accessory_w;
        let cells_handler = accessory_cells.clone();
        let hover_handler = hover;
        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let rect = ctx.current_rect();
            let h = rect.h.max(1);
            let hit_at = |pos: Point| {
                let cols = list_columns(rect.w, total_rows > h, accessory_w_handler);
                resolve_hit(
                    Point::new(pos.x - rect.x, pos.y - rect.y),
                    cols,
                    offset.get_untracked(),
                    &prefix_for_handler,
                    len,
                    cells_handler.as_ref(),
                )
            };
            match ev {
                UiEvent::MouseLeave => {
                    hover_handler.set_if_changed(None);
                }
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
                    MouseKind::Move => {
                        hover_handler.set_if_changed(hit_at(m.pos));
                    }
                    MouseKind::ScrollUp | MouseKind::ScrollDown => {
                        let delta = if m.kind == MouseKind::ScrollUp { -3 } else { 3 };
                        let max_off = (total_rows - h).max(0);
                        let to = (offset.get_untracked() + delta).clamp(0, max_off);
                        // Only a scroller that actually MOVED owns the
                        // wheel; at either end it bubbles so a parent
                        // scroller takes over (no dead zone at the edge).
                        if offset.set_if_changed(to) {
                            // The pointer did not move but the content
                            // under it did — re-resolve so the ink never
                            // lights a row the cursor has left.
                            hover_handler.set_if_changed(hit_at(m.pos));
                            ctx.stop_propagation();
                        }
                    }
                    MouseKind::Down(MouseButton::Left) => {
                        match hit_at(m.pos) {
                            // The strip is inert, but it is ours: a click
                            // that lands on it must not fall through to
                            // whatever sits behind the list.
                            Some(ListHit::Scrollbar) | None => {}
                            Some(ListHit::Row(idx, ListHitZone::Accessory)) => {
                                if let Some(f) = accessory_click.borrow_mut().as_mut() {
                                    f(idx);
                                }
                            }
                            Some(ListHit::Row(idx, _)) => {
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
                // An empty list has nothing selected — announcing a row
                // number here is the same phantom-row defect the
                // selection settle above exists to prevent.
                if len == 0 {
                    return "0 items".into();
                }
                format!("{} items, selected {}", len, selection.get_untracked() + 1)
            })
            // Solved-size readback for `scroll_to` (0130 measured-extent
            // seam): the probe records the rect during paint and latches
            // ONE `after(0)` to publish it, so the viewport height
            // reaches the reactive graph without paint ever writing a
            // signal. A steady frame records an unchanged size and
            // schedules nothing.
            .draw(super::scroll::size_probe(view_box))
            .focusable();
        if let Some(focused) = self.focused {
            el = el.focus_signal(focused);
        }
        let prefix_for_draw = prefix;
        let accessory_w_draw = accessory_w;
        let cells_draw = accessory_cells;
        let rich_items_draw = rich_items;
        let hover_draw = hover;
        let accent_draw = accent;
        el.on(Phase::Bubble, handler).child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let sel = selection.get();
                let first_row = offset.get().max(0);
                let pointer = hover_draw.get();
                let items = items.clone();
                let prefix = prefix_for_draw.clone();
                let cells_inner = cells_draw.clone();
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
                        let bar_hot = pointer == Some(ListHit::Scrollbar);
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
                            // The accessory BELONGS to the row: entering
                            // it must not make the row go dark. So ink
                            // says WHICH ROW is hot and BOLD says WHICH
                            // ZONE — hue alone cannot carry both, since
                            // `accent` and `error` are the same red in
                            // several built-in themes.
                            let hot_zone = match pointer {
                                Some(ListHit::Row(i, z)) if i == idx => Some(z),
                                _ => None,
                            };
                            let style = if selected {
                                Style::new().fg(sel_fg).bg(sel_bg)
                            } else if hot_zone.is_some() {
                                Style::new().fg(accent_draw).bg(ground)
                            } else {
                                base
                            };
                            let body_style = if hot_zone == Some(ListHitZone::Body) {
                                style.attrs(Attrs::BOLD)
                            } else {
                                style
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
                                    // Hover ink reaches rich rows too —
                                    // it is the row's BASE ink, so spans
                                    // that set their own `fg` still win.
                                    draw_rich_lines(
                                        canvas,
                                        crate::base::Rect::new(rect.x, y, text_w, 1),
                                        shaped.lines.iter().take(1),
                                        if selected {
                                            sel_fg
                                        } else if hot_zone.is_some() {
                                            accent_draw
                                        } else {
                                            text_fg
                                        },
                                        crate::render::rich::HAlign::Left,
                                    );
                                } else {
                                    let line =
                                        crate::text::truncate_ellipsis(&items[idx], text_w.max(0));
                                    canvas.print_styled(
                                        crate::base::Point::new(rect.x, y),
                                        &line,
                                        &body_style,
                                    );
                                }
                            }
                            if cols.accessory_w > 0 {
                                // Pre-truncated and pre-measured at build
                                // (see AccessoryCells): painting borrows.
                                if let Some((acc_text, acc_w)) =
                                    cells_inner.as_ref().and_then(|c| c[idx].as_ref())
                                {
                                    let acc_hot = hot_zone == Some(ListHitZone::Accessory);
                                    let acc_x =
                                        rect.x + cols.body_w + (cols.accessory_w - acc_w).max(0);
                                    let acc_style = if selected {
                                        // Selection outranks hover: the
                                        // audited selection pair stays,
                                        // and BOLD marks the zone. The
                                        // hot inks are NOT contrast-
                                        // audited against selection_bg.
                                        if acc_hot {
                                            style.attrs(Attrs::BOLD)
                                        } else {
                                            style
                                        }
                                    } else if acc_hot {
                                        Style::new().fg(acc_hot_ink).bg(ground).attrs(Attrs::BOLD)
                                    } else {
                                        style
                                    };
                                    if y >= rect.y && y < rect.bottom() {
                                        canvas.print_styled(
                                            crate::base::Point::new(acc_x, y),
                                            acc_text,
                                            &acc_style,
                                        );
                                    }
                                }
                            }
                            idx += 1;
                        }
                        if show_bar {
                            draw_scrollbar(
                                canvas,
                                rect,
                                first_row,
                                total,
                                track,
                                if bar_hot { accent_draw } else { thumb },
                                ground,
                            );
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

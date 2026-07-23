//! Feed: virtualized, append-only, keyed rich-block items — the
//! chat/log/transcript surface (backlog 0100).
//!
//! An app owns a [`FeedState`] handle and mutates it (`push`, `update`,
//! `stream_*`); the [`Feed`] widget renders a WINDOW over it. Appending
//! is O(1): one item typesets, prefix sums extend, one damaged region
//! repaints. 100k items cost only the visible rows per frame.
//!
//! ```
//! use abstracttui::base::Size;
//! use abstracttui::reactive::{create_root, flush_effects};
//! use abstracttui::ui::{BufferCanvas, Element, UiTree};
//! use abstracttui::widgets::{Feed, FeedItem, FeedState};
//!
//! let mut tree = UiTree::new(Size::new(24, 4));
//! let (root, ()) = create_root(|cx| {
//!     let feed = FeedState::new(cx);
//!     feed.push("m1", FeedItem::markdown("**hello** feed"));
//!     let view = Element::new().child(Feed::new(&feed).view(cx)).build();
//!     tree.mount(cx, view);
//! });
//! flush_effects();
//! let mut canvas = BufferCanvas::new(Size::new(24, 4));
//! tree.draw(&mut canvas);
//! assert!(canvas.row_text(0).contains("hello feed"));
//! root.dispose();
//! ```
//!
//! ## Content model
//!
//! An item is a list of blocks: plain text (wrapped verbatim),
//! markdown (the DOC vocabulary —
//! [`md::parse_doc`](crate::render::md::parse_doc): the core blocks
//! plus GFM tables, in-flow images decoded lazily at first draw, task
//! lists and `~~strikethrough~~` — through the SAME typeset recipe as
//! [`MarkdownView`](super::MarkdownView), one recipe, no drift), a
//! code fence, rich span lines ([`FeedItem::rich`] — 0102: multi-ink
//! log lines/chat headers through the shared span walk), or a
//! custom-draw block (app escape hatch with an honest height-at-width
//! callback). A STREAMING item wraps
//! [`md::DocStreamSession`](crate::render::md::DocStreamSession):
//! closed blocks typeset once and freeze; only the open tail region
//! re-typesets per delta (a table streams as the open region until its
//! closing line arrives, then freezes — an agent answer streaming a
//! markdown table renders as a table live). Image rows measure from a
//! header PROBE at typeset (`gfx::probe_dimensions`); nothing decodes
//! until an image row first draws. A feed mirroring a `Signal<Vec<T>>`
//! fold binds through [`FeedState::sync`] (0104); app-driven selection
//! binds through [`Feed::selected_key`] + [`FeedState::row_of`].
//!
//! ## Windowing
//!
//! Prefix sums over item heights (the `List` machinery generalized to
//! multi-row rich items): first visible item by binary search, walk
//! until off-screen. The visible band is `rect ∩ canvas bounds`, so a
//! feed mounted inside `Scroll` (its rect huge, mostly off-screen)
//! still draws only a screenful.
//!
//! ## Geometry and the width fixup
//!
//! Typesetting needs a width, which draw discovers. Rows re-typeset
//! inside draw (a pure cache fill, the `MarkdownView` recipe); the
//! reactive HEIGHT (`total_rows`, consumed by the element style and by
//! `Scroll`'s intrinsic measure) must not be written from a draw
//! closure (RT1-2), so a width change schedules a one-shot
//! `reactive::after(0)` fixup that syncs the signal next turn —
//! latched, so steady frames schedule nothing. Appends at a known
//! width update the signal synchronously (single-frame correct).
//!
//! OWNER: CONTENT (app-widgets wave).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::Rect;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::md::DocStreamSession;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, StyledCanvas};

use super::markdown::{draw_rows, md_styles};

// Content model (public block vocabulary + item builder) — sibling
// module for the file-size discipline.
#[path = "feed_item.rs"]
mod item;
use item::SharedDrawFn;
pub use item::{CustomBlock, FeedBlock, FeedItem};

// The Signal<Vec<T>> -> keyed-feed diffing bridge (backlog 0104).
#[path = "feed_sync.rs"]
mod sync;
pub use sync::SyncSpec;

// Entry storage + typesetting internals (file-size discipline; the
// same child-module pattern `md.rs` uses for its stream half).
#[path = "feed_typeset.rs"]
mod typeset;
use typeset::{Entry, EntryKind, FeedInner, Segment, StreamEntry};

/// Cloneable handle to a feed's items. Mutations are O(1) for appends
/// and tail streaming; the widget re-renders one dyn region per change.
#[derive(Clone)]
pub struct FeedState {
    inner: Rc<RefCell<FeedInner>>,
    /// Bumped per mutation — the widget's re-render key.
    version: Signal<u64>,
    /// Total content rows at the current width (0 before first draw).
    /// The element's reactive height; `Scroll` measures it intrinsically.
    rows: Signal<i32>,
}

impl FeedState {
    pub fn new(cx: Scope) -> FeedState {
        FeedState {
            inner: Rc::new(RefCell::new(FeedInner::new())),
            version: cx.signal(0u64),
            rows: cx.signal(0i32),
        }
    }

    /// Append a keyed item. A duplicate key replaces that item's
    /// content instead (keys are identities).
    pub fn push(&self, key: impl Into<String>, item: FeedItem) {
        let key = key.into();
        {
            let mut inner = self.inner.borrow_mut();
            let existing = inner.index.get(&key).copied();
            if let Some(i) = existing {
                drop(inner);
                self.update_at(i, EntryKind::Static(item.blocks));
                return;
            }
            let i = inner.entries.len();
            inner.mutations += 1;
            inner.index.insert(key, i);
            inner.entries.push(Entry {
                kind: EntryKind::Static(item.blocks),
                segments: Vec::new(),
                height: 0,
            });
            inner.typeset_entry(i, true);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
    }

    /// Replace an item's content by key. Returns false for an unknown
    /// key (nothing changes).
    pub fn update(&self, key: &str, item: FeedItem) -> bool {
        let Some(i) = self.inner.borrow().index.get(key).copied() else {
            return false;
        };
        self.update_at(i, EntryKind::Static(item.blocks));
        true
    }

    fn update_at(&self, i: usize, kind: EntryKind) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.mutations += 1;
            inner.entries[i].kind = kind;
            inner.entries[i].segments = Vec::new();
            inner.typeset_entry(i, true);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
    }

    /// Open a STREAMING markdown item (a live answer). Feed deltas with
    /// [`FeedState::stream_append`]; seal with [`FeedState::stream_finish`].
    pub fn push_stream(&self, key: impl Into<String>) {
        let key = key.into();
        {
            let mut inner = self.inner.borrow_mut();
            let styles = inner.tokens.as_ref().map(md_styles).unwrap_or_default();
            let kind = EntryKind::Stream(Box::new(StreamEntry {
                raw: String::new(),
                session: DocStreamSession::new(styles),
                closed_seen: 0,
                finished: false,
            }));
            inner.mutations += 1;
            let existing = inner.index.get(&key).copied();
            let i = match existing {
                Some(i) => {
                    inner.entries[i].kind = kind;
                    inner.entries[i].segments = Vec::new();
                    i
                }
                None => {
                    let i = inner.entries.len();
                    inner.index.insert(key, i);
                    inner.entries.push(Entry {
                        kind,
                        segments: Vec::new(),
                        height: 0,
                    });
                    i
                }
            };
            inner.typeset_entry(i, true);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
    }

    /// Append a delta to a streaming item. Only the OPEN tail region
    /// re-typesets; closed blocks are frozen (0110's contract carried
    /// into pixels — for the doc vocabulary the open region spans a
    /// whole in-flight table, which seals at its first non-pipe line).
    /// Returns false for unknown keys or non-stream items.
    pub fn stream_append(&self, key: &str, delta: &str) -> bool {
        {
            let mut inner = self.inner.borrow_mut();
            let Some(i) = inner.index.get(key).copied() else {
                return false;
            };
            let EntryKind::Stream(stream) = &mut inner.entries[i].kind else {
                return false;
            };
            stream.raw.push_str(delta);
            stream.session.append(delta);
            inner.mutations += 1;
            inner.typeset_entry(i, false);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
        true
    }

    /// Seal a streaming item (EOF-closes an open fence or table,
    /// freezes all rows). The item stays updatable by key.
    pub fn stream_finish(&self, key: &str) -> bool {
        {
            let mut inner = self.inner.borrow_mut();
            let Some(i) = inner.index.get(key).copied() else {
                return false;
            };
            let EntryKind::Stream(stream) = &mut inner.entries[i].kind else {
                return false;
            };
            stream.session.finish();
            stream.finished = true;
            inner.mutations += 1;
            inner.typeset_entry(i, false);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
        true
    }

    /// Remove every item (keys, rows, extent). The feed itself stays
    /// append-only; `clear` is the rebuild seam for bounded windows (a
    /// drop-oldest drain rebuilds its whole window in O(window)) and
    /// for "new conversation" flows. Width/theme bindings survive, so
    /// re-pushed items typeset immediately.
    pub fn clear(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.mutations += 1;
            inner.entries.clear();
            inner.index.clear();
            inner.prefix.clear();
        }
        self.publish();
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.borrow().entries.is_empty()
    }

    /// Total content rows at the current typeset width — the reactive
    /// content extent (`Scroll` reads it through the element's height;
    /// apps can read it for "N rows" chrome). 0 until the first draw
    /// discovers a width.
    pub fn total_rows(&self) -> Signal<i32> {
        self.rows
    }

    /// Diagnostics: blocks typeset since creation. Closed stream
    /// blocks typeset exactly once — cost tests pin deltas on this.
    pub fn blocks_typeset_total(&self) -> u64 {
        self.inner.borrow().blocks_typeset
    }

    /// ITEM mutations since creation (push/update/stream/clear — theme
    /// rebinds and geometry publishes never count). Crate-internal:
    /// the sync bridge's one-writer detector (cycle-2 review C-1) —
    /// a drain that finds this moved past its own record knows a
    /// foreign write happened and self-heals with a rebuild.
    pub(super) fn mutation_count(&self) -> u64 {
        self.inner.borrow().mutations
    }

    /// The item's first content row at the current typeset width —
    /// the scroll-to-key hook: put the feed inside a `Scroll` with a
    /// bound offset signal and set it to `row_of(key)` to bring an
    /// item to the top. `None` for unknown keys; 0 for every item
    /// before the first draw discovers a width (same warmup contract
    /// as [`FeedState::total_rows`]).
    pub fn row_of(&self, key: &str) -> Option<i32> {
        let inner = self.inner.borrow();
        let i = *inner.index.get(key)?;
        Some(inner.prefix.get(i).copied().unwrap_or(0))
    }

    /// Publish after a mutation: sync the extent signal (lawful here —
    /// mutations happen in event/effect phases) and bump the render key.
    /// The `try_` reads guard against a disposed UI scope (an app-held
    /// handle outliving its widget must stay inert, never panic).
    fn publish(&self) {
        let total = self.inner.borrow().total_rows();
        if let Some(cur) = self.rows.try_get_untracked() {
            if cur != total {
                self.rows.set(total);
            }
        }
        if self.version.try_get_untracked().is_some() {
            self.version.update(|v| *v += 1);
        }
    }

    /// Deferred geometry sync for width discovered inside draw (RT1-2:
    /// no signal writes from paint). Latched: one pending fixup.
    fn schedule_geometry_sync(&self) {
        let mut inner = self.inner.borrow_mut();
        if inner.fixup_scheduled {
            return;
        }
        inner.fixup_scheduled = true;
        drop(inner);
        let state = self.clone();
        crate::reactive::after(std::time::Duration::ZERO, move || {
            state.inner.borrow_mut().fixup_scheduled = false;
            state.publish();
        });
    }
}

/// The Feed widget builder. See the module docs.
pub struct Feed {
    state: FeedState,
    gap: i32,
    selected: Option<Signal<Option<String>>>,
    layout: Option<LayoutStyle>,
}

impl Feed {
    pub fn new(state: &FeedState) -> Feed {
        Feed {
            state: state.clone(),
            gap: 1,
            selected: None,
            layout: None,
        }
    }

    /// Bind a selection-by-key signal (the 0100 item-6 gap): while
    /// `Some(key)`, that item's row band is highlighted with the
    /// theme's `selection_bg` (item inks stay — a transcript keeps its
    /// severity/syntax colors; code-fence rows keep their own ground).
    /// Selection is app-driven state, not a keyboard behavior: the app
    /// writes the signal (and can pair it with [`FeedState::row_of`]
    /// to scroll the selected item into view). Unknown keys highlight
    /// nothing.
    pub fn selected_key(mut self, key: Signal<Option<String>>) -> Feed {
        self.selected = Some(key);
        self
    }

    /// Blank rows between items (default 1).
    pub fn gap(mut self, rows: i32) -> Feed {
        self.gap = rows.max(0);
        self
    }

    /// Explicit layout (fixed-box mode: the feed shows its head and
    /// clips). Default: content-sized height (`total_rows`), full
    /// width — the shape `Scroll` measures intrinsically.
    pub fn layout(mut self, layout: LayoutStyle) -> Feed {
        self.layout = Some(layout);
        self
    }

    /// Canonical one-call build: tokens resolve from the app's theme
    /// context (tracked read — building inside a `dyn_view` re-renders
    /// on theme switch).
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, _cx: Scope, t: &TokenSet) -> Element {
        let state = self.state;
        {
            // Bind tokens; a theme change re-typesets everything (and
            // re-parses stream sessions once — their inline styles are
            // parse-time).
            let mut inner = state.inner.borrow_mut();
            if inner.tokens != Some(*t) {
                inner.tokens = Some(*t);
                inner.gap = self.gap;
                inner.retypeset_all();
            } else if inner.gap != self.gap {
                inner.gap = self.gap;
                inner.rebuild_prefix_from(0);
            }
        }
        state.publish();

        let rows = state.rows;
        let len_state = state.clone();
        let content_sized = self.layout.is_none();
        let style = match self.layout {
            Some(explicit) => {
                let mut el_style = explicit;
                // A fixed-box feed keeps the caller's geometry.
                el_style.overflow = crate::layout::Overflow::Clip;
                el_style
            }
            None => LayoutStyle::default().width(Dimension::Percent(1.0)),
        };
        let tokens = *t;

        let mut el = Element::new()
            .role(crate::ui::Role::List)
            .access_value(move || format!("{} items", len_state.len()));
        if content_sized {
            let base = style.clone();
            el = el.style_signal(move || {
                let mut s = base.clone();
                s.height = Dimension::Cells(rows.get().max(1));
                s
            });
        } else {
            el = el.style(style);
        }

        let version = state.version;
        let selected = self.selected;
        el.child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                version.get(); // the re-render key: any mutation repaints
                               // Selection is a tracked read too: a key change
                               // repaints (draw closures never read signals — RT1-2).
                let sel = selected.and_then(|s| s.get());
                let state = state.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .draw(move |canvas, rect| {
                        draw_feed(&state, &tokens, sel.as_deref(), canvas, rect)
                    })
                    .build()
            },
        ))
    }
}

/// Windowed paint: only entries intersecting `rect ∩ canvas bounds`
/// touch the canvas. Custom draws run after the state borrow releases
/// (they are app code).
fn draw_feed(
    state: &FeedState,
    t: &TokenSet,
    selected: Option<&str>,
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
) {
    let mut customs: Vec<(SharedDrawFn, Rect)> = Vec::new();
    {
        let mut inner = state.inner.borrow_mut();
        if rect.w > 1 && inner.width != rect.w {
            // Width discovery / resize: re-typeset (pure cache fill;
            // the reactive extent syncs via the deferred fixup).
            inner.width = rect.w;
            inner.retypeset_all();
            drop(inner);
            state.schedule_geometry_sync();
            inner = state.inner.borrow_mut();
        }
        let bounds = Rect::from_size(canvas.size());
        let band = rect.intersect(bounds);
        if band.is_empty() || inner.entries.is_empty() {
            return;
        }
        let first_row = band.y - rect.y;
        let last_row = first_row + band.h; // exclusive
        let inner = &*inner;
        // Selection highlight: ground the selected item's band FIRST —
        // rows paint over with transparent backgrounds, so item inks
        // stay and the tint shows through (code-fence rows keep their
        // own ground by design).
        if let Some(i) = selected.and_then(|k| inner.index.get(k).copied()) {
            let top = inner.prefix[i];
            let h = inner.entries[i].height;
            if top < last_row && top + h > first_row {
                canvas.fill(
                    Rect::new(rect.x, rect.y + top, rect.w, h),
                    ' ',
                    t.selection_fg,
                    t.selection_bg,
                );
            }
        }
        // First entry whose span can reach the band (binary search on
        // prefix starts, then step back one).
        let mut i = inner
            .prefix
            .partition_point(|&p| p <= first_row)
            .saturating_sub(1);
        while i < inner.entries.len() {
            let top = inner.prefix[i];
            if top >= last_row {
                break;
            }
            let entry = &inner.entries[i];
            let mut seg_top = top;
            for seg in &entry.segments {
                let h = seg.height();
                if seg_top >= last_row {
                    break;
                }
                if seg_top + h > first_row {
                    match seg {
                        Segment::Rows(rows) => {
                            let skip = (first_row - seg_top).max(0) as usize;
                            let visible = ((last_row - seg_top).min(h) as usize).min(rows.len());
                            if skip < visible {
                                let y = rect.y + seg_top + skip as i32;
                                draw_rows(
                                    canvas,
                                    Rect::new(rect.x, y, rect.w, (visible - skip) as i32),
                                    t,
                                    &rows[skip..visible],
                                );
                            }
                        }
                        Segment::Custom { draw, height } => {
                            // The custom block gets its FULL rect (its
                            // top may sit above the band); the canvas
                            // clips.
                            customs.push((
                                draw.clone(),
                                Rect::new(rect.x, rect.y + seg_top, rect.w, *height),
                            ));
                        }
                    }
                }
                seg_top += h;
            }
            i += 1;
        }
    }
    for (draw, r) in customs {
        draw(canvas, r);
    }
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;

// Rich-block + selection tests, split for the file-size discipline.
#[cfg(test)]
#[path = "feed_rich_tests.rs"]
mod rich_tests;

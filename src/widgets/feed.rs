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
//! An item is a list of [`FeedBlock`]s: plain text (wrapped verbatim),
//! markdown (the `render::md` vocabulary through the SAME typeset
//! recipe as [`MarkdownView`](super::MarkdownView) — one recipe, no
//! drift), a code fence, or a custom-draw block (app escape hatch with
//! an honest height-at-width callback). A STREAMING item wraps
//! [`md::StreamSession`]: closed blocks typeset once and freeze; only
//! the open tail block re-typesets per delta.
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
use std::collections::HashMap;
use std::rc::Rc;

use crate::base::Rect;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::md::{self, Block, StreamSession};
use crate::render::{RichLine, RichText};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, StyledCanvas};

use super::markdown::{draw_rows, md_styles, BlockTypesetter, Row};

/// One rich block of a feed item.
pub enum FeedBlock {
    /// Plain text, wrapped verbatim (log lines, tool output). No
    /// markdown parsing.
    Text(String),
    /// Markdown source (the supported `render::md` subset).
    Markdown(String),
    /// A fenced code block: highlighted like a markdown fence.
    Code {
        /// Language label (currently informational; the C-like lexer
        /// highlights all languages).
        lang: String,
        /// Verbatim source.
        source: String,
    },
    /// App-drawn block: an honest height-at-width callback plus a draw
    /// closure over the block's solved sub-rect. The draw MUST NOT
    /// mutate the owning `FeedState` (it runs during paint).
    Custom(CustomBlock),
}

/// Shared draw closure (custom blocks are drawn from cloned handles so
/// user paint code runs outside the feed-state borrow).
type SharedDrawFn = Rc<dyn Fn(&mut dyn StyledCanvas, Rect)>;

/// The custom-draw escape hatch (badges, tool cards, charts).
pub struct CustomBlock {
    height: Box<dyn Fn(i32) -> i32>,
    draw: SharedDrawFn,
}

impl CustomBlock {
    /// `height(width) -> rows` must be honest (windowing trusts it);
    /// `draw(canvas, rect)` paints inside the block's rect.
    pub fn new(
        height: impl Fn(i32) -> i32 + 'static,
        draw: impl Fn(&mut dyn StyledCanvas, Rect) + 'static,
    ) -> CustomBlock {
        CustomBlock {
            height: Box::new(height),
            draw: Rc::new(draw),
        }
    }
}

/// One feed item: a small block list. Items are IDENTITIES (keyed);
/// see [`FeedState::push`].
pub struct FeedItem {
    blocks: Vec<FeedBlock>,
}

impl FeedItem {
    pub fn new() -> FeedItem {
        FeedItem { blocks: Vec::new() }
    }

    /// Single markdown-block item (the common chat message).
    pub fn markdown(src: impl Into<String>) -> FeedItem {
        FeedItem::new().block(FeedBlock::Markdown(src.into()))
    }

    /// Single plain-text item (the common log line).
    pub fn text(s: impl Into<String>) -> FeedItem {
        FeedItem::new().block(FeedBlock::Text(s.into()))
    }

    /// Single code-fence item.
    pub fn code(lang: impl Into<String>, source: impl Into<String>) -> FeedItem {
        FeedItem::new().block(FeedBlock::Code {
            lang: lang.into(),
            source: source.into(),
        })
    }

    pub fn block(mut self, b: FeedBlock) -> FeedItem {
        self.blocks.push(b);
        self
    }
}

impl Default for FeedItem {
    fn default() -> Self {
        FeedItem::new()
    }
}

enum EntryKind {
    Static(Vec<FeedBlock>),
    /// Boxed: the session dwarfs the static variant and entries live in
    /// a big Vec (clippy::large_enum_variant).
    Stream(Box<StreamEntry>),
}

struct StreamEntry {
    /// Full raw source (kept so a theme rebind can re-parse; the
    /// session itself never revisits closed content).
    raw: String,
    session: StreamSession,
    /// Closed blocks already typeset into `segments` (freeze line).
    closed_seen: usize,
    finished: bool,
}

/// A typeset run of an entry: markdown rows or a custom-draw region.
enum Segment {
    Rows(Vec<Row>),
    Custom { draw: SharedDrawFn, height: i32 },
}

impl Segment {
    fn height(&self) -> i32 {
        match self {
            Segment::Rows(rows) => rows.len() as i32,
            Segment::Custom { height, .. } => *height,
        }
    }
}

struct Entry {
    kind: EntryKind,
    /// Typeset at `FeedInner::width`. For streams: [closed, open].
    segments: Vec<Segment>,
    height: i32,
}

impl Entry {
    fn recount(&mut self) {
        self.height = self.segments.iter().map(Segment::height).sum();
    }
}

struct FeedInner {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    /// Typeset width; 0 = unknown (nothing typeset yet).
    width: i32,
    /// prefix[i] = first content row of entry i (gaps included);
    /// prefix[len] = total rows + trailing gap allowance (unused).
    prefix: Vec<i32>,
    /// Blank rows between items.
    gap: i32,
    tokens: Option<TokenSet>,
    /// One pending after(0) geometry sync at a time.
    fixup_scheduled: bool,
    /// Diagnostics: blocks typeset since creation (cost pins — closed
    /// stream blocks must typeset exactly once).
    blocks_typeset: u64,
}

impl FeedInner {
    fn total_rows(&self) -> i32 {
        match self.entries.len() {
            0 => 0,
            n => self.prefix[n - 1] + self.entries[n - 1].height,
        }
    }

    fn rebuild_prefix_from(&mut self, start: usize) {
        self.prefix.truncate(start);
        let mut acc = if start == 0 {
            0
        } else {
            self.prefix[start - 1] + self.entries[start - 1].height + self.gap
        };
        for e in &self.entries[start..] {
            self.prefix.push(acc);
            acc += e.height + self.gap;
        }
    }

    /// Typeset one entry's segments at `width` with `tokens`. Streams
    /// typeset closed blocks once and re-do only the open tail; a full
    /// reset (width/theme change) rebuilds everything.
    fn typeset_entry(&mut self, i: usize, full: bool) {
        let (width, Some(tokens)) = (self.width, self.tokens) else {
            return;
        };
        if width <= 0 {
            return;
        }
        let ts = BlockTypesetter::new(&tokens);
        let entry = &mut self.entries[i];
        match &mut entry.kind {
            EntryKind::Static(blocks) => {
                if full || entry.segments.is_empty() {
                    self.blocks_typeset += blocks.len() as u64;
                    entry.segments = typeset_static(blocks, &ts, &tokens, width);
                    entry.recount();
                }
            }
            EntryKind::Stream(stream) => {
                if full {
                    // Theme/width reset: re-parse the raw source once
                    // through a fresh session (closed content is only
                    // ever re-parsed HERE, never on append).
                    let mut s = StreamSession::new(ts.styles().clone());
                    s.append(&stream.raw);
                    if stream.finished {
                        s.finish();
                    }
                    stream.session = s;
                    stream.closed_seen = 0;
                    entry.segments = vec![Segment::Rows(Vec::new()), Segment::Rows(Vec::new())];
                }
                if entry.segments.is_empty() {
                    entry.segments = vec![Segment::Rows(Vec::new()), Segment::Rows(Vec::new())];
                }
                // Freeze newly closed blocks into segment 0.
                let closed = stream.session.closed_blocks();
                if stream.closed_seen < closed.len() {
                    let Segment::Rows(rows) = &mut entry.segments[0] else {
                        unreachable!("stream segment 0 is rows");
                    };
                    for b in &closed[stream.closed_seen..] {
                        self.blocks_typeset += 1;
                        ts.push_block(rows, b, width, true);
                    }
                    stream.closed_seen = closed.len();
                }
                // Re-typeset the open tail into segment 1.
                let closed_rows = match &entry.segments[0] {
                    Segment::Rows(rows) => rows.len(),
                    _ => 0,
                };
                let open = stream.session.open_blocks();
                let mut rows: Vec<Row> = Vec::new();
                for (bi, b) in open.iter().enumerate() {
                    self.blocks_typeset += 1;
                    // The blank separator between the frozen rows and
                    // the first open block mirrors push_block's policy
                    // (out non-empty), which cannot see across the
                    // segment boundary: list items stack tight,
                    // everything else gets one blank row.
                    if bi == 0 && closed_rows > 0 && !matches!(b, Block::ListItem { .. }) {
                        rows.push(Row::plain(RichLine::new()));
                    }
                    ts.push_block(&mut rows, b, width, bi > 0);
                }
                entry.segments[1] = Segment::Rows(rows);
                entry.recount();
            }
        }
    }

    fn retypeset_all(&mut self) {
        for i in 0..self.entries.len() {
            self.typeset_entry(i, true);
        }
        self.rebuild_prefix_from(0);
    }
}

/// Typeset a static block list into segments (rows runs split around
/// custom blocks). Separator policy matches the markdown document
/// rhythm: one blank row before every non-list block after content.
fn typeset_static(
    blocks: &[FeedBlock],
    ts: &BlockTypesetter,
    tokens: &TokenSet,
    width: i32,
) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut current: Vec<Row> = Vec::new();
    let mut any_content = false;
    for b in blocks {
        match b {
            FeedBlock::Text(s) => {
                if any_content && current.is_empty() {
                    current.push(Row::plain(RichLine::new()));
                }
                let ink = crate::render::Style::new().fg(tokens.text);
                for line in RichText::plain(s, ink).wrap(width.max(4)).lines {
                    current.push(Row::plain(line));
                }
                any_content = true;
            }
            FeedBlock::Markdown(src) => {
                if any_content && current.is_empty() {
                    current.push(Row::plain(RichLine::new()));
                }
                for block in md::parse(src, ts.styles()) {
                    ts.push_block(&mut current, &block, width, true);
                }
                any_content = true;
            }
            FeedBlock::Code { lang, source } => {
                if any_content && current.is_empty() {
                    current.push(Row::plain(RichLine::new()));
                }
                let block = Block::CodeFence {
                    lang: lang.clone(),
                    lines: source.split('\n').map(str::to_string).collect(),
                };
                ts.push_block(&mut current, &block, width, true);
                any_content = true;
            }
            FeedBlock::Custom(c) => {
                if !current.is_empty() {
                    segments.push(Segment::Rows(std::mem::take(&mut current)));
                }
                if any_content {
                    // Same one-blank-row rhythm before a custom block.
                    segments.push(Segment::Rows(vec![Row::plain(RichLine::new())]));
                }
                segments.push(Segment::Custom {
                    draw: c.draw.clone(),
                    height: (c.height)(width).max(0),
                });
                any_content = true;
            }
        }
    }
    if !current.is_empty() {
        segments.push(Segment::Rows(current));
    }
    segments
}

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
            inner: Rc::new(RefCell::new(FeedInner {
                entries: Vec::new(),
                index: HashMap::new(),
                width: 0,
                prefix: Vec::new(),
                gap: 1,
                tokens: None,
                fixup_scheduled: false,
                blocks_typeset: 0,
            })),
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
                session: StreamSession::new(styles),
                closed_seen: 0,
                finished: false,
            }));
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

    /// Append a delta to a streaming item. Only the OPEN tail block
    /// re-typesets; closed blocks are frozen (0110's contract carried
    /// into pixels). Returns false for unknown keys or non-stream items.
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
            inner.typeset_entry(i, false);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
        true
    }

    /// Seal a streaming item (EOF-closes an open fence, freezes all
    /// rows). The item stays updatable by key.
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
            inner.typeset_entry(i, false);
            inner.rebuild_prefix_from(i);
        }
        self.publish();
        true
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
    layout: Option<LayoutStyle>,
}

impl Feed {
    pub fn new(state: &FeedState) -> Feed {
        Feed {
            state: state.clone(),
            gap: 1,
            layout: None,
        }
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
        el.child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                version.get(); // the re-render key: any mutation repaints
                let state = state.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .draw(move |canvas, rect| draw_feed(&state, &tokens, canvas, rect))
                    .build()
            },
        ))
    }
}

/// Windowed paint: only entries intersecting `rect ∩ canvas bounds`
/// touch the canvas. Custom draws run after the state borrow releases
/// (they are app code).
fn draw_feed(state: &FeedState, t: &TokenSet, canvas: &mut dyn StyledCanvas, rect: Rect) {
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

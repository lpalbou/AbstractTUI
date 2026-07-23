//! Feed internals: entry storage + typesetting (child module of
//! `feed`, split for the file-size discipline — one file, one task:
//! this one turns blocks into frozen row segments and keeps the
//! prefix-sum geometry; `feed.rs` owns the public model, the state
//! handle and the windowed painter).
//!
//! OWNER: CONTENT (app-widgets wave).

use std::collections::HashMap;

use crate::render::md::{self, Block, DocBlock, DocStreamSession};
use crate::render::rich::Span;
use crate::render::{RichLine, RichText};
use crate::theme::TokenSet;

use super::super::markdown::{BlockTypesetter, Row};
use super::item::{ItemBlock, RowCap, SharedDrawFn};

pub(super) enum EntryKind {
    Static(Vec<ItemBlock>),
    /// Boxed: the session dwarfs the static variant and entries live in
    /// a big Vec (clippy::large_enum_variant).
    Stream(Box<StreamEntry>),
}

pub(super) struct StreamEntry {
    /// Full raw source (kept so a theme rebind can re-parse; the
    /// session itself never revisits closed content).
    pub(super) raw: String,
    /// DOC-vocabulary session (wave 3): streamed answers get tables,
    /// in-flow images, task lists and strikethrough — same freeze
    /// contract as the core session (closed blocks never re-parse; a
    /// table seals only once its closing line arrives, so a streamed
    /// table is the OPEN region until then).
    pub(super) session: DocStreamSession,
    /// Closed blocks already typeset into `segments` (freeze line).
    pub(super) closed_seen: usize,
    pub(super) finished: bool,
}

/// A typeset run of an entry: markdown rows or a custom-draw region.
pub(super) enum Segment {
    Rows(Vec<Row>),
    Custom { draw: SharedDrawFn, height: i32 },
}

impl Segment {
    pub(super) fn height(&self) -> i32 {
        match self {
            Segment::Rows(rows) => rows.len() as i32,
            Segment::Custom { height, .. } => *height,
        }
    }
}

pub(super) struct Entry {
    pub(super) kind: EntryKind,
    /// Typeset at `FeedInner::width`. For streams: [closed, open].
    pub(super) segments: Vec<Segment>,
    pub(super) height: i32,
}

impl Entry {
    fn recount(&mut self) {
        self.height = self.segments.iter().map(Segment::height).sum();
    }
}

pub(super) struct FeedInner {
    pub(super) entries: Vec<Entry>,
    pub(super) index: HashMap<String, usize>,
    /// Typeset width; 0 = unknown (nothing typeset yet).
    pub(super) width: i32,
    /// prefix[i] = first content row of entry i (gaps included);
    /// prefix[len] = total rows + trailing gap allowance (unused).
    pub(super) prefix: Vec<i32>,
    /// Blank rows between items.
    pub(super) gap: i32,
    pub(super) tokens: Option<TokenSet>,
    /// One pending after(0) geometry sync at a time.
    pub(super) fixup_scheduled: bool,
    /// Diagnostics: blocks typeset since creation (cost pins — closed
    /// stream blocks must typeset exactly once).
    pub(super) blocks_typeset: u64,
    /// ITEM mutations since creation (push/update/stream/clear — never
    /// theme rebinds or geometry publishes). The sync bridge's
    /// one-writer detector: a drain finding this moved past its own
    /// record knows a foreign write happened and takes the rebuild
    /// path (cycle-2 review C-1). One u64 compare per drain.
    pub(super) mutations: u64,
}

impl FeedInner {
    pub(super) fn new() -> FeedInner {
        FeedInner {
            entries: Vec::new(),
            index: HashMap::new(),
            width: 0,
            prefix: Vec::new(),
            gap: 1,
            tokens: None,
            fixup_scheduled: false,
            blocks_typeset: 0,
            mutations: 0,
        }
    }

    pub(super) fn total_rows(&self) -> i32 {
        match self.entries.len() {
            0 => 0,
            n => self.prefix[n - 1] + self.entries[n - 1].height,
        }
    }

    pub(super) fn rebuild_prefix_from(&mut self, start: usize) {
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
    pub(super) fn typeset_entry(&mut self, i: usize, full: bool) {
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
                    let mut s = DocStreamSession::new(ts.styles().clone());
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
                        ts.push_doc_block(rows, b, width, true);
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
                    // the first open block mirrors push_doc_block's
                    // policy (out non-empty), which cannot see across
                    // the segment boundary: list/task items stack
                    // tight, everything else gets one blank row.
                    if bi == 0 && closed_rows > 0 && doc_block_separates(b) {
                        rows.push(Row::plain(RichLine::new()));
                    }
                    ts.push_doc_block(&mut rows, b, width, bi > 0);
                }
                entry.segments[1] = Segment::Rows(rows);
                entry.recount();
            }
        }
    }

    pub(super) fn retypeset_all(&mut self) {
        for i in 0..self.entries.len() {
            self.typeset_entry(i, true);
        }
        self.rebuild_prefix_from(0);
    }
}

/// Would `push_doc_block(out, b, _, separate=true)` open with a blank
/// separator row when `out` is non-empty? Mirrored here for the ONE
/// place the typesetter cannot see prior content — the stream's
/// closed/open segment boundary. Kept in lockstep with the per-arm
/// `blank(...)` calls in `push_block`/`push_doc_block`: list items and
/// task items stack tight; future doc kinds typeset to nothing
/// (`push_doc_block`'s wildcard), so no separator either. Pinned by
/// `streamed_item_matches_static_item_pixels` across the doc
/// vocabulary — drift shows up as a pixel diff at the boundary.
fn doc_block_separates(b: &DocBlock) -> bool {
    match b {
        DocBlock::Core(core) => !matches!(core, Block::ListItem { .. }),
        DocBlock::Table(_) | DocBlock::Image(_) => true,
        DocBlock::Task(_) => false,
        // Future doc blocks typeset to nothing (push_doc_block's
        // wildcard) — no separator either. Unreachable in-crate today
        // (non_exhaustive binds downstream only): same precedent as
        // push_doc_block's own wildcard arm.
        #[allow(unreachable_patterns)]
        _ => false,
    }
}

/// Typeset a static block list into segments (rows runs split around
/// custom blocks). Separator policy matches the markdown document
/// rhythm: one blank row before every non-list block after content.
fn typeset_static(
    blocks: &[ItemBlock],
    ts: &BlockTypesetter,
    tokens: &TokenSet,
    width: i32,
) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut current: Vec<Row> = Vec::new();
    let mut any_content = false;
    for b in blocks {
        match b {
            ItemBlock::Text { text, cap } => {
                if any_content && current.is_empty() {
                    current.push(Row::plain(RichLine::new()));
                }
                let ink = crate::render::Style::new().fg(tokens.text);
                let lines = RichText::plain(text, ink).wrap(width.max(4)).lines;
                push_capped(&mut current, lines, cap.as_ref(), tokens);
                any_content = true;
            }
            ItemBlock::Rich { text, cap } => {
                // Backlog 0102: span-model lines through the SAME
                // span-preserving wrap (`RichText::wrap`) and the same
                // row walk (`draw_rows` -> `print_span_clipped`) as
                // every other block — one renderer, one more face.
                // Spans are stored VERBATIM (no ink stamping): fg-less
                // spans inherit the item ink at draw time, so the
                // theme patch rule survives typesetting. Separator
                // policy mirrors `Text` (its sibling class).
                if any_content && current.is_empty() {
                    current.push(Row::plain(RichLine::new()));
                }
                let lines = text.wrap(width.max(4)).lines;
                push_capped(&mut current, lines, cap.as_ref(), tokens);
                any_content = true;
            }
            ItemBlock::Markdown(src) => {
                if any_content && current.is_empty() {
                    current.push(Row::plain(RichLine::new()));
                }
                // DOC vocabulary (wave 3): tables, in-flow images (lazy
                // mosaic), task lists — one recipe with MarkdownView
                // (`layout_doc` walks the same parse + typeset pair).
                for block in md::parse_doc(src, ts.styles()) {
                    ts.push_doc_block(&mut current, &block, width, true);
                }
                any_content = true;
            }
            ItemBlock::Code { lang, source } => {
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
            ItemBlock::Custom(c) => {
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

/// Push wrapped Text/Rich lines under an optional row cap
/// (first-app/0283). Uncapped or fitting content pushes unchanged.
/// Overflow keeps the first `max_rows - 1` wrapped rows and spends the
/// last row on an honest marker — "… (+K more lines)" (or the block's
/// override wording) in `text_muted` — so a capped block is never
/// taller than `max_rows`, never hides content silently, and the
/// extent arithmetic (segment height = row count) stays exact with the
/// marker row counted. K is the HIDDEN wrapped-row count at this
/// width, so it changes when the width does — which is exactly why the
/// cap lives where the wrap lives. The marker is minted at typeset
/// time from the bound tokens, so a theme rebind retints it (feeds
/// re-typeset everything on token change).
fn push_capped(
    current: &mut Vec<Row>,
    lines: Vec<RichLine>,
    cap: Option<&RowCap>,
    tokens: &TokenSet,
) {
    let max = cap.and_then(|c| c.max_rows).map(|m| m.max(1));
    match max {
        Some(max) if lines.len() > max => {
            let shown = max - 1;
            let hidden = lines.len() - shown;
            for line in lines.into_iter().take(shown) {
                current.push(Row::plain(line));
            }
            let text = match cap.and_then(|c| c.marker.as_ref()) {
                Some(f) => f(hidden),
                None => format!("… (+{hidden} more lines)"),
            };
            // One row by design: overwide marker text clips at the
            // item width through the shared row walk, never wraps.
            current.push(Row::plain(RichLine::from_spans(vec![Span::new(
                text,
                crate::render::Style::new().fg(tokens.text_muted),
            )])));
        }
        _ => {
            for line in lines {
                current.push(Row::plain(line));
            }
        }
    }
}

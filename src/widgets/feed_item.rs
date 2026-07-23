//! Feed content model: the public block vocabulary (`FeedBlock`,
//! `CustomBlock`) and the item builder (`FeedItem`) — split from
//! `feed.rs` for the file-size discipline (child module of `feed`,
//! the `feed_typeset.rs` pattern).
//!
//! ## The block-vocabulary note (backlog 0102, coordinates 0660/0280)
//!
//! `FeedBlock` shipped EXHAUSTIVE in 0.2.x, so new block kinds cannot
//! land as public variants without a major bump (`cargo semver-checks`
//! `enum_variant_added`). The vocabulary therefore grows in two places:
//! the crate-private [`ItemBlock`] enum is the REAL block vocabulary
//! (typesetting matches on it), and `FeedItem` grows constructors
//! (`rich`, `rich_block`, `rich_lines`) that mint the new kinds.
//! `FeedBlock` values convert losslessly via `From`. The 0.3 budget
//! (docs/backlog/planned/0002) records the fold-back: `FeedBlock`
//! gains `#[non_exhaustive]` + the `Rich` variant, and 0660/0280's
//! block kinds land additively after it.
//!
//! OWNER: CONTENT (app-widgets wave).

use std::rc::Rc;

use crate::base::Rect;
use crate::render::RichText;
use crate::ui::StyledCanvas;

/// One rich block of a feed item.
pub enum FeedBlock {
    /// Plain text, wrapped verbatim (log lines, tool output). No
    /// markdown parsing.
    Text(String),
    /// Markdown source — the DOC vocabulary
    /// ([`md::parse_doc`](crate::render::md::parse_doc)): core blocks
    /// plus GFM tables, in-flow images (lazy mosaic), task lists and
    /// strikethrough.
    Markdown(String),
    /// A fenced code block: highlighted like a markdown fence.
    Code {
        /// Language label. Routes the lexer like a markdown fence:
        /// `"diff"`/`"patch"` tint through the diff mapping
        /// (`code::diff_token_color`); every other label renders with
        /// the C-like lexer.
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
pub(super) type SharedDrawFn = Rc<dyn Fn(&mut dyn StyledCanvas, Rect)>;

/// The custom-draw escape hatch (badges, tool cards, charts).
pub struct CustomBlock {
    pub(super) height: Box<dyn Fn(i32) -> i32>,
    pub(super) draw: SharedDrawFn,
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

/// Width-aware row cap on a Text/Rich block (first-app/0283): applied
/// POST-WRAP at the width the engine typesets at, because the row
/// count only exists after the wrap — a consumer cannot precompute it.
/// Both fields fill through the [`FeedItem::max_rows`] /
/// [`FeedItem::overflow_marker`] builders; a marker without a cap is
/// inert by construction (nothing overflows).
pub(super) struct RowCap {
    /// Total typeset rows the block may occupy, MARKER ROW INCLUDED
    /// (so extent/windowing arithmetic stays exact). Clamped ≥ 1.
    pub(super) max_rows: Option<usize>,
    /// Marker wording override: `hidden_line_count -> text`. Default:
    /// "… (+K more lines)".
    pub(super) marker: Option<Rc<dyn Fn(usize) -> String>>,
}

impl RowCap {
    fn empty() -> RowCap {
        RowCap {
            max_rows: None,
            marker: None,
        }
    }
}

/// The crate-private block vocabulary the typesetter matches on — the
/// public `FeedBlock` plus the post-0.2 kinds (module doc above). Kept
/// FLAT (no nesting of `FeedBlock`) so `typeset_static` reads like the
/// eventual 0.3 public enum.
pub(super) enum ItemBlock {
    Text {
        text: String,
        /// Optional row cap (first-app/0283); `None` = unbounded, the
        /// pre-cap behavior byte-for-byte.
        cap: Option<RowCap>,
    },
    Markdown(String),
    Code {
        lang: String,
        source: String,
    },
    Custom(CustomBlock),
    /// Span-model lines (backlog 0102): typeset through the SAME
    /// span-preserving wrap + row walk as everything else. Replace-on-
    /// update like `Text` (streaming spans are out of scope by design).
    Rich {
        text: RichText,
        /// Optional row cap, same contract as `Text`.
        cap: Option<RowCap>,
    },
}

impl From<FeedBlock> for ItemBlock {
    fn from(b: FeedBlock) -> ItemBlock {
        match b {
            FeedBlock::Text(s) => ItemBlock::Text { text: s, cap: None },
            FeedBlock::Markdown(s) => ItemBlock::Markdown(s),
            FeedBlock::Code { lang, source } => ItemBlock::Code { lang, source },
            FeedBlock::Custom(c) => ItemBlock::Custom(c),
        }
    }
}

/// One feed item: a small block list. Items are IDENTITIES (keyed);
/// see [`super::FeedState::push`].
pub struct FeedItem {
    pub(super) blocks: Vec<ItemBlock>,
}

impl FeedItem {
    pub fn new() -> FeedItem {
        FeedItem { blocks: Vec::new() }
    }

    /// Single markdown-block item (the common chat message). Parses
    /// the DOC vocabulary: tables, images, task lists and
    /// strikethrough render alongside the core blocks.
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

    /// Single rich-text item (backlog 0102): multi-ink spans per line
    /// without a custom block — severity-tinted log lines, chat
    /// headers, status rows. Lines wrap at item width through the
    /// span-preserving wrap (`render::RichText::wrap`), and span
    /// styles stay PATCHES: `fg: None` spans inherit the item's theme
    /// ink at draw time, explicit inks are resolved `Rgba` and render
    /// verbatim (rebuild the item to retint on theme switch — the
    /// widget-wide token posture). Rich items are replace-on-update
    /// like `text`; for token streaming use [`super::FeedState::push_stream`].
    pub fn rich(text: RichText) -> FeedItem {
        FeedItem::new().rich_block(text)
    }

    /// Single rich-text item from pre-built lines — the common "one
    /// styled line" construction without spelling `RichText`:
    ///
    /// ```ignore
    /// FeedItem::rich_lines(vec![RichLine::from_spans(vec![
    ///     Span::new("ERROR ", Style::new().fg(t.error)),
    ///     Span::plain("disk full"),
    /// ])])
    /// ```
    pub fn rich_lines(lines: Vec<crate::render::RichLine>) -> FeedItem {
        FeedItem::rich(RichText::from_lines(lines))
    }

    /// Append a rich-text block (builder form of [`FeedItem::rich`],
    /// composable with [`FeedItem::block`] in any order).
    pub fn rich_block(mut self, text: RichText) -> FeedItem {
        self.blocks.push(ItemBlock::Rich { text, cap: None });
        self
    }

    pub fn block(mut self, b: FeedBlock) -> FeedItem {
        self.blocks.push(b.into());
        self
    }

    /// Cap the most recently appended Text/Rich block at `rows` typeset
    /// rows (first-app/0283) — the transcript-preview idiom. The cap is
    /// WIDTH-AWARE: it applies POST-WRAP at the width the engine
    /// typesets at, where the row count actually exists. Content that
    /// wraps to at most `rows` rows renders unchanged; overflow shows
    /// the first `rows - 1` wrapped rows and spends the last row on an
    /// honest marker ("… (+K more lines)", `text_muted` ink, K = the
    /// hidden wrapped-row count — it changes with width). A capped
    /// block is therefore never taller than `rows` and never hides
    /// content silently; extent/windowing count the marker row.
    /// `rows` clamps to ≥ 1 (a zero-row cap over hidden content would
    /// vanish it without a trace). Streaming items are unaffected
    /// (caps live on static Text/Rich blocks only).
    ///
    /// Chain per block: `.block(a).max_rows(3).block(b).max_rows(8)`.
    /// Debug builds assert when the last block is not Text/Rich
    /// (Markdown/Code/Custom carry no row cap); release ignores it.
    pub fn max_rows(mut self, rows: usize) -> FeedItem {
        let slot = self.last_cap();
        debug_assert!(
            slot.is_some(),
            "FeedItem::max_rows targets the last appended Text/Rich block"
        );
        if let Some(cap) = slot {
            cap.get_or_insert_with(RowCap::empty).max_rows = Some(rows.max(1));
        }
        self
    }

    /// Override the overflow-marker wording of the most recently
    /// appended Text/Rich block: `hidden_line_count -> text` (e.g.
    /// `|k| format!("… (+{k} more — full text in the ledger)")`).
    /// Renders in `text_muted`, one row, clipped at the item width.
    /// Inert without [`FeedItem::max_rows`] on the same block.
    pub fn overflow_marker(mut self, marker: impl Fn(usize) -> String + 'static) -> FeedItem {
        let slot = self.last_cap();
        debug_assert!(
            slot.is_some(),
            "FeedItem::overflow_marker targets the last appended Text/Rich block"
        );
        if let Some(cap) = slot {
            cap.get_or_insert_with(RowCap::empty).marker = Some(Rc::new(marker));
        }
        self
    }

    /// The cap slot of the last block, when that block kind carries one.
    fn last_cap(&mut self) -> Option<&mut Option<RowCap>> {
        match self.blocks.last_mut() {
            Some(ItemBlock::Text { cap, .. }) | Some(ItemBlock::Rich { cap, .. }) => Some(cap),
            _ => None,
        }
    }
}

impl Default for FeedItem {
    fn default() -> Self {
        FeedItem::new()
    }
}

//! MarkdownView: RENDER's markdown blocks, themed and typeset.
//!
//! ```ignore
//! use abstracttui::widgets::MarkdownView;
//! let view = MarkdownView::new(doc_source)
//!     .scroll_offset(top.get())
//!     .element(&t)
//!     .build();
//! ```
//!
//! Typesetting (all tokens, §3.3): headings step `accent`(+BOLD) ->
//! `accent` -> `text`+BOLD with a `border` underline rule beneath level 1;
//! list markers in `accent_alt`; blockquotes carry a `border` bar with
//! `text_muted` prose; code fences render through the highlighter on the
//! `surface_raised` code ground (the same inks as `CodeView` — one
//! mapping, `code_token_color`); inline code = `surface_raised` chip;
//! links = `link` ink + underline. Layout happens at draw width (wrap),
//! deterministically — `MarkdownView::rows(...)` exposes the same fold
//! for the app's scroll clamp.
//!
//! OWNER: DESIGN.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rgba};
use crate::layout::Style as LayoutStyle;
use crate::render::md::{self, Block, Marker, MdStyles};
use crate::render::rich::{RichLine, RichText, Span};
use crate::render::{Attrs, Style};
use crate::text::{CLikeLexer, DiffLexer, JsonLexer, YamlLexer};
use crate::theme::TokenSet;
use crate::ui::{Element, StyledCanvas};

use super::code::{code_token_color, data_rich_line, diff_rich_line};

/// Doc-vocabulary typesetting (0142): tables (column solving shared
/// with the Table widget), task items, and the doc layout fold with
/// heading rows.
#[path = "markdown_doc.rs"]
pub(crate) mod doc;
pub use doc::OutlineEntry;

/// In-flow image rows (0144): probe at typeset, decode lazily at first
/// draw, mosaic-only rendering.
#[path = "markdown_image.rs"]
pub(crate) mod imageflow;

/// Find-in-typeset-text (0148) + the text↔cells mapping substrate
/// shared with content selection (0160).
#[path = "markdown_search.rs"]
pub(crate) mod search;
pub use search::MdSearchMatch;

/// Which ink a horizontal rule is drawn in.
///
/// `Token` resolves against the live theme at every typeset, so a rule
/// keeps following theme swaps; `Fixed` opts out of that deliberately
/// and is the caller's to maintain across themes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MdRuleInk {
    Token(crate::theme::TokenId),
    Fixed(Rgba),
}

/// How wide a horizontal rule runs.
///
/// The measure box is the width the block was TYPESET at — not the
/// width of its text (a rule has none) and not necessarily the rect it
/// is drawn into, which a consumer may make wider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdRuleWidth {
    /// Edge to edge of the drawn rect. The default, and what every
    /// release before this policy existed did unconditionally.
    FullBleed,
    /// The measure box.
    Measure,
    /// The measure box inset by `cells` on each side.
    Inset(i32),
}

/// The three axes a horizontal rule (`---`) spends: ink, width, and
/// vertical space. All three are the caller's, all three at once —
/// a policy that opened one of them would let a consumer ship half an
/// ordinal and believe it was finished (`decision:thread-shape-in-a-terminal`
/// clause 3, and agora-wui's acceptance condition on it).
///
/// [`MdRuleStyle::default()`] is byte-identical to the hardwired
/// behaviour that preceded it: `border` ink, full bleed, one blank row
/// on each side. Nothing moves until a caller asks it to.
///
/// The values — one row or three, full bleed or inset, which ink — stay
/// the caller's taste; this type is only the door to them
/// (`decision:panel-ground-ownership` clause 1).
///
/// ```ignore
/// // The console's ordinal in a terminal: the in-body rule quiet and
/// // inset, so the boundary between two authors can be the louder line.
/// let quiet = MdRuleStyle::default()
///     .ink(MdRuleInk::Token(TokenId::TextFaint))
///     .width(MdRuleWidth::Inset(2))
///     .space(0, 0);
/// MarkdownView::new(src).rule_style(quiet)
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MdRuleStyle {
    pub ink: MdRuleInk,
    pub width: MdRuleWidth,
    /// Blank rows before the rule. Suppressed when the rule opens the
    /// document (nothing to separate from) — as before this policy.
    pub space_before: i32,
    /// Blank rows after the rule. The rule owns BOTH sides of its own
    /// gap: the block that follows contributes none of its usual
    /// separator, so this number is the whole story. `1`/`1` is the
    /// historical three-row rule, `0`/`0` a one-row one.
    ///
    /// Trailing space is emitted by the next block, not by the rule, so
    /// a document ENDING in a rule still ends at the rule — as before.
    pub space_after: i32,
}

impl Default for MdRuleStyle {
    fn default() -> MdRuleStyle {
        MdRuleStyle {
            ink: MdRuleInk::Token(crate::theme::TokenId::Border),
            width: MdRuleWidth::FullBleed,
            space_before: 1,
            space_after: 1,
        }
    }
}

impl MdRuleStyle {
    pub fn ink(mut self, ink: MdRuleInk) -> MdRuleStyle {
        self.ink = ink;
        self
    }

    pub fn width(mut self, width: MdRuleWidth) -> MdRuleStyle {
        self.width = width;
        self
    }

    /// Blank rows before and after. Negative counts clamp to zero.
    pub fn space(mut self, before: i32, after: i32) -> MdRuleStyle {
        self.space_before = before.max(0);
        self.space_after = after.max(0);
        self
    }

    /// Resolve this style against a theme and a measure into the row
    /// chrome the painter reads.
    fn resolve(&self, t: &TokenSet, measure: i32) -> RuleRow {
        let ink = match self.ink {
            MdRuleInk::Token(id) => t.get(id),
            MdRuleInk::Fixed(c) => c,
        };
        let (indent, extent) = match self.width {
            MdRuleWidth::FullBleed => (0, None),
            MdRuleWidth::Measure => (0, Some(measure.max(1))),
            // A rule inset past its own measure would paint NOTHING,
            // which is the silent no-op `decision:panel-ground-ownership`
            // clause 4 names — the renderer will not manufacture one, so
            // the extent floors at a single cell.
            MdRuleWidth::Inset(cells) => {
                let cells = cells.clamp(0, (measure - 1).max(0) / 2);
                (cells, Some((measure - 2 * cells).max(1)))
            }
        };
        RuleRow {
            ink,
            indent,
            extent,
            gap_after: self.space_after.max(0),
        }
    }
}

/// A rule row's resolved chrome: what `draw_rows` paints, and how much
/// space the NEXT block leaves in front of itself.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct RuleRow {
    pub(crate) ink: Rgba,
    /// Left inset from the drawn rect, in cells.
    pub(crate) indent: i32,
    /// Cells of ink; `None` runs to the right edge of the drawn rect.
    pub(crate) extent: Option<i32>,
    /// Blank rows the FOLLOWING block emits instead of its usual one.
    pub(crate) gap_after: i32,
}

/// One typeset row: a rich line plus its chrome. Crate-shared: the Feed
/// widget caches these per item/block (backlog 0100) — ONE row recipe,
/// so a feed item and a MarkdownView can never typeset differently.
pub(crate) struct Row {
    pub(crate) line: RichLine,
    pub(crate) indent: i32,
    /// Full-width ground override (code fences).
    pub(crate) ground: Option<Rgba>,
    /// Leading quote bar.
    pub(crate) quote: bool,
    /// A rule row (`---` and the level-1 underline) and its resolved
    /// ink, width and trailing gap. `None` = not a rule row.
    pub(crate) rule: Option<RuleRow>,
    /// One mosaic slice of an in-flow image (0144): when set, the row
    /// paints image cells instead of `line` (which stays empty).
    pub(crate) image: Option<imageflow::MdImageSlice>,
    /// One row of a fence claimed by a [`FenceBlock`]: the row paints
    /// through the block instead of as code.
    pub(crate) fence: Option<FenceSlice>,
}

/// The width-keyed typeset cache one `MarkdownView` element shares
/// between its intrinsic measure and its draw closure: `(width, rows)`
/// of the last layout; either side recomputes on a width change.
type TypesetCache = Rc<RefCell<Option<(i32, Vec<Row>)>>>;

impl Row {
    pub(crate) fn plain(line: RichLine) -> Row {
        Row {
            line,
            indent: 0,
            ground: None,
            quote: false,
            rule: None,
            image: None,
            fence: None,
        }
    }
}

/// The typeset markdown document surface: headings, inline styles, GFM
/// tables with alignment, task lists, quotes, fences, in-flow images —
/// the reader vocabulary in one widget.
///
/// It typesets at draw time (cached per width) and carries an INTRINSIC
/// MEASURE (wave 13): height answers as the typeset row count at the
/// offered width, so `Scroll::new(view)` scrolls the real document out
/// of the box and content-sized panels hug it — the scrolling markdown
/// pane is a one-liner:
///
/// ```ignore
/// Scroll::new(MarkdownView::new(doc).view(cx)).view(cx)
/// ```
///
/// [`scroll_offset`](MarkdownView::scroll_offset) remains the
/// app-managed row-offset door (transcript tails, TOC jumps against
/// [`MarkdownView::rows`] — the same fold, so clamps never drift).
/// [`outline_rows`](MarkdownView::outline_rows) feeds a TOC;
/// [`find`](MarkdownView::find) powers search highlights — the reader
/// example composes all of it. The canonical build is `.view(cx)`; see
/// the [module docs](crate::widgets::markdown). Very long documents
/// paint whole-extent under a `Scroll` (cells clip per-frame); for
/// unbounded transcripts prefer [`Feed`](super::Feed), which
/// virtualizes rows.
pub struct MarkdownView {
    /// Claimant for fenced blocks (`fence_block`).
    fence: Option<Rc<dyn FenceBlock>>,
    source: String,
    scroll_offset: i32,
    layout: Option<LayoutStyle>,
    /// Search-highlight overlay (0148): matches from [`MarkdownView::find`]
    /// at the SAME width the element renders at, painted non-destructively
    /// over the typeset rows. Empty = zero extra work at draw.
    highlights: Vec<MdSearchMatch>,
    current_match: Option<usize>,
    rule: MdRuleStyle,
}

impl MarkdownView {
    pub fn new(source: impl Into<String>) -> MarkdownView {
        MarkdownView {
            fence: None,
            source: source.into(),
            scroll_offset: 0,
            layout: None,
            highlights: Vec::new(),
            current_match: None,
            rule: MdRuleStyle::default(),
        }
    }

    /// Set the `---` policy: its ink, its width and what it costs in
    /// rows ([`MdRuleStyle`]). Defaults reproduce every earlier release
    /// exactly, so this changes nothing until you set it.
    ///
    /// Row positions move with it. Pair it with
    /// [`MarkdownView::rows_ruled`] and friends rather than the
    /// default-fold statics, or a scroll clamp and a TOC jump will
    /// drift from what is painted.
    pub fn rule_style(mut self, rule: MdRuleStyle) -> MarkdownView {
        self.rule = rule;
        self
    }

    /// Let `block` claim fenced code blocks it recognizes — a mermaid
    /// diagram, a chart, anything — so they render INSIDE the
    /// document instead of as code. Fences it declines are unchanged.
    ///
    /// The engine ships no diagram languages; this is how a sibling
    /// crate brings one without the core knowing it exists. See
    /// [`FenceBlock`].
    pub fn fence_block(mut self, block: Rc<dyn FenceBlock>) -> MarkdownView {
        self.fence = Some(block);
        self
    }

    /// First visible typeset row (app-managed scrolling).
    /// Scrolling this widget yourself gets you KEYBOARD scrolling and
    /// nothing else — the wheel, PgUp/PgDn, Home/End and a draggable
    /// scrollbar thumb all live in [`Scroll`](crate::widgets::Scroll),
    /// which measures this widget and moves it for you:
    /// `Scroll::new(view.view(cx))`. Reach for this setter only when
    /// the app genuinely owns the offset (a synced split view, a
    /// replay).
    pub fn scroll_offset(mut self, rows: i32) -> MarkdownView {
        self.scroll_offset = rows.max(0);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> MarkdownView {
        self.layout = Some(layout);
        self
    }

    /// Overlay search matches (0148): `matches` from [`MarkdownView::find`]
    /// computed at the width this element will draw at; `current` indexes
    /// into `matches` for the distinct current-match treatment. Painted as
    /// a style patch AFTER the rows — glyphs stay, tones change
    /// (`selection_bg`/`selection_fg`; the current match adds BOLD +
    /// UNDERLINE — the token set has no dedicated search tone, documented
    /// in the 0148 report).
    pub fn highlights(mut self, matches: Vec<MdSearchMatch>, current: Option<usize>) -> Self {
        self.highlights = matches;
        self.current_match = current;
        self
    }

    /// Typeset row count at `width` — the scroll clamp (same fold as the
    /// renderer, so the clamp can never drift from the pixels).
    pub fn rows(source: &str, t: &TokenSet, width: i32) -> usize {
        Self::rows_ruled(source, t, width, MdRuleStyle::default())
    }

    /// [`MarkdownView::rows`] for a view built with
    /// [`MarkdownView::rule_style`] — a `---` policy changes the row
    /// count, so the clamp has to be taken through the SAME policy the
    /// view renders under.
    pub fn rows_ruled(source: &str, t: &TokenSet, width: i32, rule: MdRuleStyle) -> usize {
        doc::layout_doc_ruled(source, t, width, rule).rows.len()
    }

    /// Heading outline `(level, text)` — table-of-contents material.
    /// See [`MarkdownView::outline_rows`] for anchor ids + typeset row
    /// positions (0146).
    pub fn outline(source: &str, t: &TokenSet) -> Vec<(u8, String)> {
        md::parse(source, &md_styles(t))
            .iter()
            .filter_map(|b| match b {
                Block::Heading { level, content } => Some((*level, content.plain())),
                _ => None,
            })
            .collect()
    }

    /// The document outline with TYPESET ROW positions (0146): each
    /// heading paired with the row its text starts at when the document
    /// is laid out at `width` — the row to scroll to for a TOC jump.
    /// Anchor ids are GitHub-compatible and deduplicated
    /// ([`md::outline`]); rows come from the SAME fold the renderer
    /// draws, so a jump can never drift from the pixels.
    pub fn outline_rows(source: &str, t: &TokenSet, width: i32) -> Vec<OutlineEntry> {
        Self::outline_rows_ruled(source, t, width, MdRuleStyle::default())
    }

    /// [`MarkdownView::outline_rows`] under a `---` policy.
    pub fn outline_rows_ruled(
        source: &str,
        t: &TokenSet,
        width: i32,
        rule: MdRuleStyle,
    ) -> Vec<OutlineEntry> {
        doc::outline_rows_ruled(source, t, width, rule)
    }

    /// Resolve an intra-document anchor (`#getting-started`, leading
    /// `#` optional) to the typeset row of its heading at `width` —
    /// `[text](#anchor)` link targets against [`md::outline`] ids.
    pub fn resolve_anchor(source: &str, t: &TokenSet, width: i32, anchor: &str) -> Option<usize> {
        Self::resolve_anchor_ruled(source, t, width, anchor, MdRuleStyle::default())
    }

    /// [`MarkdownView::resolve_anchor`] under a `---` policy.
    pub fn resolve_anchor_ruled(
        source: &str,
        t: &TokenSet,
        width: i32,
        anchor: &str,
        rule: MdRuleStyle,
    ) -> Option<usize> {
        let want = anchor.strip_prefix('#').unwrap_or(anchor);
        doc::outline_rows_ruled(source, t, width, rule)
            .into_iter()
            .find(|e| e.heading.anchor_id == want)
            .map(|e| e.row)
    }

    /// Find `query` in the TYPESET text at `width` (0148): literal
    /// match, whole-fragment scope (matches never span wrapped rows —
    /// they live in what the eye sees). `case_insensitive` folds via
    /// Unicode lowercasing. Empty query = no matches, no work. Feed the
    /// result to [`MarkdownView::highlights`] and scroll to
    /// `matches[i].row`.
    pub fn find(
        source: &str,
        t: &TokenSet,
        width: i32,
        query: &str,
        case_insensitive: bool,
    ) -> Vec<MdSearchMatch> {
        Self::find_ruled(
            source,
            t,
            width,
            query,
            case_insensitive,
            MdRuleStyle::default(),
        )
    }

    /// [`MarkdownView::find`] under a `---` policy: matches carry ROW
    /// positions, which the policy moves.
    pub fn find_ruled(
        source: &str,
        t: &TokenSet,
        width: i32,
        query: &str,
        case_insensitive: bool,
        rule: MdRuleStyle,
    ) -> Vec<MdSearchMatch> {
        search::find_in_rows(
            &doc::layout_doc_ruled(source, t, width, rule).rows,
            query,
            case_insensitive,
        )
    }

    /// Canonical one-call build: tokens resolve from the app's theme
    /// context (a tracked read — building inside a `dyn_view`
    /// re-renders on theme switch). `element(&tokens)` remains the
    /// explicit-theming door.
    pub fn view(self, cx: crate::reactive::Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(&t).build()
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let tokens = *t;
        let offset = self.scroll_offset as usize;
        let source = self.source;
        let highlights = self.highlights;
        let current = self.current_match;
        // basis 0 beside grow: in a definite flex parent the view takes
        // LEFTOVER space exactly as before the measure landed — the
        // intrinsic height must never become overflow pressure that
        // crushes fixed siblings (the 0240 modal-overflow class; same
        // default as `Scroll`).
        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .grow(1.0)
                .basis(crate::layout::Dimension::Cells(0))
        });
        // ONE width-keyed typeset cache shared by the intrinsic measure
        // and the draw: whichever runs first at a width pays the
        // layout, the other reuses it (measure runs during solving,
        // draw during paint — never concurrently).
        let cache: TypesetCache = Rc::new(RefCell::new(None));
        let measure_cache = Rc::clone(&cache);
        let measure_source = source.clone();
        let draw_fence = self.fence.clone();
        let measure_fence = draw_fence.clone();
        let rule = self.rule;
        Element::new()
            .style(layout)
            // The measure seam (wave 13, the "doesn't scroll" fix): an
            // Auto-sized axis sees the TYPESET extent — height is the
            // row count at the offered width (the same fold the draw
            // renders, so a Scroll clamp can never drift from the
            // pixels), width is the widest typeset text row (rules and
            // image mosaics follow the granted rect instead). With
            // this, `Scroll::new(view)` measures the real document and
            // content-sized panels hug it — no `content_size` hint, no
            // app-managed offset needed.
            .measure(move |avail| {
                if avail.w <= 1 {
                    return crate::base::Size::ZERO;
                }
                let mut slot = measure_cache.borrow_mut();
                let rows = match &mut *slot {
                    Some((w, rows)) if *w == avail.w => rows,
                    slot => {
                        let rows = doc::layout_doc_all(
                            &measure_source,
                            &tokens,
                            avail.w,
                            measure_fence.clone(),
                            rule,
                        )
                        .rows;
                        &mut slot.insert((avail.w, rows)).1
                    }
                };
                let widest = rows
                    .iter()
                    .map(|r| r.indent + r.line.width())
                    .max()
                    .unwrap_or(0);
                crate::base::Size::new(widest.min(avail.w), rows.len() as i32)
            })
            .draw(move |canvas, rect| {
                if rect.w <= 1 || rect.h <= 0 {
                    return;
                }
                let mut slot = cache.borrow_mut();
                let rows = match &mut *slot {
                    Some((w, rows)) if *w == rect.w => rows,
                    slot => {
                        let rows =
                            doc::layout_doc_all(&source, &tokens, rect.w, draw_fence.clone(), rule)
                                .rows;
                        &mut slot.insert((rect.w, rows)).1
                    }
                };
                let offset = offset.min(rows.len().saturating_sub(1));
                draw_rows(canvas, rect, &tokens, &rows[offset..]);
                if !highlights.is_empty() {
                    search::draw_highlights(
                        canvas,
                        rect,
                        &tokens,
                        rows,
                        offset,
                        &highlights,
                        current,
                    );
                }
            })
    }
}

/// The markdown span vocabulary in theme tokens. `base` deliberately
/// carries NO fg: parse_inline stamps `base` onto every plain span, and
/// an explicit fg there would defeat block-level recoloring (blockquotes
/// dim to `text_muted`); fg-less spans inherit at draw time instead.
/// Crate-shared with the Feed widget (one mapping, no drift).
pub(crate) fn md_styles(t: &TokenSet) -> MdStyles {
    MdStyles {
        base: Style::EMPTY,
        bold: Style::new().attrs(Attrs::BOLD),
        italic: Style::new().attrs(Attrs::ITALIC),
        // Inline code: a raised chip, body ink.
        code: Style::new().fg(t.text).bg(t.surface_raised),
        link: Style::new().fg(t.link).attrs(Attrs::UNDERLINE),
        heading: Style::new().attrs(Attrs::BOLD),
    }
}

/// Render a fenced code block as something OTHER than code.
///
/// The engine knows nothing about mermaid, dot, or any other diagram
/// language — and must not. This is the seam that lets a sibling
/// crate (or an app) claim a fence by its info string and paint cells
/// where the code would have gone, INSIDE the document: one scroll
/// surface, one outline, one search index.
///
/// Both calls are pure with respect to the document. An implementation
/// that renders something expensive should cache keyed by
/// `(source, width)` — `measure` and `draw_row` are called for the same
/// block many times as the reader scrolls.
///
/// ```no_run
/// use abstracttui::base::{Point, Rgba};
/// use abstracttui::theme::TokenSet;
/// use abstracttui::ui::StyledCanvas;
/// use abstracttui::widgets::{FenceBlock, MarkdownView};
/// use std::rc::Rc;
///
/// struct Banner;
/// impl FenceBlock for Banner {
///     fn measure(&self, lang: &str, _source: &str, _width: i32) -> Option<i32> {
///         (lang == "banner").then_some(1)
///     }
///     fn draw_row(
///         &self,
///         _lang: &str,
///         source: &str,
///         _row: i32,
///         at: Point,
///         width: i32,
///         t: &TokenSet,
///         canvas: &mut dyn StyledCanvas,
///     ) {
///         let text: String = source.chars().take(width.max(0) as usize).collect();
///         canvas.print(at, &text, t.text, Rgba::TRANSPARENT);
///     }
/// }
///
/// let view = MarkdownView::new("```banner\nhello\n```").fence_block(Rc::new(Banner));
/// # let _ = view;
/// ```
pub trait FenceBlock {
    /// Rows this block needs at `width`, or `None` to decline it —
    /// declining renders the fence as code, exactly as before.
    fn measure(&self, lang: &str, source: &str, width: i32) -> Option<i32>;

    /// Paint row `row` (0-based within the block) at `at`, `width`
    /// cells wide, in the document's own tokens — a block is content,
    /// and content wears the theme like everything else. Only rows the
    /// reader can see are drawn.
    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &self,
        lang: &str,
        source: &str,
        row: i32,
        at: Point,
        width: i32,
        tokens: &TokenSet,
        canvas: &mut dyn StyledCanvas,
    );
}

/// One row of a [`FenceBlock`]-rendered fence, the way
/// [`imageflow::MdImageSlice`] is one row of an in-flow image.
#[derive(Clone)]
pub(crate) struct FenceSlice {
    pub(crate) block: Rc<dyn FenceBlock>,
    pub(crate) lang: String,
    pub(crate) source: Rc<String>,
    pub(crate) row: i32,
    pub(crate) width: i32,
}

/// The block -> typeset-rows recipe, crate-shared (backlog 0100): the
/// Feed widget caches rows per item/block through this same fold, so a
/// feed item and a `MarkdownView` can never typeset differently.
pub(crate) struct BlockTypesetter {
    styles: MdStyles,
    /// Set by `MarkdownView::fence_block`: the claimant for fenced
    /// blocks it recognizes.
    fence: Option<Rc<dyn FenceBlock>>,
    lexer: CLikeLexer,
    diff: DiffLexer,
    json: JsonLexer,
    yaml: YamlLexer,
    code_base: Style,
    t: TokenSet,
    /// The `---` policy (`MarkdownView::rule_style` / `FeedState::rule_style`).
    /// Lives HERE rather than on either widget so both consumers of
    /// this recipe get the same rule or neither does.
    rule: MdRuleStyle,
}

impl BlockTypesetter {
    pub(crate) fn new(t: &TokenSet) -> BlockTypesetter {
        BlockTypesetter {
            styles: md_styles(t),
            fence: None,
            lexer: CLikeLexer::default(),
            diff: DiffLexer::new(),
            json: JsonLexer::new(),
            yaml: YamlLexer::new(),
            code_base: Style::new().fg(t.text),
            t: *t,
            rule: MdRuleStyle::default(),
        }
    }

    /// Install the `---` policy.
    pub(crate) fn with_rule_style(mut self, rule: MdRuleStyle) -> BlockTypesetter {
        self.rule = rule;
        self
    }

    /// The `---` policy in force (the stream/segment boundaries in the
    /// Feed spend a rule's leading space themselves — `push_block`
    /// cannot see across a segment).
    pub(crate) fn rule_style(&self) -> MdRuleStyle {
        self.rule
    }

    /// How many blank rows the next block's separator emits after
    /// `out`. THE definition — `push_block` spends it and the doc fold
    /// reads it to locate a heading's text row, so the two cannot
    /// disagree. They used to agree only by a comment, which a rule's
    /// `space_after` would have quietly falsified.
    pub(crate) fn separator_rows(&self, out: &[Row], separate: bool) -> usize {
        if !separate || out.is_empty() {
            return 0;
        }
        // A rule owns both sides of its own gap: after one, the
        // separator is ITS `space_after`, not this block's usual single
        // row. Spending it HERE rather than from the rule is what keeps
        // a document that ENDS in a rule ending at the rule, exactly as
        // it did before the policy existed.
        out.last()
            .and_then(|r| r.rule)
            .map_or(1, |r| r.gap_after.max(0) as usize)
    }

    /// Install the fenced-block claimant.
    pub(crate) fn with_fence_block(mut self, fence: Option<Rc<dyn FenceBlock>>) -> BlockTypesetter {
        self.fence = fence;
        self
    }

    /// The span styles matching this typesetter's tokens — parse
    /// sources with these so inline patches line up.
    pub(crate) fn styles(&self) -> &MdStyles {
        &self.styles
    }

    /// Append `block`'s typeset rows to `out` at `width`. `separate`
    /// applies the document spacing policy: one blank row before every
    /// non-list block when `out` is not empty (list items stack tight).
    pub(crate) fn push_block(&self, out: &mut Vec<Row>, block: &Block, width: i32, separate: bool) {
        let t = &self.t;
        let blank = |rows: &mut Vec<Row>| {
            for _ in 0..self.separator_rows(rows, separate) {
                rows.push(Row::plain(RichLine::new()));
            }
        };
        match block {
            Block::Heading { level, content } => {
                blank(out);
                let ink = match level {
                    1 => Style::new().fg(t.accent).attrs(Attrs::BOLD),
                    2 => Style::new().fg(t.accent),
                    _ => Style::new().fg(t.text).attrs(Attrs::BOLD),
                };
                let mut line = RichLine::new();
                // Level legibility from H3 down (wave 13, ported from
                // mdpad): L1/L2 are told apart by ink and the L1
                // underline, but L3..L6 all render body+BOLD — a faint
                // hash prefix keeps the depth readable exactly where
                // the ink stops differentiating.
                if *level >= 3 {
                    line.push(Span::new(
                        format!("{} ", "#".repeat(*level as usize)),
                        Style::new().fg(t.text_faint),
                    ));
                }
                for span in &content.spans {
                    line.push(Span::new(span.text.clone(), span.style.merge(ink)));
                }
                out.push(Row::plain(line));
                if *level == 1 {
                    // The heading underline is a DIFFERENT block kind
                    // that happens to paint the same chrome, so it
                    // keeps the default style rather than the caller's
                    // `---` policy: one setter must not silently
                    // restyle two block kinds (pinned by
                    // `rule_policy_leaves_the_h1_underline_alone`).
                    out.push(Row {
                        line: RichLine::new(),
                        indent: 0,
                        ground: None,
                        quote: false,
                        rule: Some(MdRuleStyle::default().resolve(t, width)),
                        image: None,
                        fence: None,
                    });
                }
            }
            Block::Paragraph(line) => {
                blank(out);
                for wrapped in wrap_line(line.clone(), width) {
                    out.push(Row::plain(wrapped));
                }
            }
            Block::ListItem {
                depth,
                marker,
                content,
            } => {
                let indent = 2 + *depth as i32 * 2;
                let mut line = RichLine::new();
                let marker_text = match marker {
                    Marker::Bullet => "• ".to_string(),
                    Marker::Number(n) => format!("{n}. "),
                };
                line.push(Span::new(marker_text, Style::new().fg(t.accent_alt)));
                for span in &content.spans {
                    line.push(span.clone());
                }
                for (i, wrapped) in wrap_line(line, width - indent).into_iter().enumerate() {
                    out.push(Row {
                        line: wrapped,
                        // Continuation rows hang past the marker.
                        indent: indent + if i > 0 { 2 } else { 0 },
                        ground: None,
                        quote: false,
                        rule: None,
                        image: None,
                        fence: None,
                    });
                }
            }
            Block::Blockquote(line) => {
                blank(out);
                let mut muted = RichLine::new();
                for span in &line.spans {
                    // Quote prose dims; spans with their OWN ink (links,
                    // inline code) keep it.
                    let style = if span.style.fg.is_none() {
                        span.style.fg(t.text_muted)
                    } else {
                        span.style
                    };
                    muted.push(Span::new(span.text.clone(), style));
                }
                for wrapped in wrap_line(muted, width - 2) {
                    out.push(Row {
                        line: wrapped,
                        indent: 2,
                        ground: None,
                        quote: true,
                        rule: None,
                        image: None,
                        fence: None,
                    });
                }
            }
            Block::CodeFence { lang, lines } => {
                blank(out);
                // A claimed fence renders through its block: the code
                // never reaches the screen, and the rows live in this
                // document's scroll surface like any other rows.
                if let Some(fence) = &self.fence {
                    let source = lines.join("\n");
                    if let Some(rows) = fence.measure(lang, &source, width) {
                        let source = Rc::new(source);
                        for row in 0..rows.max(0) {
                            out.push(Row {
                                line: RichLine::new(),
                                indent: 0,
                                ground: None,
                                quote: false,
                                rule: None,
                                image: None,
                                fence: Some(FenceSlice {
                                    block: Rc::clone(fence),
                                    lang: lang.clone(),
                                    source: Rc::clone(&source),
                                    row,
                                    width,
                                }),
                            });
                        }
                        return;
                    }
                }
                // Fence labels route the lexer (0140's diff slice +
                // wave 13's data slice): ```diff / ```patch tint
                // through the diff mapping, ```json / ```yaml (and
                // dialects) through the data mapping; every other
                // label keeps the C-like lexer as before.
                let diff_fence = DiffLexer::matches_lang(lang);
                let json_fence = JsonLexer::matches_lang(lang);
                let yaml_fence = YamlLexer::matches_lang(lang);
                for code_line in lines {
                    let rich = if diff_fence {
                        diff_rich_line(code_line, &self.diff, self.code_base, t)
                    } else if json_fence {
                        data_rich_line(code_line, self.json.spans(code_line), self.code_base, t)
                    } else if yaml_fence {
                        data_rich_line(code_line, self.yaml.spans(code_line), self.code_base, t)
                    } else {
                        RichLine::from_highlighted(code_line, &self.lexer, self.code_base, |k| {
                            Style::new().fg(code_token_color(k, t))
                        })
                    };
                    let mut padded = RichLine::new();
                    padded.push(Span::new(" ", self.code_base));
                    for span in rich.spans {
                        padded.push(span);
                    }
                    out.push(Row {
                        line: padded,
                        indent: 1,
                        ground: Some(t.surface_raised),
                        quote: false,
                        rule: None,
                        image: None,
                        fence: None,
                    });
                }
            }
            Block::Rule => {
                // The rule's own leading space, not the document's:
                // `space_before` replaces the generic separator so the
                // spacing axis is reachable from the same policy as ink
                // and width.
                if separate && !out.is_empty() {
                    for _ in 0..self.rule.space_before.max(0) {
                        out.push(Row::plain(RichLine::new()));
                    }
                }
                out.push(Row {
                    line: RichLine::new(),
                    indent: 0,
                    ground: None,
                    quote: false,
                    rule: Some(self.rule.resolve(t, width)),
                    image: None,
                    fence: None,
                });
            }
        }
    }
}

fn wrap_line(line: RichLine, width: i32) -> Vec<RichLine> {
    RichText::from_lines(vec![line]).wrap(width.max(4)).lines
}

/// Paint typeset rows into `rect`, one row per line from `rect.y` down,
/// clipped at `rect.bottom()`. Crate-shared with the Feed widget.
pub(crate) fn draw_rows(
    canvas: &mut dyn StyledCanvas,
    rect: crate::base::Rect,
    t: &TokenSet,
    rows: &[Row],
) {
    for (i, row) in rows.iter().enumerate() {
        let y = rect.y + i as i32;
        if y >= rect.bottom() {
            break;
        }
        if let Some(rule) = row.rule {
            // Ink and extent travel ON the row, resolved at typeset
            // against the live theme and the measure the block was set
            // at — the painter has neither, and a rect is not a measure.
            let start = rect.x + rule.indent.clamp(0, rect.w.max(0));
            let end = match rule.extent {
                Some(cells) => (start + cells).min(rect.right()),
                None => rect.right(),
            };
            for x in start..end {
                canvas.put(Point::new(x, y), '─', rule.ink, Rgba::TRANSPARENT);
            }
            continue;
        }
        // In-flow image slice (0144): decode-on-first-draw, mosaic
        // cells only. The row's `line` is empty by construction.
        if let Some(slice) = &row.image {
            imageflow::draw_image_row(canvas, rect, y, t, row.indent, slice);
            continue;
        }
        if let Some(slice) = &row.fence {
            slice.block.draw_row(
                &slice.lang,
                &slice.source,
                slice.row,
                Point::new(rect.x, y),
                slice.width.min(rect.w),
                t,
                canvas,
            );
            continue;
        }
        if let Some(ground) = row.ground {
            canvas.fill(
                crate::base::Rect::new(rect.x, y, rect.w, 1),
                ' ',
                t.text,
                ground,
            );
        }
        if row.quote {
            canvas.put(Point::new(rect.x, y), '▎', t.border, Rgba::TRANSPARENT);
        }
        let mut x = rect.x + row.indent;
        for span in &row.line.spans {
            let style = if span.style.fg.is_none() {
                span.style.fg(t.text)
            } else {
                span.style
            };
            x += crate::widgets::richtext::print_span_clipped(
                canvas,
                x,
                y,
                rect.right(),
                &span.text,
                &style,
            );
            if x >= rect.right() {
                break;
            }
        }
    }
}

/// Scroll/panel composition tests (wave 13): the measure seam.
#[cfg(test)]
#[path = "markdown_scroll_tests.rs"]
mod scroll_composition_tests;

#[cfg(test)]
#[path = "markdown_view_tests.rs"]
mod tests;

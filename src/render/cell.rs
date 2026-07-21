//! Cell, glyph and attribute model — the atomic unit of terminal content.
//!
//! Layout rationale (see docs/design/render.md §2.1): a `Cell` is exactly
//! 28 bytes (12 glyph + 4 fg + 4 bg + 4 underline color + 2 attrs + 2
//! link), inside the sanctioned 24–32 budget; a 200x60 frame stays under
//! 340 KiB. The glyph stores up to 10 UTF-8 bytes inline, which covers
//! every single codepoint (max 4 bytes), combining stacks, emoji
//! presentation pairs (base + VS16, 7 bytes), flags (8 bytes) and
//! skin-tone pairs (8 bytes). Only rare long clusters (ZWJ families, deep
//! mark stacks) spill into a per-`Surface` [`GlyphPool`] — notcurses'
//! egcpool idea with a wider inline window, because modern UI text hits
//! emoji far more often than 2020-era terminal content did.
//!
//! The underline color is a plain `Rgba` value, not an interned id: colors
//! are values (unlike link URIs), and a second id space would recreate the
//! ambient-pool ambiguity RT1-4 exists to kill.

use crate::base::Rgba;
use crate::text;

pub use super::attrs::Attrs;

/// Maximum UTF-8 bytes stored inline in a [`Glyph`].
pub const GLYPH_INLINE_CAP: usize = 10;

/// Interned-entry cap for one surface's glyph pool (RT1-14). Beyond it,
/// new long clusters degrade to the visible U+FFFD replacement and
/// [`GlyphPool::dropped`] counts them — bounded memory and a bounded
/// dedup scan beat silent unbounded growth under hostile churn (a log
/// viewer streaming unique ZWJ emoji). Compaction on `Surface::clear` is
/// the documented recovery hook.
pub const GLYPH_POOL_CAP: usize = 4096;

/// Shown when pool capacity is exhausted; a visible, labeled degradation
/// rather than silent truncation of a cluster.
const REPLACEMENT: &str = "\u{FFFD}";

const TAG_CONTINUATION: u8 = 0xFE;
const TAG_POOLED: u8 = 0xFF;

// ---------------------------------------------------------------------------
// GlyphPool
// ---------------------------------------------------------------------------

/// Per-surface interned storage for grapheme clusters longer than
/// [`GLYPH_INLINE_CAP`] bytes. Ids are indices and therefore only valid
/// against the pool that minted them; cross-surface copies must re-intern
/// (see `Surface::adopt_cell`).
///
/// Dedup is a linear scan: pools hold only *unique long clusters*, which in
/// practice is a handful of ZWJ emoji, so a search beats a hash table's
/// memory and hashing cost. [`GLYPH_POOL_CAP`] bounds both the memory and
/// the scan under hostile churn (RT1-14).
#[derive(Default, Clone, Debug)]
pub struct GlyphPool {
    entries: Vec<Box<str>>,
    /// Clusters refused because the pool was full — the labeled
    /// degradation counter (each refusal rendered a visible U+FFFD).
    dropped: u32,
}

impl GlyphPool {
    /// Interns a cluster and returns its id, or `None` at the cap (the
    /// caller degrades to U+FFFD, visibly, and the drop is counted).
    pub fn intern(&mut self, cluster: &str) -> Option<u16> {
        if let Some(i) = self.entries.iter().position(|e| &**e == cluster) {
            return Some(i as u16);
        }
        if self.entries.len() >= GLYPH_POOL_CAP {
            self.dropped = self.dropped.saturating_add(1);
            return None;
        }
        self.entries.push(cluster.into());
        Some((self.entries.len() - 1) as u16)
    }

    /// Resolves a pooled id to its cluster text.
    pub fn get(&self, id: u16) -> Option<&str> {
        self.entries.get(id as usize).map(|s| &**s)
    }

    /// Drops all entries (and forgives counted drops). Only safe when
    /// every pooled glyph is being discarded too (full-surface fill) —
    /// this is the documented compaction hook.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.dropped = 0;
    }

    /// Interned entry count (deduplicated).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing has spilled to the pool yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many interning requests were refused at the cap since the last
    /// [`GlyphPool::clear`]. Nonzero means the screen shows U+FFFD where
    /// unique long clusters overflowed the budget.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }
}

// ---------------------------------------------------------------------------
// Glyph
// ---------------------------------------------------------------------------

/// One grapheme cluster (or the continuation marker of a wide one), with a
/// cached display width so diff/present never re-measure (notcurses lesson).
///
/// Derived equality is only meaningful *within one surface*: pooled ids are
/// pool-relative. Cross-surface comparison goes through the crate-private
/// `Glyph::content_eq` (the diff uses it; user code compares rendered
/// text via [`Surface::glyph_str`](super::surface::Surface::glyph_str)).
/// Within a surface, pool dedup makes id equality equal content equality.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Glyph {
    data: [u8; GLYPH_INLINE_CAP],
    /// `0..=10`: inline byte length (0 = empty). `0xFE`: continuation.
    /// `0xFF`: pooled; id is little-endian in `data[0..2]`.
    len_or_tag: u8,
    /// Display width in columns (0 for continuation, 1 or 2 otherwise).
    width: u8,
}

impl Glyph {
    /// The transparent glyph: compositing sees through it to lower layers;
    /// the presenter draws it as a space over the cell background. Distinct
    /// from an actual `' '`, which is opaque content that erases.
    pub const EMPTY: Glyph = Glyph {
        data: [0; GLYPH_INLINE_CAP],
        len_or_tag: 0,
        width: 1,
    };

    /// Trailing half of a wide glyph. Never constructed by callers; surface
    /// write paths manage pairing invariants.
    pub const CONTINUATION: Glyph = Glyph {
        data: [0; GLYPH_INLINE_CAP],
        len_or_tag: TAG_CONTINUATION,
        width: 0,
    };

    /// A real `' '`: opaque content that ERASES what is underneath when
    /// composited (contrast [`Glyph::EMPTY`], which lets it show).
    pub const SPACE: Glyph = {
        let mut data = [0u8; GLYPH_INLINE_CAP];
        data[0] = b' ';
        Glyph {
            data,
            len_or_tag: 1,
            width: 1,
        }
    };

    /// Builds a glyph from the *first* grapheme cluster of `s`, interning
    /// into `pool` when it does not fit inline. Returns `None` for empty
    /// input, control clusters and zero-width clusters (callers skip them —
    /// a cell must occupy at least one column).
    ///
    /// Crate-private (RT1-4): a pooled glyph is only meaningful inside the
    /// surface owning `pool`, so public construction goes through
    /// `Surface::draw_text`/`Canvas` where pool ownership is structural.
    pub(crate) fn new(s: &str, pool: &mut GlyphPool) -> Option<Glyph> {
        use unicode_segmentation::UnicodeSegmentation;
        let cluster = s.graphemes(true).next()?;
        let width = text::cluster_width(cluster);
        if width <= 0 {
            return None;
        }
        Some(Self::from_cluster_unchecked(cluster, width as u8, pool))
    }

    /// `cluster` is already segmented and measured. Internal fast path for
    /// `draw_text`, which iterates clusters itself.
    pub(crate) fn from_cluster_unchecked(cluster: &str, width: u8, pool: &mut GlyphPool) -> Glyph {
        let bytes = cluster.as_bytes();
        if bytes.len() <= GLYPH_INLINE_CAP {
            let mut data = [0u8; GLYPH_INLINE_CAP];
            data[..bytes.len()].copy_from_slice(bytes);
            return Glyph {
                data,
                len_or_tag: bytes.len() as u8,
                width,
            };
        }
        match pool.intern(cluster) {
            Some(id) => {
                let mut data = [0u8; GLYPH_INLINE_CAP];
                data[0..2].copy_from_slice(&id.to_le_bytes());
                Glyph {
                    data,
                    len_or_tag: TAG_POOLED,
                    width,
                }
            }
            // Pool exhausted: visible degradation beats silent data loss.
            None => {
                let mut data = [0u8; GLYPH_INLINE_CAP];
                data[..REPLACEMENT.len()].copy_from_slice(REPLACEMENT.as_bytes());
                Glyph {
                    data,
                    len_or_tag: REPLACEMENT.len() as u8,
                    width: 1,
                }
            }
        }
    }

    /// True for [`Glyph::EMPTY`] (the see-through glyph).
    pub const fn is_empty(self) -> bool {
        self.len_or_tag == 0
    }

    /// True for the trailing half of a wide pair.
    pub const fn is_continuation(self) -> bool {
        self.len_or_tag == TAG_CONTINUATION
    }

    /// True when the cluster spilled to the owning surface's pool (rare:
    /// clusters longer than [`GLYPH_INLINE_CAP`] bytes).
    pub const fn is_pooled(self) -> bool {
        self.len_or_tag == TAG_POOLED
    }

    /// Display width in columns: 0 for a continuation, 1 or 2 otherwise
    /// (cached at construction; never re-measured).
    pub const fn width(self) -> i32 {
        self.width as i32
    }

    pub(crate) fn pool_id(self) -> u16 {
        u16::from_le_bytes([self.data[0], self.data[1]])
    }

    /// Resolves the cluster text. Empty and continuation glyphs resolve to
    /// `""`; the presenter substitutes a space for empties.
    ///
    /// Crate-private on purpose (RT1-4): resolving a glyph against the
    /// wrong pool is exactly the ambient-ownership bug the finding names.
    /// The public path is `Surface::glyph_str`, which can only resolve
    /// through the owning surface's pool.
    pub(crate) fn as_str<'a>(&'a self, pool: &'a GlyphPool) -> &'a str {
        match self.len_or_tag {
            0 | TAG_CONTINUATION => "",
            TAG_POOLED => pool.get(self.pool_id()).unwrap_or(REPLACEMENT),
            n => std::str::from_utf8(&self.data[..n as usize]).unwrap_or(REPLACEMENT),
        }
    }

    /// Content equality across two (possibly different) pools. Inline and
    /// pooled glyphs can never be content-equal because spill happens only
    /// past the inline capacity. Crate-private: callers must name both
    /// owning pools explicitly (diff does), never guess.
    pub(crate) fn content_eq(
        &self,
        other: &Glyph,
        own_pool: &GlyphPool,
        other_pool: &GlyphPool,
    ) -> bool {
        match (self.is_pooled(), other.is_pooled()) {
            (false, false) => self == other,
            (true, true) => {
                self.width == other.width && self.as_str(own_pool) == other.as_str(other_pool)
            }
            _ => false,
        }
    }
}

impl std::fmt::Debug for Glyph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.len_or_tag {
            0 => write!(f, "Glyph(EMPTY)"),
            TAG_CONTINUATION => write!(f, "Glyph(CONT)"),
            TAG_POOLED => write!(f, "Glyph(pool#{} w{})", self.pool_id(), self.width),
            n => {
                let s = std::str::from_utf8(&self.data[..n as usize]).unwrap_or("<bad>");
                write!(f, "Glyph({s:?} w{})", self.width)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cell
// ---------------------------------------------------------------------------

/// One screen cell. Colors are RGBA; alpha 0 means "terminal default color"
/// once a frame reaches the presenter, and "see-through" while compositing.
/// `ul` is the underline color (SGR 58/59; alpha 0 = default, i.e. follow
/// fg). `link` is a surface-local hyperlink id (0 = none), resolved
/// through `Surface::link_uri`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Cell {
    /// The occupying grapheme cluster (or EMPTY / a continuation marker).
    pub glyph: Glyph,
    /// Ink color. Alpha 0 = terminal default foreground.
    pub fg: Rgba,
    /// Ground color. Alpha 0 = terminal default background.
    pub bg: Rgba,
    /// Underline color (only visible with UNDERLINE/UNDERCURL set).
    pub ul: Rgba,
    /// Text attributes (bold, italic, ...).
    pub attrs: Attrs,
    /// Surface-local hyperlink id; 0 = no link.
    pub link: u16,
}

// The layout budget this module is built around (12+4+4+4+2+2, align 2 —
// within the 24..=32 sanction); a change here is a cache-density
// regression and must be a deliberate decision.
const _: () = assert!(std::mem::size_of::<Cell>() == 28);

impl Cell {
    /// The fully default cell: see-through glyph, terminal-default colors,
    /// no attributes, no link — the `fill` value for fresh surfaces.
    pub const EMPTY: Cell = Cell {
        glyph: Glyph::EMPTY,
        fg: Rgba::TRANSPARENT,
        bg: Rgba::TRANSPARENT,
        ul: Rgba::TRANSPARENT,
        attrs: Attrs::NONE,
        link: 0,
    };

    /// A cell holding `glyph` with everything else default.
    pub const fn new(glyph: Glyph) -> Cell {
        Cell {
            glyph,
            ..Cell::EMPTY
        }
    }

    /// Copy with the foreground replaced.
    pub const fn with_fg(self, fg: Rgba) -> Cell {
        Cell { fg, ..self }
    }

    /// Copy with the background replaced.
    pub const fn with_bg(self, bg: Rgba) -> Cell {
        Cell { bg, ..self }
    }

    /// Copy with the underline color replaced.
    pub const fn with_ul(self, ul: Rgba) -> Cell {
        Cell { ul, ..self }
    }

    /// Copy with the attributes replaced (not merged).
    pub const fn with_attrs(self, attrs: Attrs) -> Cell {
        Cell { attrs, ..self }
    }

    /// Copy with the hyperlink id replaced.
    pub const fn with_link(self, link: u16) -> Cell {
        Cell { link, ..self }
    }

    /// True for the trailing half of a wide pair.
    pub const fn is_continuation(&self) -> bool {
        self.glyph.is_continuation()
    }

    /// True for the leading half of a wide pair.
    pub const fn is_wide_leader(&self) -> bool {
        self.glyph.width() >= 2
    }

    /// The cell with its glyph destroyed but its style kept: what remains
    /// when half of a wide pair is clobbered. Keeping style (including the
    /// link) preserves the visual field the glyph sat in.
    pub const fn blanked(&self) -> Cell {
        Cell {
            glyph: Glyph::SPACE,
            ..*self
        }
    }

    /// The continuation cell paired with `leader`. Style is mirrored so
    /// style-only reads (selection highlight, diff) never special-case the
    /// trailing half.
    pub(crate) const fn continuation_of(leader: &Cell) -> Cell {
        Cell {
            glyph: Glyph::CONTINUATION,
            ..*leader
        }
    }
}

impl Default for Cell {
    fn default() -> Cell {
        Cell::EMPTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_is_28_bytes() {
        assert_eq!(std::mem::size_of::<Cell>(), 28);
        assert_eq!(std::mem::size_of::<Glyph>(), 12);
    }

    #[test]
    fn pool_caps_with_counted_drops() {
        let mut pool = GlyphPool::default();
        // Distinct 11-byte clusters: "0000000000" + one multibyte tail
        // won't segment as one cluster; use ASCII digits + combining mark
        // (12 bytes total, one cluster).
        let mut refused = 0u32;
        for i in 0..(GLYPH_POOL_CAP + 10) {
            let cluster = format!("{i:010}\u{0301}");
            if pool.intern(&cluster).is_none() {
                refused += 1;
            }
        }
        assert_eq!(pool.len(), GLYPH_POOL_CAP);
        assert_eq!(refused, 10);
        assert_eq!(pool.dropped(), 10);
        // Dedup of an existing entry still succeeds at the cap.
        assert!(pool.intern(&format!("{:010}\u{0301}", 0)).is_some());
        pool.clear();
        assert_eq!(pool.dropped(), 0);
    }

    #[test]
    fn glyph_inline_roundtrip() {
        let mut pool = GlyphPool::default();
        let g = Glyph::new("é", &mut pool).unwrap();
        assert!(!g.is_pooled());
        assert_eq!(g.as_str(&pool), "é");
        assert_eq!(g.width(), 1);
        assert_eq!(pool.len(), 0);

        // Emoji presentation pair fits inline (7 bytes).
        let heart = Glyph::new("❤\u{FE0F}", &mut pool).unwrap();
        assert!(!heart.is_pooled());
        assert_eq!(heart.width(), 2);
    }

    #[test]
    fn glyph_long_cluster_spills_and_dedups() {
        let mut pool = GlyphPool::default();
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦"; // 25 bytes
        assert!(family.len() > GLYPH_INLINE_CAP);
        let a = Glyph::new(family, &mut pool).unwrap();
        let b = Glyph::new(family, &mut pool).unwrap();
        assert!(a.is_pooled());
        assert_eq!(a, b, "dedup must return the same id");
        assert_eq!(pool.len(), 1);
        assert_eq!(a.as_str(&pool), family);
        assert_eq!(a.width(), 2);
    }

    #[test]
    fn glyph_rejects_control_and_zero_width() {
        let mut pool = GlyphPool::default();
        assert!(Glyph::new("\t", &mut pool).is_none());
        assert!(Glyph::new("\u{200C}", &mut pool).is_none()); // ZWNJ alone
        assert!(Glyph::new("", &mut pool).is_none());
    }

    #[test]
    fn content_eq_across_pools() {
        let mut pa = GlyphPool::default();
        let mut pb = GlyphPool::default();
        let long = "👩\u{200D}🚀🏽\u{200D}x"; // force > 10 bytes
                                              // Seed pool b so ids differ across pools.
        pb.intern("padding-entry");
        let a = Glyph::new(long, &mut pa).unwrap();
        let b = Glyph::new(long, &mut pb).unwrap();
        assert_ne!(a.pool_id(), b.pool_id());
        assert!(a.content_eq(&b, &pa, &pb));
        let space = Glyph::SPACE;
        assert!(!a.content_eq(&space, &pa, &pb));
        // Space content is not the same as the transparent EMPTY glyph.
        assert!(!space.content_eq(&Glyph::EMPTY, &pa, &pb));
    }

    #[test]
    fn attrs_ops() {
        let a = Attrs::BOLD | Attrs::UNDERLINE;
        assert!(a.contains(Attrs::BOLD));
        assert!(!a.contains(Attrs::DIM));
        assert!(a.intersects(Attrs::UNDERLINE | Attrs::DIM));
        assert_eq!((a - Attrs::BOLD).bits(), Attrs::UNDERLINE.bits());
        assert_eq!(Attrs::from_bits_truncate(0xFFFF), Attrs::ALL);
        assert_eq!(
            format!("{:?}", Attrs::BOLD | Attrs::STRIKE),
            "Attrs(BOLD|STRIKE)"
        );
    }

    #[test]
    fn blank_and_continuation_keep_style() {
        let mut pool = GlyphPool::default();
        let leader = Cell::new(Glyph::new("世", &mut pool).unwrap())
            .with_fg(Rgba::rgb(1, 2, 3))
            .with_attrs(Attrs::BOLD)
            .with_link(7);
        let cont = Cell::continuation_of(&leader);
        assert!(cont.is_continuation());
        assert_eq!(cont.fg, leader.fg);
        assert_eq!(cont.link, 7);
        let blank = leader.blanked();
        assert_eq!(blank.glyph, Glyph::SPACE);
        assert_eq!(blank.attrs, Attrs::BOLD);
    }
}

//! Text drawn several cells tall, out of the engine's own 8x16 bitmap
//! font.
//!
//! A terminal has exactly one font size and no API makes a cell taller.
//! The only way to draw a bigger letter is to spend more cells on it,
//! and the only way to spend a cell fractionally is a sub-cell mosaic
//! glyph — which this crate already does for images. So this module
//! rasterizes a string into a [`Bitmap`] and hands it to
//! [`mosaic`]: same vocabularies, same capability
//! ladder, no second encoder.
//!
//! # What is measured, and what is taste
//!
//! A reader cannot distinguish what the renderer did not distinguish.
//! So legibility here is one arithmetic question — rasterize an
//! alphabet, compare every pair of subpixel bitmaps, take the closest
//! pair — and [`closest_pair`] is that question as an API. Nothing in
//! this module claims a size is legible without being able to produce
//! the number.
//!
//! ## There is no single floor, and pretending there was one was a bug
//!
//! This module shipped with `GlyphScale::FLOOR` (4x3), a `TIGHT` step
//! below it, and a `has_margin()` that answered by comparing `cols` and
//! `rows` against the floor. Three things were wrong with that, and all
//! three are the same mistake — asserting a measurement instead of
//! taking one:
//!
//! - **The classes disagree.** Uppercase, lowercase-with-digits and
//!   icons stop reading at *different* sizes. At 2x2 in braille the
//!   closest lowercase pair is 1 subpixel apart — dead — while the
//!   closest icon pair is 5, which is a wider margin than 3x3 gives
//!   UPPERCASE. One boolean cannot say that, so [`Content`] is a
//!   parameter and there is a floor per class.
//! - **The mode matters and was ignored.** For uppercase, 4x3 measures
//!   8 subpixels in braille and 3 in quadrant. For mixed TEXT the same
//!   4x3 is Clear in braille and only Marginal in sextant — so
//!   `GlyphScale::FLOOR`, the constant named for safety, was not safe
//!   in the mode this module recommends for text. A predicate that
//!   never looked at [`MosaicMode`] was answering for a mode it had not
//!   been told.
//! - **A rectangle test refuses better scales.** `has_margin` required
//!   `cols >= 4 && rows >= 3`, so it refused 4x2 — which measures
//!   (UPPER 4, lower 2, icons 11) in braille — while offering 3x3,
//!   which measures (4, **0**, 11). 3x3 renders two lowercase
//!   characters IDENTICALLY. The crate was recommending the strictly
//!   worse of the two because it had swept rows and not columns.
//!
//! [`legibility`] replaces the predicate: it measures, at the style and
//! for the content you name, and answers [`Legibility::Collides`],
//! [`Distorted`](Legibility::Distorted),
//! [`Marginal`](Legibility::Marginal) or [`Clear`](Legibility::Clear).
//! [`smallest_clear`] runs it down a ladder of candidate scales and
//! hands back the cheapest one that clears — the "what size should I
//! use" question, answered per class and per mode rather than by a
//! constant.
//!
//! **The judgements, stated as judgements.** Zero subpixels apart is a
//! measurement: those two characters ARE the same picture. The line
//! between "marginal" and "clear" is not, and neither is the line
//! between a shape that still reads and one that does not —
//! [`MARGINAL_MAX`] and [`FIDELITY_MAX`] are chosen numbers. A caller
//! who disagrees should read [`closest_pair`] and [`least_faithful`] and
//! set their own bar rather than argue with mine.
//!
//! # Two ways to be unreadable, and the second one took a screenshot
//!
//! Pairwise distance answers *are these two characters different*. It
//! cannot answer *is either one still itself*, and those come apart
//! badly. The operator sent a picture of a row of icons at `6x2`
//! braille: `●` and `◆` measure 16 subpixels apart — emphatically
//! distinct — and both render as the same white bar. This crate's own
//! example printed `clearly apart` directly above them.
//!
//! So there is a second measurement. [`fidelity_loss`] compares what the
//! renderer draws against the same glyph drawn at its NATURAL
//! proportions in the same footprint, and [`least_faithful`] reports the
//! worst character of a class. Over [`FIDELITY_MAX`], the verdict is
//! [`Distorted`](Legibility::Distorted) whatever the pair distance says.
//!
//! # The aspect trap, and the band that closes half of it
//!
//! A terminal cell is about twice as tall as it is wide, so
//! `GlyphScale { cols: 6, rows: 6 }` is not a big square icon — it is a
//! stretched one, twelve half-cells tall and six wide. A SQUARE
//! footprint needs `cols == 2 * rows`: use [`GlyphScale::square`].
//!
//! Against the font's full 8x16 box a glyph is undistorted when
//! `cols == rows`, and [`smallest_clear`] searches only inside
//! [`MAX_STRETCH`] of that in either direction
//! ([`GlyphScale::within_aspect_band`]). That is the operator's cap, and
//! it is what refuses `6x2`.
//!
//! **It is a coarse bound around the wrong centre, and it needs the
//! measured term beside it.** No run fills all 16 rows — the crop below
//! sees to that — so the true undistorted point moves with the content:
//! for a row of icons it is nearer `cols == 1.5 * rows`. The band
//! therefore passes `2x3` braille icons, which is a two-and-a-half times
//! vertical stretch, while refusing `6x2` icons that measure inside
//! [`FIDELITY_MAX`]. Each term catches what the other misses;
//! [`legibility`] applies both, and neither is redundant. There is a
//! guard per direction.
//!
//! # Vertical crop, and why the whole run is cropped at once
//!
//! Capitals occupy 9 of the font's 16 rows; the rest is ascender and
//! descender space. Rasterizing inside the full box spends a third of a
//! three-row budget on nothing, which is what makes small scales look
//! dead. The ink is therefore cropped vertically — but over the WHOLE
//! string at once, never per glyph. Cropping each glyph to its own box
//! would stretch `o` to the height of `A` and lift `p` off its
//! descender; cropping the run preserves the baseline and every
//! relative height. The consequence is worth knowing: a string with a
//! descender genuinely needs more rows than one without, so `Agora`
//! reads weaker than `AGORA` at the same scale. That is the font
//! telling the truth, not a defect.

use crate::base::{Rgba, Size};
use crate::gfx::mosaic::{self, MosaicGrid, MosaicMode};
use crate::gfx::Bitmap;
use crate::render::screenshot_font_data::{GLYPHS, GLYPH_H, GLYPH_W};

/// How many CELLS one character is drawn into.
///
/// Cells, not pixels: the mosaic mode decides how many subpixels that
/// buys (braille 2x4 per cell, sextant 2x3, quadrant 2x2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphScale {
    /// Cells across, per character.
    pub cols: i32,
    /// Cells down, per character.
    pub rows: i32,
}

impl GlyphScale {
    /// Four cells across, three down. **Clear for every content class
    /// in braille, and MARGINAL for mixed text in sextant** — `o`
    /// against `0` measures 3 subpixels there, one under the bar.
    ///
    /// That split is worth reading twice, because it cuts against this
    /// module's own advice. Sextants are what [`render`] tells you to
    /// prefer for text — braille draws as separated dots and a letter
    /// reads as a constellation — so the mode that LOOKS more solid is
    /// the one with less measured room, and at this scale it is trading
    /// distinctness for solidity.
    ///
    /// **In sextant, mixed text clears at 4x4** — four more cells, and
    /// what `smallest_clear(Sextant, Content::Text)` returns. This
    /// paragraph used to name [`COMPACT_WIDE`](Self::COMPACT_WIDE) (6x2)
    /// instead, on the grounds that it measured clear for the same
    /// twelve cells; it is out of the aspect band and the recommendation
    /// was wrong. See [`within_aspect_band`](Self::within_aspect_band).
    ///
    /// It is not "the floor" and no longer pretends to be: [`Content`]
    /// classes bottom out at different sizes and modes disagree with
    /// each other. Ask [`smallest_clear`] rather than reaching for a
    /// constant, and reach for this one only when you want a
    /// conservative three-row banner and have not measured.
    pub const FLOOR: GlyphScale = GlyphScale { cols: 4, rows: 3 };

    /// Three cells square. **Uppercase only** — in both text modes a
    /// lowercase pair renders IDENTICALLY here (`0` against `8` in
    /// braille), so a caller passing arbitrary text at this scale is
    /// showing the reader two characters they cannot tell apart.
    ///
    /// Kept because a three-column header is sometimes the only one
    /// that fits and a silent refusal would be worse. But prefer
    /// [`COMPACT`](Self::COMPACT): it costs one row LESS and is
    /// strictly better measured.
    pub const TIGHT: GlyphScale = GlyphScale { cols: 3, rows: 3 };

    /// Four across, two down — the cheapest scale that reads for mixed
    /// text, and the one this module used to refuse.
    ///
    /// It measures (UPPER 4, lower 2, icons 11) in braille against
    /// 3x3's (4, **0**, 11): the same uppercase margin, a whole row
    /// cheaper, and it keeps apart the lowercase pair that 3x3
    /// collapses. The old rectangle test rejected it for having two
    /// rows while offering the strictly worse 3x3, which is what
    /// `smallest_clear` exists to stop happening again.
    pub const COMPACT: GlyphScale = GlyphScale { cols: 4, rows: 2 };

    /// Six across, two down. **Out of the aspect band, and kept as the
    /// worked example of why the band exists.**
    ///
    /// It measures beautifully: twelve cells, exactly what 4x3 costs,
    /// spent on columns instead of a row — braille gives lowercase 8
    /// subpixels here against 4x3's 5, and sextant 5 where 4x3 is
    /// Marginal at 3. This module recommended it on those numbers, and
    /// [`smallest_clear`] returned it.
    ///
    /// **On screen it is a 3x horizontal stretch, and a row of icons at
    /// this scale renders as white bars.** `●` at 6x2 braille comes back
    /// as `⣾⠀⠀⠀⣷⣦` / `⠻⠿⠀⠀⠿⠏` — six near-solid cells. The pairwise
    /// measure sees 16 subpixels between that and `◆`, and it is right;
    /// neither one looks like its glyph. That is the whole case for
    /// [`within_aspect_band`](Self::within_aspect_band), which this
    /// scale fails: `6 > MAX_STRETCH * 2`.
    ///
    /// Still constructible — the band governs what the SEARCH offers,
    /// not what you may draw — and genuinely fine for a run of
    /// uppercase, which has no round strokes to lose. Do not reach for
    /// it for icons.
    pub const COMPACT_WIDE: GlyphScale = GlyphScale { cols: 6, rows: 2 };

    /// A scale whose footprint is SQUARE on screen, `rows` cells tall.
    ///
    /// Cells are about 1:2, so this is `cols == 2 * rows`.
    ///
    /// **`square(2)` is a 4x2 badge, and it is the size a LONE icon
    /// wants — not a row of them.** This doc said the second thing and
    /// the shape measure contradicted it: in braille 4x2 icons lose 0.25
    /// of the footprint and read [`Clear`](Legibility::Clear); in
    /// sextant they lose 0.36 and read
    /// [`Distorted`](Legibility::Distorted), and the answer there is
    /// 3x2. The cause is this module's own vertical crop — a ROW of
    /// icons crops as one run, so the box is the union of `⚠ ☑ → ●` and
    /// taller than any single one of them, which makes four columns a
    /// horizontal stretch. Ask [`smallest_clear`] for your mode.
    ///
    /// **`square(1)` is not an icon size and this doc used to say it
    /// was.** Measured across the icon set, 2x1 gives 1 subpixel in
    /// braille, 2 in sextant and **0** in quadrant, where `★` and `✗`
    /// come out as the same picture. Use it only for an icon that never
    /// has to be told apart from another icon — a lone spinner, not a
    /// row of status glyphs.
    ///
    /// Columns do fix it — 3x1 in braille clears for icons — but 3x1 is
    /// past the aspect band and [`smallest_clear`] will not offer it.
    /// For braille icons it returns 2x2 instead: one more row, in band,
    /// and 5 subpixels rather than 4. Ask it for your mode instead of
    /// taking a rule of thumb from here;
    /// `one_row_icons_are_not_distinguishable` is the guard under the
    /// 2x1 numbers above.
    ///
    /// Every `square(rows)` sits on the band's wide edge by
    /// construction, since the edge IS `cols == 2 * rows`.
    pub fn square(rows: i32) -> GlyphScale {
        GlyphScale {
            cols: rows.max(1) * 2,
            rows: rows.max(1),
        }
    }

    /// Cells this scale spends per character, gap excluded — the
    /// ordering `smallest_clear` walks.
    fn cost(&self) -> i32 {
        self.cols * self.rows
    }

    /// Whether this scale distorts the glyph by no more than
    /// [`MAX_STRETCH`] in either direction — the constraint
    /// [`smallest_clear`] searches under.
    ///
    /// # The arithmetic, because "aspect" is the word everyone gets
    /// backwards here
    ///
    /// The font's glyph BOX is 8x16 pixels: **twice as tall as it is
    /// wide.** A terminal cell is also about twice as tall as it is
    /// wide. Measure a `cols` x `rows` scale in half-cell squares and it
    /// covers `cols` wide by `2 * rows` tall, so against the full box
    /// the horizontal stretch is `cols / rows`:
    ///
    /// - `cols == rows` — undistorted against the BOX. `4x4`.
    /// - `cols == 2 * rows` — a SQUARE footprint, and a 2x horizontal
    ///   stretch against the box. [`square`](Self::square) sits exactly
    ///   here, on the band's wide edge.
    /// - `rows == 2 * cols` — the mirror: 2x stretched tall.
    ///
    /// Both edges are included; anything past either is refused.
    ///
    /// # This bound is referenced to a box the renderer never draws
    ///
    /// Worth knowing before you trust it, and found by measuring rather
    /// than by reading: [`rasterize`] CROPS to the run's ink rows (see
    /// the module header) and stretches THAT box to the footprint. No
    /// run fills all 16 — capitals are 9 rows, the icon set 6 to 8 — so
    /// the undistorted point is not `cols == rows` but
    /// `cols == 16 * rows / ink_rows`, which for icons is
    /// `cols == 2 * rows`: exactly [`square`](Self::square), exactly
    /// where this predicate puts its outer EDGE.
    ///
    /// So the band is a coarse bound around the wrong centre. It is
    /// kept, unchanged, because it is the operator's own cap and because
    /// it catches real cases the measured term misses — `6x2` braille
    /// icons measure a fidelity loss of 0.30, inside [`FIDELITY_MAX`],
    /// and this refuses them. [`fidelity_loss`] is the term referenced
    /// to what is actually drawn, per character, and it catches what
    /// this misses: `2x2` braille icons sit dead centre of this band and
    /// are a 2x VERTICAL stretch of an 8x8 disc. [`legibility`] applies
    /// both and neither is redundant.
    ///
    /// # Why the search is bounded at all
    ///
    /// [`legibility`] counts how many subpixels two rasterizations
    /// differ in, and more columns almost always buys more subpixels.
    /// Left unbounded the search therefore walks toward whatever is
    /// widest and calls it best — which is how this module came to offer
    /// `6x2` for braille text, where a filled `●` reduces to six
    /// near-solid cells in a row and reads as a white bar. The pairwise
    /// measure is right that the bar differs from the next bar by 16
    /// subpixels; it has no way to notice that neither one looks like
    /// its glyph any more. **The band is the term the measure is
    /// missing**: it does not make the number smarter, it stops the
    /// search from buying that number with distortion.
    ///
    /// Reported by the operator against a `6x2` braille screenshot, and
    /// the cap is theirs. It is applied in both directions here because
    /// the wide half alone hands sextant uppercase a `2x5` — ten cells,
    /// clear on the number, and a glyph squeezed to two fifths of its
    /// width.
    pub fn within_aspect_band(&self) -> bool {
        self.cols <= MAX_STRETCH * self.rows && self.rows <= MAX_STRETCH * self.cols
    }
}

/// The most a scale offered by [`smallest_clear`] may stretch a glyph
/// from its natural 1:1 cell aspect, in either direction.
///
/// A judgement like [`MARGINAL_MAX`], and stated as one: 2x is where
/// the operator drew it looking at real output, not a number this
/// module derived. See [`GlyphScale::within_aspect_band`] for what it
/// bounds and why an unbounded search goes wrong.
pub const MAX_STRETCH: i32 = 2;

/// What a run is going to hold, because the classes stop reading at
/// different sizes.
///
/// This is the parameter a single global floor was missing. Pass the
/// one that describes YOUR string: a status bar of check marks can go
/// smaller than a sentence, and asking about `Text` when you are
/// drawing icons buys rows you did not need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    /// Capitals only — a banner, an acronym, a shout.
    Uppercase,
    /// Arbitrary mixed text: lowercase and digits, which is where the
    /// hard pairs are (`o`/`0`, `b`/`h`, `e`/`8`). The strictest class,
    /// and the one to ask about when you do not know.
    Text,
    /// The symbol glyphs a TUI reaches for: `★ ☾ ⚠ ✓ ✗ ● ◆ ☑ → ← ▼ ▶`.
    ///
    /// Near-duplicates that exist to look alike (`▶`/`▸`, `★`/`☆`) are
    /// deliberately NOT in the set. They would report a collision no
    /// reader cares about and bury the ones that matter.
    Icons,
}

impl Content {
    /// The alphabet legibility is measured over. Exposed so a caller
    /// can see exactly what a verdict was computed from, and measure
    /// their own set with [`closest_pair_in`] if theirs differs.
    pub fn alphabet(self) -> &'static str {
        match self {
            Content::Uppercase => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            Content::Text => "abcdefghijklmnopqrstuvwxyz0123456789",
            Content::Icons => "★☾⚠✓✗●◆☑→←▼▶",
        }
    }
}

/// How well a content class survives a given style.
///
/// Two independent things can go wrong and the verdict names the worse
/// of them. Characters can become indistinguishable FROM EACH OTHER
/// ([`Collides`](Self::Collides), [`Marginal`](Self::Marginal)), or each
/// one can stop resembling ITSELF ([`Distorted`](Self::Distorted)).
/// Nothing in the pairwise measure can see the second, which is why it
/// exists — see [`fidelity_loss`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Legibility {
    /// Two characters in the class rasterize to the SAME subpixels.
    /// Not a judgement — the renderer did not distinguish them, so no
    /// reader can.
    Collides,
    /// The characters are distinct from each other and at least one no
    /// longer resembles its own glyph: [`fidelity_loss`] over
    /// [`FIDELITY_MAX`], or a scale outside
    /// [`GlyphScale::within_aspect_band`].
    ///
    /// **Ordered below [`Marginal`](Self::Marginal) deliberately, and
    /// that is a judgement.** A tight pair is a pair a careful reader
    /// can still resolve; a stretched glyph is one nobody can, however
    /// far apart the arithmetic says it is from its neighbour. The
    /// operator's `6x2` screenshot — two icons 16 subpixels apart, both
    /// white bars — is the case that fixed the order.
    ///
    /// The fix differs too, which is why this is not folded into
    /// `Collides`: a collision wants MORE cells, a distortion wants the
    /// same cells REBALANCED between columns and rows.
    Distorted,
    /// Distinct, but by no more than [`MARGINAL_MAX`] subpixels. It may
    /// read fine in your terminal's font and it may not; this is the
    /// verdict that means "look at it before you ship it".
    Marginal,
    /// More than [`MARGINAL_MAX`] subpixels between the closest pair,
    /// and every character still resembles its glyph.
    Clear,
}

/// The line between [`Marginal`](Legibility::Marginal) and
/// [`Clear`](Legibility::Clear), in subpixels.
///
/// **A judgement, not a measurement**, and the only one in this module.
/// `Collides` is arithmetic — zero difference is zero difference — but
/// where "tight" becomes "fine" depends on the font, the terminal's
/// antialiasing and the reader. Callers who want a different bar should
/// read [`closest_pair`] and apply their own.
pub const MARGINAL_MAX: u32 = 3;

/// How far a character may drift from its own shape before
/// [`legibility`] calls it [`Distorted`](Legibility::Distorted), as a
/// fraction of the footprint.
///
/// **A judgement like [`MARGINAL_MAX`], and calibrated rather than
/// derived.** 0.35 is the plateau: every published pick below is the
/// same anywhere in 0.33..=0.38, so the number is not sitting on a
/// cliff. Push it to 0.30 and braille mixed text jumps from 6x3 to 8x4
/// (twice the cells); relax it to 0.45 and braille icons fall back to
/// the one-row 3x1 this measure was built to refuse.
///
/// Read [`fidelity_loss`] and apply your own bar if you disagree — the
/// number is exposed for exactly that, and a scale within about 0.02 of
/// this line is inside the measurement's own resampling noise.
pub const FIDELITY_MAX: f32 = 0.35;

/// Display samples per half-cell square when [`fidelity_loss`] compares
/// a rendering against its glyph.
///
/// **Not arbitrary: the smallest resolution at which every mode's
/// subpixel grid lands on whole samples, doubled.** A cell is one
/// sample-square wide per `sub_w` and `2 / sub_h` tall, so the grid must
/// divide by 1 and 2 (halfblock, quadrant), 3 (sextant) and 4 (braille)
/// — 6 is the least such, and the rendering is then represented with no
/// resampling error of its own. 12 doubles it for the reference's sake:
/// at 6 the *reference* is coarse enough to move one published pick
/// (braille mixed text reads 5x3, which measures 0.34 — inside the noise
/// FIDELITY_MAX names). 8, 12 and 16 all agree; 6 is the odd one out.
const FIDELITY_SAMPLES: u32 = 12;

/// The rows [`smallest_clear`] walks. Columns are not a fixed range:
/// they are bounded per row by the aspect band
/// ([`GlyphScale::within_aspect_band`]), which at six rows reaches
/// twelve columns.
///
/// EXHAUSTIVE within that band on purpose. The first version was a
/// hand-written ladder in cost order, and its own guard caught it: it
/// returned 4x1 for braille icons while 3x1 — which I had not thought
/// to list — also cleared and cost a cell less. A search that can be
/// beaten by an omission is not a search.
const SEARCH_ROWS: std::ops::RangeInclusive<i32> = 1..=6;

/// Fewest columns worth rasterizing into: one column is two subpixels
/// wide at most, which cannot hold a letter in any mode.
const SEARCH_MIN_COLS: i32 = 2;

/// Every scale the search considers, in no particular order.
///
/// Exposed to the guards so they walk the REAL space rather than a
/// copied range. The previous version of this module had the range
/// written out again inside
/// `smallest_clear_is_not_beaten_by_anything_else_that_clears`, which
/// means widening the search would have silently stopped widening the
/// check.
fn search_space() -> impl Iterator<Item = GlyphScale> {
    let max_cols = MAX_STRETCH * *SEARCH_ROWS.end();
    SEARCH_ROWS
        .flat_map(move |rows| {
            (SEARCH_MIN_COLS..=max_cols).map(move |cols| GlyphScale { cols, rows })
        })
        // ONE predicate, applied once. Baking the bound into the column
        // range instead would enforce only its wide half and let a
        // 2x6 — a glyph squashed to a third of its width — win on cost.
        .filter(GlyphScale::within_aspect_band)
}

/// Which weight of the embedded font to draw.
///
/// The crate carries ONE font, so this is the CSS `font-weight` axis and
/// not a family choice. `Bold` is a synthetic weight — a one-pixel
/// horizontal dilation of the same glyphs, the trick a terminal has
/// always used for a bold face it does not have.
///
/// It raises the pairwise distance between letters at tight scales (at
/// 3x3 the closest pair moves from 4 subpixels to 6) and changes
/// nothing from four rows up. Read that number carefully rather than as
/// a quality score: dilation adds ink to EVERY glyph, so some of the
/// gain is mechanical, and at three rows it can also close a counter —
/// a bold `A` can lose the gap in its bowl. It is offered as a weight to
/// choose, not as an improvement to apply by default. Judge it at your
/// size, in your terminal; `cargo run --example bigtext` cycles it
/// with `w`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphWeight {
    /// The embedded 8x16 glyphs, unmodified.
    #[default]
    Regular,
    /// The same glyphs with stems thickened one pixel to the right.
    Bold,
}

/// How source pixels are reduced to subpixels.
///
/// Every scale worth using downsamples in at least one axis, and the
/// two answers are genuinely different rather than one being better:
///
/// - [`AreaAverage`](Sampling::AreaAverage) weights each source pixel by
///   how much of it the target covers, then thresholds. Thin strokes
///   survive, so letters stay APART — at 4x3 the closest pair is 8
///   subpixels rather than 4.
/// - [`Nearest`](Sampling::Nearest) takes one source pixel per target.
///   Edges are harder and the result reads crisper, at the cost of
///   dropping whatever falls between taps: the same 4x3 margin halves.
///
/// Neither is correct for every use, which is why this is a knob and not
/// a constant. Solid display type at three rows often looks better
/// nearest; a dense run of arbitrary text needs the margin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sampling {
    /// Coverage-weighted average, then threshold. The default.
    #[default]
    AreaAverage,
    /// One source pixel per target subpixel.
    Nearest,
}

/// Everything about HOW a run is drawn, so the call does not grow a
/// seventh positional argument.
///
/// `BigTextStyle::new(scale, mode)` takes the defaults ([`Regular`](GlyphWeight::Regular),
/// [`AreaAverage`](Sampling::AreaAverage)); the builders change one axis
/// each.
#[derive(Clone, Copy, Debug)]
pub struct BigTextStyle {
    /// Cells per character.
    pub scale: GlyphScale,
    /// Which sub-cell symbols the ink is re-encoded into.
    pub mode: MosaicMode,
    /// Which weight of the embedded font.
    pub weight: GlyphWeight,
    /// How source pixels reduce to subpixels.
    pub sampling: Sampling,
}

impl BigTextStyle {
    /// The default weight and sampling at `scale` in `mode`.
    pub fn new(scale: GlyphScale, mode: MosaicMode) -> BigTextStyle {
        BigTextStyle {
            scale,
            mode,
            weight: GlyphWeight::Regular,
            sampling: Sampling::AreaAverage,
        }
    }

    /// Draw at the given weight.
    pub fn weight(self, weight: GlyphWeight) -> BigTextStyle {
        BigTextStyle { weight, ..self }
    }

    /// Draw with the given sampling.
    pub fn sampling(self, sampling: Sampling) -> BigTextStyle {
        BigTextStyle { sampling, ..self }
    }
}

/// The two characters of `content` that come out most alike at this
/// style, and how many subpixels separate them.
///
/// This is the measurement every legibility claim in this module rests
/// on, exposed rather than summarised: rasterize each character of the
/// class alone, compare the resulting bit patterns pairwise, keep the
/// closest. `0` means the renderer produced the same picture twice.
///
/// It depends on all four axes, which is why it takes a whole
/// [`BigTextStyle`] — braille at 4x3 measures 8 where quadrant measures
/// 3, and point sampling roughly halves area averaging's margin.
///
/// `None` only if the class rasterizes to fewer than two characters
/// (an empty scale).
///
/// ```no_run
/// # use abstracttui::gfx::bigtext::{self, BigTextStyle, Content, GlyphScale};
/// # use abstracttui::gfx::mosaic::MosaicMode;
/// let style = BigTextStyle::new(GlyphScale::TIGHT, MosaicMode::Braille);
/// let (a, b, apart) = bigtext::closest_pair(&style, Content::Text).unwrap();
/// // 3x3 renders two of them identically: apart == 0.
/// println!("{a} and {b} are {apart} subpixels apart");
/// ```
pub fn closest_pair(style: &BigTextStyle, content: Content) -> Option<(char, char, u32)> {
    closest_pair_in(style, content.alphabet())
}

/// [`closest_pair`] over an arbitrary character set — YOUR alphabet
/// rather than one of the three [`Content`] classes.
///
/// Worth reaching for when the run is known and narrow: a clock shows
/// `0123456789:` and nothing else, and measuring the whole lowercase
/// alphabet against it would refuse sizes that are perfectly safe for
/// digits and a colon. Characters the font has no glyph for are
/// skipped, since a run containing one is refused by [`render`] anyway.
pub fn closest_pair_in(style: &BigTextStyle, set: &str) -> Option<(char, char, u32)> {
    let ink = Rgba::rgb(255, 255, 255);
    let ground = Rgba::rgb(0, 0, 0);
    let table: Vec<(char, Vec<bool>)> = set
        .chars()
        .filter_map(|c| {
            let bmp = rasterize_with(&c.to_string(), style, ink, ground).ok()?;
            let bits = (0..bmp.height())
                .flat_map(|y| (0..bmp.width()).map(move |x| (x, y)))
                .map(|(x, y)| bmp.get(x, y) == Some(ink))
                .collect();
            Some((c, bits))
        })
        .collect();
    let mut best: Option<(char, char, u32)> = None;
    for (i, (a, pa)) in table.iter().enumerate() {
        for (b, pb) in table.iter().skip(i + 1) {
            let d = pa.iter().zip(pb).filter(|(x, y)| x != y).count() as u32;
            if best.is_none_or(|(_, _, best_d)| d < best_d) {
                best = Some((*a, *b, d));
            }
        }
    }
    best
}

/// How far `c` drawn at this style is from `c` drawn at its NATURAL
/// proportions in the same footprint — 0.0 for a perfect match, 1.0 if
/// the two have no sample in common.
///
/// This is the recognisability term [`closest_pair`] cannot supply.
/// Pairwise distance asks whether two characters differ; this asks
/// whether one of them is still itself. They are independent, and the
/// operator's report that started this — a row of icons at `6x2`
/// measuring 16 subpixels apart and rendering as identical white bars —
/// is the case where the first says yes and the second says no.
///
/// # What it compares
///
/// A terminal cell is about 1:2, so a `cols x rows` scale is a
/// footprint `cols` wide and `2 * rows` tall in half-cell squares. The
/// renderer STRETCHES the glyph's cropped ink box to fill that
/// footprint, whatever its shape. The reference is the same box drawn
/// into the same footprint at its own proportions and centred, so the
/// stretch shows up as difference. Both are sampled at
/// a fixed resolution per half-square and the normalised Hamming
/// distance is the answer.
///
/// It therefore charges for two different losses at once, which is the
/// point: distortion (a wide footprint spent on a round glyph) and
/// quantisation (too few subpixels to hold the strokes).
///
/// # What it does NOT know
///
/// - **It is aspect-symmetric.** Stretching a disc 2x wide and 2x tall
///   score the same, and on a real screen they do not always read the
///   same. [`GlyphScale::within_aspect_band`] is the operator's
///   asymmetric judgement on top, and it catches cases this misses.
/// - It says nothing about the terminal's own font: braille drawn as
///   separated dots is a rendering fact no bitmap comparison can see.
///
/// # Alone, not in a run — and the difference is large
///
/// The crop is over the WHOLE run, so a character's shape depends on
/// what it is drawn NEXT TO: `z` on its own fills the box top to bottom,
/// while `z` in `Amazing` keeps its x-height because the run's box
/// reaches from the `A` to the `g`. This function answers for `c` ALONE
/// — the honest reading of "how does this one character come out".
///
/// For a class, ask [`least_faithful`], which measures every character
/// inside the class's own run. At `4x3` braille the two disagree by a
/// whole verdict: `z` alone measures 0.39 (a short letter stretched to
/// fill three rows) and the same `z` in the mixed-text run measures
/// 0.19. Reading the alone number as the class number would have
/// condemned [`GlyphScale::FLOOR`] for prose it renders perfectly well.
///
/// `None` if the font has no glyph for `c`, or the scale has no cells.
pub fn fidelity_loss(c: char, style: &BigTextStyle) -> Option<f32> {
    let glyph = dilate(lookup(c)?, style.weight);
    let (top, bottom) = ink_rows(core::slice::from_ref(&glyph))?;
    let ink = Rgba::rgb(255, 255, 255);
    let bmp = rasterize_with(&c.to_string(), style, ink, Rgba::rgb(0, 0, 0)).ok()?;
    slot_loss(&glyph, top, bottom, style, |x, y| {
        bmp.get(x, y) == Some(ink)
    })
}

/// The character of `content` that this style treats worst, and its
/// fidelity loss — the shape counterpart of [`closest_pair`].
///
/// Worst, not average, for the same reason `closest_pair` reports the
/// closest pair: one unrecognisable glyph in a run is a run the reader
/// stumbles on, and a mean hides it behind eleven that survived.
///
/// Every character is measured **inside the class's own run**, sharing
/// one vertical crop, which is how [`render`] will actually draw it —
/// see [`fidelity_loss`] for why measuring each one alone gives a
/// different and harsher answer.
///
/// `None` if the class cannot be rasterized at this style at all.
pub fn least_faithful(style: &BigTextStyle, content: Content) -> Option<(char, f32)> {
    let alphabet = content.alphabet();
    let glyphs: Vec<[u8; GLYPH_H as usize]> = alphabet
        .chars()
        .filter_map(|c| Some(dilate(lookup(c)?, style.weight)))
        .collect();
    let (top, bottom) = ink_rows(&glyphs)?;

    let ink = Rgba::rgb(255, 255, 255);
    let bmp = rasterize_with(alphabet, style, ink, Rgba::rgb(0, 0, 0)).ok()?;
    let (sub_w, _) = subpixels(style.mode);
    // The renderer's own slot arithmetic: one blank cell of gap between
    // characters, none on the ends.
    let stride = style.scale.cols as u32 * sub_w + sub_w;

    alphabet
        .chars()
        .zip(glyphs.iter())
        .enumerate()
        .filter_map(|(i, (c, glyph))| {
            let x0 = i as u32 * stride;
            let loss = slot_loss(glyph, top, bottom, style, |x, y| {
                bmp.get(x0 + x, y) == Some(ink)
            })?;
            Some((c, loss))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// The shared arithmetic: one character's rendered slot against the same
/// character drawn at its natural proportions in the same footprint.
///
/// `top..=bottom` is the ink box the renderer cropped to — the
/// character's own when it is drawn alone, the run's when it is not.
/// `read` samples the rendered slot in its own subpixel coordinates.
fn slot_loss(
    glyph: &[u8; GLYPH_H as usize],
    top: u32,
    bottom: u32,
    style: &BigTextStyle,
    read: impl Fn(u32, u32) -> bool,
) -> Option<f32> {
    let scale = style.scale;
    if scale.cols <= 0 || scale.rows <= 0 {
        return None;
    }
    let src_h = bottom.checked_sub(top)? + 1;

    // The footprint, in half-cell squares, upsampled.
    let w = scale.cols as u32 * FIDELITY_SAMPLES;
    let h = 2 * scale.rows as u32 * FIDELITY_SAMPLES;

    // The reference: the same ink box, uniformly scaled to fit and
    // centred. `min` is what keeps it undistorted; the letterboxed
    // remainder is blank, and a rendering that filled it is charged for
    // the difference.
    let k = (w as f32 / GLYPH_W as f32).min(h as f32 / src_h as f32);
    let fit_w = (GLYPH_W as f32 * k).max(1.0);
    let fit_h = (src_h as f32 * k).max(1.0);
    let ox = (w as f32 - fit_w) / 2.0;
    let oy = (h as f32 - fit_h) / 2.0;

    // The rendered slot, in subpixels.
    let (per_col, per_row) = subpixels(style.mode);
    let (sub_w, sub_h) = (scale.cols as u32 * per_col, scale.rows as u32 * per_row);
    let mut diff = 0u32;
    for y in 0..h {
        for x in 0..w {
            let drawn = read(
                (x * sub_w / w).min(sub_w - 1),
                (y * sub_h / h).min(sub_h - 1),
            );
            let fx = x as f32 - ox;
            let fy = y as f32 - oy;
            let natural = fx >= 0.0
                && fy >= 0.0
                && fx < fit_w
                && fy < fit_h
                && glyph_bit(
                    glyph,
                    (fx * GLYPH_W as f32 / fit_w) as u32,
                    (top as f32 + fy * src_h as f32 / fit_h) as u32,
                );
            if drawn != natural {
                diff += 1;
            }
        }
    }
    Some(diff as f32 / (w * h) as f32)
}

/// Whether `content` reads at this style, measured now rather than
/// asserted by a constant.
///
/// Two independent measurements, and the verdict is the worse:
///
/// - [`closest_pair`] — [`Collides`](Legibility::Collides) at zero
///   subpixels (arithmetic), [`Marginal`](Legibility::Marginal) up to
///   [`MARGINAL_MAX`].
/// - [`least_faithful`] — [`Distorted`](Legibility::Distorted) over
///   [`FIDELITY_MAX`], or for a scale outside
///   [`GlyphScale::within_aspect_band`].
///
/// [`Clear`](Legibility::Clear) only when both pass. A class that cannot
/// be measured at all (an empty scale) reads as `Collides`, because a
/// scale that draws nothing distinguishes nothing.
///
/// **This verdict used to be the pairwise half alone, and that made it
/// wrong in the one place it was being read.** `6x2` braille icons
/// measure 16 subpixels apart and render as two white bars; the old
/// answer was `Clear`, printed by this crate's own example directly
/// above the bars. The aspect band fixed what [`smallest_clear`]
/// OFFERS; this fixes what `legibility` SAYS about a scale the caller
/// names.
///
/// It is a design-time query and it is not cheap — it rasterizes the
/// whole class twice over. Call it in a test or a tool, not per frame.
///
/// ```no_run
/// # use abstracttui::gfx::bigtext::{self, BigTextStyle, Content, GlyphScale, Legibility};
/// # use abstracttui::gfx::mosaic::MosaicMode;
/// // Two ways to be unreadable. 2x2 collapses lowercase against digits,
/// // and stretches an icon to twice its height — the icon pair is
/// // measurably far apart and neither one is still a disc or a star.
/// let tiny = BigTextStyle::new(GlyphScale { cols: 2, rows: 2 }, MosaicMode::Braille);
/// assert!(bigtext::legibility(&tiny, Content::Text) < Legibility::Clear);
/// assert_eq!(bigtext::legibility(&tiny, Content::Icons), Legibility::Distorted);
/// // The badge shape is what icons want, and it is what the search returns.
/// let badge = BigTextStyle::new(GlyphScale::square(2), MosaicMode::Braille);
/// assert_eq!(bigtext::legibility(&badge, Content::Icons), Legibility::Clear);
/// ```
pub fn legibility(style: &BigTextStyle, content: Content) -> Legibility {
    let pairwise = match closest_pair(style, content) {
        None | Some((_, _, 0)) => return Legibility::Collides,
        Some((_, _, d)) if d <= MARGINAL_MAX => Legibility::Marginal,
        Some(_) => Legibility::Clear,
    };
    let unfaithful = least_faithful(style, content).is_none_or(|(_, loss)| loss > FIDELITY_MAX);
    if unfaithful || !style.scale.within_aspect_band() {
        return Legibility::Distorted;
    }
    pairwise
}

/// The cheapest scale that measures [`Clear`](Legibility::Clear) for
/// `content` in `mode` — the floor, DERIVED per class instead of
/// declared once for all of them.
///
/// "Cheapest" is fewest cells per character; ties break toward fewer
/// ROWS, because a terminal row costs about twice what a column does.
/// Weight and sampling take their defaults; measure with
/// [`legibility`] directly if you are changing those.
///
/// Only scales inside the aspect band are candidates — see
/// [`GlyphScale::within_aspect_band`] for why a cheapest-clear search
/// without one walks straight into unreadable output.
///
/// `None` means nothing in the band clears. **No mode and class answers
/// `None` today, including [`MosaicMode::HalfBlock`], and this doc used
/// to say the opposite.**
///
/// It read: `None` is "the honest answer for HalfBlock, whose
/// one-subpixel-wide cells cannot separate the lowercase set at any size
/// this module offers". Two things were wrong with that sentence, and
/// the second is the one worth carrying away.
///
/// - It was already false when written, for two of the three classes.
///   Halfblock uppercase clears at 6x5 and icons at 5x3 — both inside
///   the old six-column ceiling, so the search had been returning them
///   the whole time. The prose generalised a real result about
///   *lowercase* into a claim about the mode, and nothing measured it.
/// - For mixed text it WAS true, and it stopped being true when the
///   ceiling moved: 7x5 clears, and seven columns did not exist before.
///   So "no size works" was a fact about the search space wearing the
///   costume of a fact about the terminal — which is this module's own
///   recorded mistake, made a third time by the doc that records it.
///
/// Today: 6x5 uppercase, 7x5 mixed text, 5x3 icons.
///
/// ```no_run
/// # use abstracttui::gfx::bigtext::{self, Content};
/// # use abstracttui::gfx::mosaic::MosaicMode;
/// let for_text = bigtext::smallest_clear(MosaicMode::Braille, Content::Text);
/// let for_icons = bigtext::smallest_clear(MosaicMode::Braille, Content::Icons);
/// // Icons clear at a smaller scale than prose does.
/// assert!(for_icons.unwrap().cols * for_icons.unwrap().rows
///     <= for_text.unwrap().cols * for_text.unwrap().rows);
/// ```
pub fn smallest_clear(mode: MosaicMode, content: Content) -> Option<GlyphScale> {
    search_space()
        .filter(|scale| legibility(&BigTextStyle::new(*scale, mode), content) == Legibility::Clear)
        // Ties break toward fewer ROWS: a terminal row costs about twice
        // what a column does, so 6x2 beats 4x3 at equal cell count.
        .min_by_key(|scale| (scale.cost(), scale.rows))
}

/// Why a string cannot be drawn large.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BigTextError {
    /// The embedded font has no bitmap for this character. It carries
    /// 164 glyphs — Latin letters, digits and common punctuation — and
    /// no accented forms at all, so `é`, `à` and `ñ` land here.
    ///
    /// This is an error rather than a dropped character on purpose: a
    /// silently dropped `é` renders "Café" as "Caf", which looks like a
    /// finished word.
    UnsupportedChar(char),
    /// The scale asked for zero or negative cells in some axis.
    EmptyScale(GlyphScale),
    /// The string has no ink at all (empty, or all spaces), so there is
    /// no baseline to crop to and nothing to draw.
    NothingToDraw,
    /// A transparent ground was passed for a mode that fits TWO colours
    /// per cell ([`Quadrant`](MosaicMode::Quadrant),
    /// [`Sextant`](MosaicMode::Sextant)).
    ///
    /// Those modes choose the glyph by fitting ink and ground against
    /// each other, and a transparent subpixel does not vote — so the fit
    /// has nothing to weigh and every cell comes back blank. Refusing is
    /// the point: a correctly-sized grid of empty cells looks like a
    /// working call, and it is the hardest kind of bug to see.
    ///
    /// Pass the colour you are drawing ONTO (the theme's surface, say),
    /// or use [`Braille`](MosaicMode::Braille) or
    /// [`HalfBlock`](MosaicMode::HalfBlock), which threshold by
    /// luminance and carry transparency fine.
    TransparentGround(MosaicMode),
}

impl core::fmt::Display for BigTextError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BigTextError::UnsupportedChar(c) => write!(
                f,
                "the embedded 8x16 font has no glyph for {c:?} \
                 (164 glyphs: Latin, digits, common punctuation; no accents)"
            ),
            BigTextError::EmptyScale(s) => {
                write!(f, "scale {}x{} has no cells to draw into", s.cols, s.rows)
            }
            BigTextError::NothingToDraw => write!(f, "the string carries no ink"),
            BigTextError::TransparentGround(m) => write!(
                f,
                "{m:?} fits two colours per cell and renders blank against a \
                 transparent ground — pass the colour you are drawing onto, \
                 or use Braille or HalfBlock"
            ),
        }
    }
}

impl std::error::Error for BigTextError {}

/// Cells the banner will occupy, WITHOUT rendering it.
///
/// Call this before committing layout: at [`GlyphScale::FLOOR`] a
/// nine-character title is 45 columns and 3 rows, which is a banner and
/// not a navbar. `None` for a scale with no cells.
pub fn measure(text: &str, scale: GlyphScale) -> Option<Size> {
    if scale.cols <= 0 || scale.rows <= 0 {
        return None;
    }
    let n = text.chars().count() as i32;
    if n == 0 {
        return Some(Size::new(0, 0));
    }
    // One blank cell column between characters, none on the ends.
    Some(Size::new(n * scale.cols + (n - 1), scale.rows))
}

/// Rasterize `text` into a 1-bit bitmap sized for `mode` at `scale`.
///
/// The bitmap is `ink` where the glyph is set and `ground` elsewhere,
/// ready for [`mosaic::render`]. Most callers want [`render`] instead;
/// this is the seam for anyone who wants the pixels.
pub fn rasterize(
    text: &str,
    scale: GlyphScale,
    mode: MosaicMode,
    ink: Rgba,
    ground: Rgba,
) -> Result<Bitmap, BigTextError> {
    rasterize_with(text, &BigTextStyle::new(scale, mode), ink, ground)
}

/// [`rasterize`] with the weight and sampling chosen explicitly.
pub fn rasterize_with(
    text: &str,
    style: &BigTextStyle,
    ink: Rgba,
    ground: Rgba,
) -> Result<Bitmap, BigTextError> {
    let BigTextStyle {
        scale,
        mode,
        weight,
        sampling,
    } = *style;
    if scale.cols <= 0 || scale.rows <= 0 {
        return Err(BigTextError::EmptyScale(scale));
    }
    let chars: Vec<char> = text.chars().collect();
    let glyphs = chars
        .iter()
        .map(|c| lookup(*c).ok_or(BigTextError::UnsupportedChar(*c)))
        .map(|g| g.map(|rows| dilate(rows, weight)))
        .collect::<Result<Vec<_>, _>>()?;
    if glyphs.is_empty() {
        return Err(BigTextError::NothingToDraw);
    }

    // The run's own ink rows, so the baseline survives the crop.
    let (top, bottom) = ink_rows(&glyphs).ok_or(BigTextError::NothingToDraw)?;
    let src_h = bottom - top + 1;

    let (sub_w, sub_h) = subpixels(mode);
    let cell_w = scale.cols as u32 * sub_w;
    let cell_h = scale.rows as u32 * sub_h;
    let gap = sub_w; // the one-cell gap, in subpixels
    let n = glyphs.len() as u32;
    let out_w = n * cell_w + (n - 1) * gap;

    Ok(Bitmap::from_fn(out_w, cell_h, |x, y| {
        let slot = x / (cell_w + gap);
        let within = x % (cell_w + gap);
        if within >= cell_w {
            return ground; // the inter-character gap
        }
        let x0 = within as f32 * GLYPH_W as f32 / cell_w as f32;
        let x1 = (within + 1) as f32 * GLYPH_W as f32 / cell_w as f32;
        let y0 = top as f32 + y as f32 * src_h as f32 / cell_h as f32;
        let y1 = top as f32 + (y + 1) as f32 * src_h as f32 / cell_h as f32;
        let rows = &glyphs[slot as usize];
        let set = match sampling {
            Sampling::AreaAverage => coverage(rows, x0, x1, y0, y1) >= INK_THRESHOLD,
            Sampling::Nearest => glyph_bit(rows, x0 as u32, y0 as u32),
        };
        if set {
            ink
        } else {
            ground
        }
    }))
}

/// The glyph rows at `weight`. `Bold` dilates one pixel to the right:
/// bit 7 is the leftmost column, so `row >> 1` moves ink rightward and
/// nothing wraps between rows.
fn dilate(rows: &[u8; GLYPH_H as usize], weight: GlyphWeight) -> [u8; GLYPH_H as usize] {
    match weight {
        GlyphWeight::Regular => *rows,
        GlyphWeight::Bold => core::array::from_fn(|i| rows[i] | (rows[i] >> 1)),
    }
}

/// Rasterize and encode in one call — the surface most callers want.
///
/// The returned grid is `measure(text, scale)` cells, ready for
/// [`mosaic::blit_to_cells`].
///
/// `mode` is the caller's, deliberately: [`MosaicMode::auto`] picks by
/// probed capability, and the trade here differs from the image case.
/// Braille has the most subpixels but a terminal draws it as separated
/// DOTS, so a letter reads as a constellation; sextants are solid ink
/// at 2x3 and read as letterforms. For text, prefer
/// [`MosaicMode::Sextant`] where the font has it.
pub fn render(
    text: &str,
    scale: GlyphScale,
    mode: MosaicMode,
    ink: Rgba,
    ground: Rgba,
) -> Result<MosaicGrid, BigTextError> {
    render_with(text, &BigTextStyle::new(scale, mode), ink, ground)
}

/// [`render`] with the weight and sampling chosen explicitly.
pub fn render_with(
    text: &str,
    style: &BigTextStyle,
    ink: Rgba,
    ground: Rgba,
) -> Result<MosaicGrid, BigTextError> {
    if ground.is_transparent() && matches!(style.mode, MosaicMode::Quadrant | MosaicMode::Sextant) {
        return Err(BigTextError::TransparentGround(style.mode));
    }
    let bmp = rasterize_with(text, style, ink, ground)?;
    let size = measure(text, style.scale).ok_or(BigTextError::EmptyScale(style.scale))?;
    Ok(mosaic::render(
        &bmp,
        size.w.max(0) as u32,
        size.h.max(0) as u32,
        style.mode,
    ))
}

/// Fraction of a source box that must be ink before the subpixel is
/// set. Below ~0.25 stems bloat and counters close up; above ~0.4 thin
/// strokes drop out entirely. 0.30 was picked by rendering the alphabet
/// at every offered scale and is guarded by the distinctness tests.
const INK_THRESHOLD: f32 = 0.30;

/// Ink fraction of the source rectangle `[x0,x1) x [y0,y1)`, weighted
/// by how much of each source pixel the box actually covers.
fn coverage(rows: &[u8; GLYPH_H as usize], x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
    let mut acc = 0.0;
    let mut area = 0.0;
    for sy in y0.floor() as u32..(y1.ceil() as u32).min(GLYPH_H) {
        let fy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
        if fy == 0.0 {
            continue;
        }
        for sx in x0.floor() as u32..(x1.ceil() as u32).min(GLYPH_W) {
            let fx = (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
            let w = fx * fy;
            area += w;
            if glyph_bit(rows, sx, sy) {
                acc += w;
            }
        }
    }
    if area == 0.0 {
        0.0
    } else {
        acc / area
    }
}

fn subpixels(mode: MosaicMode) -> (u32, u32) {
    match mode {
        MosaicMode::HalfBlock => (1, 2),
        MosaicMode::Quadrant => (2, 2),
        MosaicMode::Sextant => (2, 3),
        MosaicMode::Braille => (2, 4),
    }
}

fn lookup(c: char) -> Option<&'static [u8; GLYPH_H as usize]> {
    GLYPHS
        .binary_search_by_key(&c, |(g, _)| *g)
        .ok()
        .map(|i| &GLYPHS[i].1)
}

fn glyph_bit(rows: &[u8; GLYPH_H as usize], x: u32, y: u32) -> bool {
    if x >= GLYPH_W || y >= GLYPH_H {
        return false;
    }
    rows[y as usize] >> (GLYPH_W - 1 - x) & 1 == 1
}

/// First and last row of the run that carries any ink.
fn ink_rows(glyphs: &[[u8; GLYPH_H as usize]]) -> Option<(u32, u32)> {
    let mut top = None;
    let mut bottom = 0;
    for y in 0..GLYPH_H {
        if glyphs.iter().any(|g| g[y as usize] != 0) {
            top.get_or_insert(y);
            bottom = y;
        }
    }
    top.map(|t| (t, bottom))
}

#[cfg(test)]
#[path = "bigtext_tests.rs"]
mod tests;

//! The claims in `bigtext`'s own docs, made falsifiable.
//!
//! The module's reason to exist is a measurement — "4x3 is the floor" —
//! and a measurement nobody re-runs is a comment. These tests re-derive
//! it from the font on every build, so lowering the constant or
//! changing the rasterizer turns them red rather than quietly making
//! the docs wrong.

use super::*;
use crate::gfx::mosaic::MosaicMode;

const INK: Rgba = Rgba::rgb(255, 255, 255);
const GROUND: Rgba = Rgba::rgb(0, 0, 0);
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The library's own measurement, wrapped for the call sites here.
///
/// These tests used to carry a private copy of the pairwise-distance
/// arithmetic. That is the failure mode an example or a test suite
/// reaches for most easily: a copy agrees with itself forever while the
/// thing it is meant to guard drifts underneath. `closest_pair_in` is
/// public API now, so the guards call it and a break in the real code
/// reds the real tests.
fn closest_pair(scale: GlyphScale, mode: MosaicMode) -> (char, char, u32) {
    closest_in(UPPER, scale, mode)
}

fn closest_pair_with(style: &BigTextStyle) -> u32 {
    closest_pair_in(style, UPPER)
        .expect("uppercase is measurable at every offered scale")
        .2
}

fn closest_in(set: &str, scale: GlyphScale, mode: MosaicMode) -> (char, char, u32) {
    closest_pair_in(&BigTextStyle::new(scale, mode), set).unwrap_or((' ', ' ', 0))
}

/// THE load-bearing guard. `FLOOR` is a measured claim about letters
/// staying apart; this re-derives it. Lower `FLOOR.cols` to 3 and the
/// margin assertion reds with the real arithmetic.
#[test]
fn every_letter_stays_distinct_at_the_declared_floor() {
    let (a, b, d) = closest_pair(GlyphScale::FLOOR, MosaicMode::Braille);
    assert!(
        d >= 8,
        "at the declared FLOOR the closest pair {a}/{b} differs by only {d} subpixels — \
         the floor no longer buys the margin its doc claims"
    );
}

/// The other end of the same measurement, and the reason `TIGHT` is
/// offered rather than refused: it is legible — no two letters collide
/// — but with visibly less room than the floor. If these two ever
/// measure the same, one of the constants is pointless.
#[test]
fn the_tight_scale_is_legible_but_measurably_tighter_than_the_floor() {
    let (_, _, tight) = closest_pair(GlyphScale::TIGHT, MosaicMode::Braille);
    let (_, _, floor) = closest_pair(GlyphScale::FLOOR, MosaicMode::Braille);
    assert!(
        tight > 0,
        "TIGHT collapses two UPPERCASE letters into identical output"
    );
    assert!(
        tight < floor,
        "TIGHT ({tight}) is not tighter than FLOOR ({floor}), so one of them is not worth having"
    );
}

/// The two text modes this module tells callers to use for text.
const TEXT_MODES: [MosaicMode; 2] = [MosaicMode::Sextant, MosaicMode::Braille];

/// `FLOOR`'s doc makes a SPLIT claim — clear for everything in
/// braille, marginal for mixed text in sextant — and the split is the
/// interesting half, so both sides are pinned.
///
/// This test is why the doc says that. It was written asserting `Clear`
/// everywhere, which is what the constant's name implies and what I
/// believed; it went red on sextant text at 3 subpixels. The constant
/// named for safety was not safe in the mode this module recommends for
/// text, and no amount of reading the code would have said so.
#[test]
fn the_floor_is_clear_in_braille_and_only_marginal_for_text_in_sextant() {
    let braille = BigTextStyle::new(GlyphScale::FLOOR, MosaicMode::Braille);
    for content in [Content::Uppercase, Content::Text, Content::Icons] {
        let (a, b, d) = closest_pair_in(&braille, content.alphabet()).expect("measurable");
        assert_eq!(
            legibility(&braille, content),
            Legibility::Clear,
            "FLOOR is documented as clear for every class in braille, but {content:?} \
             measures {a}/{b} at {d} subpixels"
        );
    }

    let sextant = BigTextStyle::new(GlyphScale::FLOOR, MosaicMode::Sextant);
    assert_eq!(
        legibility(&sextant, Content::Text),
        Legibility::Marginal,
        "FLOOR's doc, COMPACT_WIDE's reason to exist and the module header all rest on \
         mixed text being MARGINAL at 4x3 in sextant. If that moved, three docs are now \
         wrong together"
    );
    for content in [Content::Uppercase, Content::Icons] {
        assert_eq!(
            legibility(&sextant, content),
            Legibility::Clear,
            "the sextant caveat is documented as applying to mixed TEXT only"
        );
    }
}

/// `COMPACT_WIDE` measures better than `FLOOR` for the same twelve
/// cells: one row cheaper, and clear for mixed text in both text modes
/// where `FLOOR` is not.
///
/// **All of that is still true and it is no longer a recommendation.**
/// The scale is out of the aspect band, and this guard is now what
/// makes that interesting rather than obvious: it pins the fact that
/// the band rejects a scale the pairwise measure genuinely prefers. If
/// these assertions ever go red, the band would be discarding nothing
/// and `COMPACT_WIDE`'s doc — which calls itself the worked example of
/// why the band exists — would have no example left in it.
///
/// **The two halves must disagree, and that is now asserted rather than
/// implied.** `legibility` says `Distorted` and `closest_pair` says this
/// is the widest margin twelve cells can buy. A version of this guard
/// that only checked the verdict would pass just as well if the pairwise
/// number had collapsed too — and then the scale would be plain bad
/// rather than the interesting case the doc calls it.
#[test]
fn the_wide_compact_scale_clears_text_where_the_floor_is_only_marginal() {
    for mode in TEXT_MODES {
        let wide = BigTextStyle::new(GlyphScale::COMPACT_WIDE, mode);
        assert_eq!(
            legibility(&wide, Content::Text),
            Legibility::Distorted,
            "COMPACT_WIDE is the operator's white-bar scale; legibility must refuse it \
             on SHAPE in {mode:?}, however far apart the pair measures"
        );
        let (_, _, wide_d) = closest_pair_in(&wide, Content::Text.alphabet()).unwrap();
        let (_, _, floor_d) = closest_pair_in(
            &BigTextStyle::new(GlyphScale::FLOOR, mode),
            Content::Text.alphabet(),
        )
        .unwrap();
        assert!(
            wide_d > floor_d,
            "in {mode:?} COMPACT_WIDE gives lowercase {wide_d} subpixels against FLOOR's \
             {floor_d} — the doc claims it is the better spend of the same twelve cells"
        );
    }
    assert_eq!(
        GlyphScale::COMPACT_WIDE.cols * GlyphScale::COMPACT_WIDE.rows,
        GlyphScale::FLOOR.cols * GlyphScale::FLOOR.rows,
        "the whole pitch is SAME cost, fewer rows"
    );
    // A const block, not a runtime assert: this compares two constants,
    // so the compiler holds it. Clippy flagged the runtime version as
    // constant-valued, which is the same observation.
    //
    // Measured, because "it fails at compile time" is exactly the kind
    // of claim this file exists to distrust: setting COMPACT_WIDE.rows
    // to FLOOR.rows reds `cargo test --no-run` and passes
    // `cargo check --all-targets`. It is a TEST-build failure, not a
    // check failure — which is still earlier and louder than an assert.
    const { assert!(GlyphScale::COMPACT_WIDE.rows < GlyphScale::FLOOR.rows) };
}

/// And the derivation must agree with the doc rather than being a
/// second opinion.
///
/// This guard used to assert `COMPACT_WIDE` here, because `FLOOR`'s doc
/// told sextant callers to reach for it. **The aspect band overturned
/// that recommendation** — `6x2` is a 3x horizontal stretch, which is
/// what the operator's white-bar screenshot was a picture of — so both
/// the doc and this guard now name the in-band answer. The point of the
/// guard is unchanged: the prose and the search must not be two
/// opinions.
#[test]
fn smallest_clear_recommends_the_scale_the_floor_doc_points_at() {
    let pick = smallest_clear(MosaicMode::Sextant, Content::Text)
        .expect("sextant text clears somewhere in the band");
    assert_eq!(
        (pick.cols, pick.rows),
        (4, 4),
        "FLOOR's doc names the scale sextant callers should use for mixed text; if the \
         search disagrees, one of them is lying to the reader"
    );
    assert!(
        !GlyphScale::COMPACT_WIDE.within_aspect_band(),
        "COMPACT_WIDE's doc warns it is out of band; if it is back inside, that warning \
         and FLOOR's cross-reference are both wrong"
    );
}

/// **THE finding, pinned.** `has_margin` refused 4x2 for having two
/// rows while offering 3x3, and 3x3 is strictly worse: same uppercase
/// margin, one row MORE, and it renders two lowercase characters
/// identically.
///
/// If this ever reds, either the rasterizer changed or `COMPACT`'s doc
/// is lying — and the doc is what a caller picks a size from.
#[test]
fn the_compact_scale_beats_the_tight_scale_that_used_to_be_offered_instead() {
    for mode in TEXT_MODES {
        let compact = BigTextStyle::new(GlyphScale::COMPACT, mode);
        let tight = BigTextStyle::new(GlyphScale::TIGHT, mode);

        assert_eq!(
            legibility(&tight, Content::Text),
            Legibility::Collides,
            "TIGHT is documented as uppercase-only because mixed text COLLIDES there; \
             in {mode:?} it no longer does, so the doc oversells the danger"
        );
        assert_ne!(
            legibility(&compact, Content::Text),
            Legibility::Collides,
            "COMPACT is the scale that keeps apart what TIGHT collapses; in {mode:?} it \
             collapses too, so there is no reason to prefer it"
        );

        let (_, _, c_upper) = closest_pair_in(&compact, Content::Uppercase.alphabet()).unwrap();
        let (_, _, t_upper) = closest_pair_in(&tight, Content::Uppercase.alphabet()).unwrap();
        assert!(
            c_upper >= t_upper,
            "in {mode:?} COMPACT gives uppercase {c_upper} subpixels against TIGHT's \
             {t_upper} — it is supposed to be at least as good for a row less"
        );
        // COMPACT's whole claim is that it costs fewer ROWS — two
        // constants, so the compiler can hold it rather than a test run.
        const { assert!(GlyphScale::COMPACT.rows < GlyphScale::TIGHT.rows) };
    }
}

/// The reason [`Content`] is a parameter at all: at ONE scale the
/// classes disagree, so no single verdict can describe it.
///
/// 2x2 is dead for lowercase and fine for icons. Delete `Content` and
/// collapse the API back to one boolean and this is the test that
/// cannot be written.
#[test]
fn one_scale_is_dead_for_text_and_clear_for_icons_at_the_same_time() {
    let tiny = BigTextStyle::new(GlyphScale { cols: 2, rows: 2 }, MosaicMode::Braille);
    let text = legibility(&tiny, Content::Text);
    let icons = legibility(&tiny, Content::Icons);
    assert!(
        text < Legibility::Clear,
        "2x2 no longer fails for mixed text ({text:?}), so the example's headline \
         disagreement is stale"
    );
    assert_eq!(
        icons,
        Legibility::Clear,
        "2x2 is supposed to be perfectly serviceable for ICONS — that is the half \
         of the finding a single global floor could not express"
    );
}

/// The mode was the other thing the old predicate ignored. Same scale,
/// same class, a verdict that moves — which is why `legibility` takes a
/// whole style and `smallest_clear` takes a mode.
#[test]
fn the_same_scale_measures_differently_in_different_symbols() {
    let scale = GlyphScale::FLOOR;
    let braille = closest_pair_in(
        &BigTextStyle::new(scale, MosaicMode::Braille),
        Content::Uppercase.alphabet(),
    )
    .unwrap()
    .2;
    let quadrant = closest_pair_in(
        &BigTextStyle::new(scale, MosaicMode::Quadrant),
        Content::Uppercase.alphabet(),
    )
    .unwrap()
    .2;
    assert!(
        braille > quadrant,
        "at the same {}x{}, braille measures {braille} and quadrant {quadrant} — if a \
         mode-blind predicate were adequate these would not differ",
        scale.cols,
        scale.rows
    );
}

/// The floor is DERIVED per class, and the derivation must actually
/// order them: icons clear cheaper than mixed text does.
#[test]
fn the_derived_floor_is_cheaper_for_icons_than_for_prose() {
    for mode in TEXT_MODES {
        let text = smallest_clear(mode, Content::Text)
            .unwrap_or_else(|| panic!("{mode:?} must clear for text at SOME offered scale"));
        let icons = smallest_clear(mode, Content::Icons)
            .unwrap_or_else(|| panic!("{mode:?} must clear for icons at SOME offered scale"));
        assert!(
            icons.cols * icons.rows <= text.cols * text.rows,
            "in {mode:?} the derived icon floor {}x{} costs more than the text floor \
             {}x{} — then a per-class floor is buying nothing",
            icons.cols,
            icons.rows,
            text.cols,
            text.rows
        );
        // And what it returns must actually be clear, or the search is
        // reporting a size it never checked.
        assert_eq!(
            legibility(&BigTextStyle::new(text, mode), Content::Text),
            Legibility::Clear
        );
        assert_eq!(
            legibility(&BigTextStyle::new(icons, mode), Content::Icons),
            Legibility::Clear
        );
    }
}

/// `smallest_clear` must return the CHEAPEST clear scale, not the first
/// one it happened to walk past. Nothing else in the ladder that clears
/// may cost less than what it returned.
///
/// It walks `search_space()` rather than a range written out again
/// here. The copy is what this file had before, and it was already
/// stale the moment the space grew: with `2..=6 x 1..=6` inlined, this
/// guard reported `2x5` beating the in-band `4x3` — a scale the search
/// deliberately refuses. A check that disagrees with the code about
/// what the candidates ARE cannot say whether the code chose well
/// among them.
#[test]
fn smallest_clear_is_not_beaten_by_anything_else_that_clears() {
    for mode in TEXT_MODES {
        for content in [Content::Uppercase, Content::Text, Content::Icons] {
            let Some(pick) = smallest_clear(mode, content) else {
                continue;
            };
            for other in search_space() {
                if legibility(&BigTextStyle::new(other, mode), content) != Legibility::Clear {
                    continue;
                }
                assert!(
                    other.cols * other.rows >= pick.cols * pick.rows,
                    "{mode:?}/{content:?}: smallest_clear returned {}x{} ({} cells) but \
                     {}x{} ({} cells) also clears",
                    pick.cols,
                    pick.rows,
                    pick.cols * pick.rows,
                    other.cols,
                    other.rows,
                    other.cols * other.rows
                );
            }
        }
    }
}

/// The band is a CONSTRAINT, so the thing to guard is that it actually
/// constrains — and that it does so in both directions.
///
/// Measured by deleting each half in turn, not asserted: dropping the
/// TALL half (`rows <= MAX_STRETCH * cols`) reds this test alone, and
/// dropping the WIDE half reds four — this one plus the three that pin
/// published numbers. The tall half is the one worth having a guard
/// for, because nothing else notices it: the wide half is load-bearing
/// everywhere, so it could never rot quietly, and the tall half exists
/// only to stop `2x5`/`2x6` winning sextant uppercase on cost while
/// squeezing the glyph to two fifths of its width.
#[test]
fn the_search_space_holds_no_scale_that_distorts_past_the_band() {
    let space: Vec<GlyphScale> = search_space().collect();
    assert!(
        !space.is_empty(),
        "an empty search space would make every guard below vacuously true"
    );
    for scale in &space {
        assert!(
            scale.cols <= MAX_STRETCH * scale.rows,
            "{}x{} is stretched {}x wide, past MAX_STRETCH",
            scale.cols,
            scale.rows,
            scale.cols / scale.rows
        );
        assert!(
            scale.rows <= MAX_STRETCH * scale.cols,
            "{}x{} is stretched {}x tall, past MAX_STRETCH",
            scale.cols,
            scale.rows,
            scale.rows / scale.cols
        );
    }
    // The named out-of-band scales must be absent, not merely unlikely.
    for absent in [
        GlyphScale::COMPACT_WIDE,        // 6x2 — the operator's screenshot
        GlyphScale { cols: 6, rows: 1 }, // the old space's widest one-row
        GlyphScale { cols: 2, rows: 5 }, // what the wide-only cap let win
    ] {
        assert!(
            !space.contains(&absent),
            "{}x{} is out of band and must not be offered",
            absent.cols,
            absent.rows
        );
    }
    // And the band must not have eaten the space: the operator asked for
    // 6 rows to reach 12 columns, so that scale must BE there.
    assert!(
        space.contains(&GlyphScale { cols: 12, rows: 6 }),
        "12x6 is the widest scale the operator asked for; the band must admit it"
    );
    assert!(
        space.contains(&GlyphScale::square(2)),
        "square(2) sits on the band's wide edge and both edges are inclusive"
    );
}

/// **`smallest_clear`'s doc said `None` was the honest answer for
/// `HalfBlock`. It was not, and this guard is how I found out.**
///
/// I wrote it asserting that all three classes needed the wider search —
/// the flattering story, in which the old doc was right until my change
/// made it wrong. It went red on the first class it checked: halfblock
/// uppercase clears at 6x5, which was inside the OLD ceiling all along.
/// So the doc had been generalising a real result about *lowercase* into
/// a claim about the mode, and no test had ever asked.
///
/// Only mixed text actually needed the widening, at 7x5. The split is
/// pinned per class because the correction only means something if the
/// two halves stay distinguishable.
#[test]
fn only_halfblock_text_needed_the_wider_search_and_the_doc_claimed_all_three() {
    const OLD_CEILING: i32 = 6;
    for (content, expected, needed_widening) in [
        (Content::Uppercase, (6, 5), false),
        (Content::Text, (7, 5), true),
        (Content::Icons, (5, 3), false),
    ] {
        let pick = smallest_clear(MosaicMode::HalfBlock, content).unwrap_or_else(|| {
            panic!(
                "smallest_clear's doc says HalfBlock/{content:?} clears at {expected:?}; \
                 it answers None, so the doc tells callers a mode works when it does not"
            )
        });
        assert_eq!(
            (pick.cols, pick.rows),
            expected,
            "smallest_clear's doc prints this number for HalfBlock/{content:?}"
        );
        assert_eq!(
            pick.cols > OLD_CEILING,
            needed_widening,
            "the doc's correction turns on WHICH classes the old six-column ceiling was \
             hiding: it says {content:?} {} the widening, and {}x{} says otherwise",
            if needed_widening {
                "needed"
            } else {
                "did not need"
            },
            pick.cols,
            pick.rows
        );
    }
}

/// Every class in every mode now has an answer. Pinned because
/// `smallest_clear`'s doc says so in as many words, and because a
/// `None` creeping back in is the kind of regression a caller meets as
/// an `unwrap` in their own code.
#[test]
fn no_mode_and_class_pair_is_left_without_a_clear_scale() {
    for mode in [
        MosaicMode::Braille,
        MosaicMode::Sextant,
        MosaicMode::Quadrant,
        MosaicMode::HalfBlock,
    ] {
        for content in [Content::Uppercase, Content::Text, Content::Icons] {
            let pick = smallest_clear(mode, content)
                .unwrap_or_else(|| panic!("{mode:?}/{content:?} has no clear scale in the band"));
            assert!(
                pick.within_aspect_band(),
                "{mode:?}/{content:?} returned {}x{}, which is outside the band it searched",
                pick.cols,
                pick.rows
            );
        }
    }
}

/// The two picks the module's prose names by number, in the modes it
/// tells callers to use for text. If the rasterizer moves these, three
/// docs go wrong together — `FLOOR`'s sextant paragraph, `square`'s
/// braille-icon paragraph, and the scale table in `docs/api.md`.
#[test]
fn the_recommended_picks_the_docs_print_are_what_the_search_returns() {
    assert_eq!(
        smallest_clear(MosaicMode::Sextant, Content::Text).map(|s| (s.cols, s.rows)),
        Some((4, 4)),
        "FLOOR's doc sends sextant text here"
    );
    assert_eq!(
        smallest_clear(MosaicMode::Braille, Content::Icons).map(|s| (s.cols, s.rows)),
        Some((2, 2)),
        "square()'s doc sends braille icons here, instead of the out-of-band 3x1 it used \
         to recommend"
    );
    // And the replacement must beat what it replaced on the number too,
    // or the band cost the caller legibility rather than buying it.
    let old = closest_in(
        Content::Icons.alphabet(),
        GlyphScale { cols: 3, rows: 1 },
        MosaicMode::Braille,
    );
    let new = closest_in(
        Content::Icons.alphabet(),
        GlyphScale { cols: 2, rows: 2 },
        MosaicMode::Braille,
    );
    assert!(
        new.2 > old.2,
        "2x2 gives braille icons {} subpixels against 3x1's {} — square()'s doc claims \
         the in-band answer is also the better-measured one",
        new.2,
        old.2
    );
}

/// Report-only: the whole derived table, re-runnable rather than quoted.
///
/// `cargo test --release --lib scale_table -- --ignored --nocapture`
///
/// A scale table pasted into a message cannot be re-derived after the
/// rasterizer changes, and this module has already shipped two docs
/// whose numbers had drifted from the font. Marks: `C` clear, `m`
/// marginal, `x` distorted, `.` collides; the two numbers are the
/// closest pair in subpixels and the worst character's fidelity loss.
/// Out-of-band scales are printed in brackets — they are still real
/// scales a caller may construct, they are just not offered.
#[test]
#[ignore = "report-only: run explicitly with --nocapture"]
fn scale_table() {
    for mode in [
        MosaicMode::Braille,
        MosaicMode::Sextant,
        MosaicMode::Quadrant,
        MosaicMode::HalfBlock,
    ] {
        for content in [Content::Uppercase, Content::Text, Content::Icons] {
            eprintln!("\n{mode:?} / {content:?}");
            for rows in 1..=6 {
                let mut line = String::new();
                for cols in 2..=12 {
                    let scale = GlyphScale { cols, rows };
                    let style = BigTextStyle::new(scale, mode);
                    let mark = match legibility(&style, content) {
                        Legibility::Clear => 'C',
                        Legibility::Marginal => 'm',
                        Legibility::Distorted => 'x',
                        Legibility::Collides => '.',
                    };
                    let d = closest_pair_in(&style, content.alphabet())
                        .map(|(_, _, d)| d)
                        .unwrap_or(0);
                    let loss = least_faithful(&style, content).map_or(1.0, |(_, l)| l);
                    let cell = format!("{cols}x{rows} {mark}{d} {loss:.2}");
                    line.push_str(&if scale.within_aspect_band() {
                        format!("{cell:<15}")
                    } else {
                        format!("{:<15}", format!("[{cell}]"))
                    });
                }
                eprintln!("  {line}");
            }
            eprintln!(
                "  => smallest_clear: {:?}",
                smallest_clear(mode, content).map(|s| (s.cols, s.rows))
            );
        }
    }
}

/// My own bad advice, made falsifiable. I recommended `square(1)` for a
/// one-row navbar icon; measured, 2x1 puts the closest icon pair 1
/// subpixel apart in braille and **0** in quadrant, where `★` and `✗`
/// are the same picture. The doc now says two rows is the floor for an
/// icon that must be told apart from another icon — this is the guard
/// under that sentence.
#[test]
fn one_row_icons_are_not_distinguishable() {
    let one_row = GlyphScale::square(1);
    assert_eq!((one_row.cols, one_row.rows), (2, 1));
    assert_eq!(
        legibility(
            &BigTextStyle::new(one_row, MosaicMode::Quadrant),
            Content::Icons
        ),
        Legibility::Collides,
        "square(1) icons in quadrant used to render two glyphs identically; if that is \
         fixed, square(1)'s doc must stop warning about it"
    );
    for mode in TEXT_MODES {
        assert!(
            legibility(&BigTextStyle::new(one_row, mode), Content::Icons) < Legibility::Clear,
            "square(1) is documented as unsafe for a ROW of icons in {mode:?} too"
        );
        // Two rows is the claim, so two rows must actually fix it — and
        // the search is what says how WIDE, because square(2) does not
        // hold in both modes. See the next guard.
        let pick = smallest_clear(mode, Content::Icons).expect("icons clear somewhere");
        assert_eq!(
            pick.rows, 2,
            "two rows is the doc's claim for a row of icons in {mode:?}"
        );
        assert_eq!(
            legibility(&BigTextStyle::new(pick, mode), Content::Icons),
            Legibility::Clear
        );
    }
}

/// **`square(2)` is "the size an icon wants" in braille and NOT in
/// sextant, and the doc said it flatly.** Measured after
/// [`least_faithful`] arrived: braille 4x2 loses 0.25 of the footprint,
/// sextant 4x2 loses 0.36 — over [`FIDELITY_MAX`] — and sextant's answer
/// is 3x2 instead.
///
/// The reason is the module's own vertical crop, one section further
/// down the same file. A row of icons crops as a RUN, so the box is the
/// union of `⚠ ☑ → ●` and taller than any single icon; the natural
/// aspect of that box is 3 columns per 2 rows, not 4. `square(2)` is the
/// answer for a LONE icon, whose box really is about square.
///
/// This guard exists because the flat claim was in `square`'s doc, in
/// `COMPACT`'s doc and in `docs/api.md`, and one measurement contradicts
/// all three. It is deliberately two-sided: if braille ever stops
/// clearing at 4x2, or sextant ever starts, the prose that now
/// distinguishes them is wrong again in the other direction.
#[test]
fn the_badge_scale_is_an_icon_answer_in_braille_and_a_distortion_in_sextant() {
    let badge = GlyphScale::square(2);
    assert_eq!((badge.cols, badge.rows), (4, 2));
    assert_eq!(
        legibility(
            &BigTextStyle::new(badge, MosaicMode::Braille),
            Content::Icons
        ),
        Legibility::Clear,
        "braille has the subpixels to hold a badge at 4x2"
    );
    assert_eq!(
        legibility(
            &BigTextStyle::new(badge, MosaicMode::Sextant),
            Content::Icons
        ),
        Legibility::Distorted,
        "sextant 4x2 stretches the icon RUN's box horizontally; the doc must not send \
         sextant callers here"
    );
    assert_eq!(
        smallest_clear(MosaicMode::Sextant, Content::Icons).map(|s| (s.cols, s.rows)),
        Some((3, 2)),
        "and the search must name the narrower answer the prose now points at"
    );
    // The pairwise measure cannot see any of this: it PREFERS the wide
    // one. Without this line the guard above would pass just as well if
    // 4x2 had simply become a bad scale in sextant by every measure.
    let (_, _, wide) = closest_pair_in(
        &BigTextStyle::new(badge, MosaicMode::Sextant),
        Content::Icons.alphabet(),
    )
    .unwrap();
    let (_, _, narrow) = closest_pair_in(
        &BigTextStyle::new(GlyphScale { cols: 3, rows: 2 }, MosaicMode::Sextant),
        Content::Icons.alphabet(),
    )
    .unwrap();
    assert!(
        wide > narrow,
        "4x2 measures {wide} subpixels against 3x2's {narrow} — the shape term is the only \
         thing standing between the reader and the wider scale"
    );
}

/// `Content::alphabet` is what every verdict is computed from, so an
/// empty or one-character class would make `closest_pair` return `None`
/// and `legibility` report `Collides` for a class that is simply
/// unmeasured — a false red that looks like a real one.
#[test]
fn every_content_class_has_enough_characters_to_measure() {
    for content in [Content::Uppercase, Content::Text, Content::Icons] {
        assert!(
            content.alphabet().chars().count() >= 2,
            "{content:?} cannot have a closest PAIR"
        );
        let style = BigTextStyle::new(GlyphScale::FLOOR, MosaicMode::Braille);
        assert!(
            super::closest_pair(&style, content).is_some(),
            "{content:?} has characters the embedded font cannot draw, so its verdict is \
             computed from fewer glyphs than it claims"
        );
    }
}

/// The aspect trap, as a test rather than a warning nobody reads: a
/// square footprint is twice as many columns as rows, because a cell is
/// half as wide as it is tall.
#[test]
fn a_square_icon_is_twice_as_many_columns_as_rows() {
    for rows in 1..=6 {
        let s = GlyphScale::square(rows);
        assert_eq!(s.cols, rows * 2, "square({rows}) is not square on screen");
    }
    // The shape that prompted this: 6x6 is NOT a big square icon.
    assert_ne!(GlyphScale { cols: 6, rows: 6 }, GlyphScale::square(6));
}

/// `measure` is what a caller commits layout on, so it must equal what
/// the renderer actually produces — not approximately.
#[test]
fn measure_agrees_with_the_grid_the_renderer_returns() {
    for text in ["A", "AGORA", "AGORA-TUI"] {
        for scale in [GlyphScale::TIGHT, GlyphScale::FLOOR, GlyphScale::square(2)] {
            let want = measure(text, scale).expect("measure");
            let grid = render(text, scale, MosaicMode::Sextant, INK, GROUND).expect("render");
            assert_eq!(
                (grid.cols() as i32, grid.rows() as i32),
                (want.w, want.h),
                "measure lied about {text:?} at {}x{}",
                scale.cols,
                scale.rows
            );
        }
    }
}

/// The documented cost, pinned: a nine-character title at the floor is
/// a banner, not a navbar. If this number moves, every caller's layout
/// budget moved with it. Nine glyphs at 4 columns plus eight
/// one-column gaps and no outer padding — 44, not the 45 an earlier
/// scratch renderer reported by padding its left edge.
#[test]
fn a_nine_character_title_costs_44_columns_and_3_rows_at_the_floor() {
    assert_eq!(
        measure("AGORA-TUI", GlyphScale::FLOOR),
        Some(Size::new(44, 3))
    );
}

/// A missing glyph must NAME itself and refuse. A dropped `é` renders
/// "Café" as "Caf", which looks like a finished word — this is the
/// whole reason the API returns a Result.
#[test]
fn an_accented_character_is_refused_by_name_not_dropped() {
    let err = rasterize("Café", GlyphScale::FLOOR, MosaicMode::Sextant, INK, GROUND)
        .expect_err("the font has no accented glyphs, so this must not succeed");
    assert_eq!(err, BigTextError::UnsupportedChar('é'));
    assert!(
        err.to_string().contains('é'),
        "the message must name the character a caller has to fix: {err}"
    );
}

/// Ink extent of one character slot of a multi-character run, as
/// (topmost row, bottommost row). Reading a SLOT rather than a
/// separately-rendered glyph is the whole point: rendered alone, a run
/// crop and a per-glyph crop are the same operation, so a test that
/// rasterizes `"o"` on its own cannot tell them apart. It has to see
/// `o` sharing a run with something taller.
fn slot_extent(text: &str, slot: u32, scale: GlyphScale) -> (u32, u32) {
    let bmp = rasterize(text, scale, MosaicMode::Braille, INK, GROUND).expect("rasterize");
    let (sub_w, _) = (2u32, 4u32); // braille
    let cell_w = scale.cols as u32 * sub_w;
    let x0 = slot * (cell_w + sub_w);
    let inked: Vec<u32> = (0..bmp.height())
        .filter(|y| (x0..x0 + cell_w).any(|x| bmp.get(x, *y) == Some(INK)))
        .collect();
    (
        *inked.first().expect("slot has ink"),
        *inked.last().unwrap(),
    )
}

/// The crop is over the whole RUN, not per glyph. Cropped per glyph, a
/// round lowercase `o` would be stretched to the height of `O` and the
/// two would come out the same height; cropping the run keeps `o`
/// short and sitting on the baseline.
#[test]
fn lowercase_keeps_its_height_because_the_crop_is_run_wide_not_per_glyph() {
    let scale = GlyphScale { cols: 6, rows: 4 };
    let (cap_top, cap_bottom) = slot_extent("Oo", 0, scale);
    let (low_top, low_bottom) = slot_extent("Oo", 1, scale);
    assert!(
        low_top > cap_top,
        "`o` starts at row {low_top} and `O` at {cap_top} — the x-height was \
         stretched to cap height, so the crop is per glyph, not run-wide"
    );
    assert!(
        cap_bottom.abs_diff(low_bottom) <= 1,
        "`O` ends at {cap_bottom} and `o` at {low_bottom} — they must share a baseline"
    );
}

/// A descender must reach below the baseline rather than being lifted
/// onto it. In a run with a capital, `p` has to end LOWER than `P`.
#[test]
fn a_descender_still_descends_below_the_baseline_it_shares() {
    let scale = GlyphScale { cols: 6, rows: 4 };
    let (_, cap_bottom) = slot_extent("Pp", 0, scale);
    let (_, low_bottom) = slot_extent("Pp", 1, scale);
    assert!(
        low_bottom > cap_bottom,
        "`p` ends at row {low_bottom} and `P` at {cap_bottom} — the descender \
         was cropped away or lifted onto the baseline"
    );
}

/// Empty and degenerate inputs are refused, not rendered as something.
#[test]
fn nothing_to_draw_and_no_cells_are_named_errors() {
    assert_eq!(
        rasterize("", GlyphScale::FLOOR, MosaicMode::Sextant, INK, GROUND),
        Err(BigTextError::NothingToDraw)
    );
    assert_eq!(
        rasterize("   ", GlyphScale::FLOOR, MosaicMode::Sextant, INK, GROUND),
        Err(BigTextError::NothingToDraw),
        "an all-space run has no baseline to crop to"
    );
    let flat = GlyphScale { cols: 4, rows: 0 };
    assert_eq!(
        rasterize("A", flat, MosaicMode::Sextant, INK, GROUND),
        Err(BigTextError::EmptyScale(flat))
    );
    assert_eq!(measure("A", flat), None);
}

/// Every mosaic mode is wired to its real subpixel density. A mode
/// mapped to the wrong density renders a squashed letter and nothing
/// else notices.
#[test]
fn each_mode_gets_the_subpixel_density_it_advertises() {
    let scale = GlyphScale::FLOOR;
    for (mode, (sw, sh)) in [
        (MosaicMode::HalfBlock, (1, 2)),
        (MosaicMode::Quadrant, (2, 2)),
        (MosaicMode::Sextant, (2, 3)),
        (MosaicMode::Braille, (2, 4)),
    ] {
        let bmp = rasterize("A", scale, mode, INK, GROUND).expect("rasterize");
        assert_eq!(
            (bmp.width(), bmp.height()),
            (scale.cols as u32 * sw, scale.rows as u32 * sh),
            "{mode:?} rasterized at the wrong density"
        );
    }
}

/// `Sampling` is a real trade, and this is the number behind the doc
/// claim. Area averaging keeps letters further apart than point
/// sampling at the floor — if that stops being true, the doc telling
/// readers to keep it on for arbitrary text is wrong.
#[test]
fn area_averaging_keeps_letters_further_apart_than_point_sampling() {
    let base = BigTextStyle::new(GlyphScale::FLOOR, MosaicMode::Braille);
    let area = closest_pair_with(&base.sampling(Sampling::AreaAverage));
    let near = closest_pair_with(&base.sampling(Sampling::Nearest));
    assert!(
        area > near,
        "area averaging ({area}) no longer beats nearest ({near}) at the floor, \
         so the documented reason to default to it is gone"
    );
}

/// Pins the two numbers the `GlyphWeight::Bold` doc quotes: distinctness
/// rises at 3x3 and is unchanged at four rows.
///
/// Note what this does NOT claim. Dilation adds ink to every glyph, so
/// a rise in pairwise distance is partly mechanical and is not a
/// verdict on how bold LOOKS — at three rows it can close a counter.
/// The doc says so; this test only keeps the quoted figures honest.
#[test]
fn synthetic_bold_raises_pairwise_distinctness_at_3x3_and_is_neutral_from_four_rows() {
    let tight = BigTextStyle::new(GlyphScale::TIGHT, MosaicMode::Braille);
    let regular = closest_pair_with(&tight.weight(GlyphWeight::Regular));
    let bold = closest_pair_with(&tight.weight(GlyphWeight::Bold));
    assert!(
        bold > regular,
        "bold ({bold}) no longer exceeds regular ({regular}) at 3x3 — the figure \
         quoted in the GlyphWeight::Bold doc is stale"
    );

    let roomy = BigTextStyle::new(GlyphScale { cols: 4, rows: 4 }, MosaicMode::Braille);
    assert_eq!(
        closest_pair_with(&roomy.weight(GlyphWeight::Bold)),
        closest_pair_with(&roomy.weight(GlyphWeight::Regular)),
        "the docs say bold changes nothing at four rows and above"
    );
}

/// Bold must actually add ink, not merely differ. A dilation that
/// shifted instead of thickened would pass a "they differ" check.
#[test]
fn the_bold_weight_adds_ink_rather_than_moving_it() {
    let style = BigTextStyle::new(GlyphScale { cols: 6, rows: 4 }, MosaicMode::Braille);
    let ink_of = |f| {
        let bmp = rasterize_with("E", &style.weight(f), INK, GROUND).expect("rasterize");
        (0..bmp.height())
            .flat_map(|y| (0..bmp.width()).map(move |x| (x, y)))
            .filter(|(x, y)| bmp.get(*x, *y) == Some(INK))
            .count()
    };
    let (regular, bold) = (ink_of(GlyphWeight::Regular), ink_of(GlyphWeight::Bold));
    assert!(
        bold > regular,
        "bold `E` carries {bold} subpixels against regular's {regular} — \
         the dilation moved the stem instead of thickening it"
    );
}

/// The simple call and the explicit one must agree on the defaults, or
/// the two-call API quietly renders two different things.
#[test]
fn the_simple_call_equals_the_explicit_call_at_its_defaults() {
    let scale = GlyphScale::FLOOR;
    let simple = rasterize("AG", scale, MosaicMode::Sextant, INK, GROUND).expect("simple");
    let explicit = rasterize_with(
        "AG",
        &BigTextStyle::new(scale, MosaicMode::Sextant),
        INK,
        GROUND,
    )
    .expect("explicit");
    assert_eq!(simple, explicit);
    assert_eq!(GlyphWeight::default(), GlyphWeight::Regular);
    assert_eq!(Sampling::default(), Sampling::AreaAverage);
}

/// The guard that was missing, and it cost a blank example to notice:
/// **`render` must PAINT.**
///
/// Every other test here reads `rasterize`'s bitmap or `render`'s
/// dimensions, and a correctly-sized grid of blank cells satisfies all
/// of them. It happened for real — sextant against a transparent ground
/// returned 27 empty cells and nothing went red. Assert on glyphs.
#[test]
fn render_paints_glyph_cells_in_every_mode_not_an_empty_grid() {
    let opaque = Rgba::rgb(0, 0, 0);
    let text = "AGORA";
    let scale = GlyphScale::FLOOR;
    for mode in [
        MosaicMode::HalfBlock,
        MosaicMode::Quadrant,
        MosaicMode::Sextant,
        MosaicMode::Braille,
    ] {
        let style = BigTextStyle::new(scale, mode);
        let grid = render_with(text, &style, INK, opaque).expect("render");
        // Per CHARACTER, not per grid: a density bar polices the mode's
        // subpixel count, while "every slot shows something" is the
        // property that actually distinguishes drawn from blank — and it
        // also catches one letter painting while the rest do not.
        for slot in 0..text.chars().count() as i32 {
            let x0 = slot * (scale.cols + 1);
            let painted = (x0..x0 + scale.cols)
                .flat_map(|c| (0..grid.rows() as i32).map(move |r| (c, r)))
                .filter(|(c, r)| {
                    grid.get(*c as u32, *r as u32)
                        .is_some_and(|cell| cell.ch != ' ')
                })
                .count();
            assert!(
                painted > 0,
                "{mode:?} left character {slot} of {text:?} entirely blank — the \
                 grid is the right size and nothing is drawn"
            );
        }
    }
}

/// A transparent ground is refused for the two-colour fits by name,
/// because it renders blank rather than failing. The luminance modes
/// keep working, so the refusal must not be a blanket one.
#[test]
fn a_transparent_ground_is_refused_only_where_it_would_render_blank() {
    for mode in [MosaicMode::Quadrant, MosaicMode::Sextant] {
        let style = BigTextStyle::new(GlyphScale::FLOOR, mode);
        let err = render_with("AG", &style, INK, Rgba::TRANSPARENT)
            .err()
            .unwrap_or_else(|| panic!("{mode:?} must refuse a transparent ground"));
        assert_eq!(err, BigTextError::TransparentGround(mode));
        assert!(
            err.to_string().contains("transparent ground"),
            "the message must name the problem a caller has to fix: {err}"
        );
    }
    for mode in [MosaicMode::Braille, MosaicMode::HalfBlock] {
        let style = BigTextStyle::new(GlyphScale::FLOOR, mode);
        let grid = render_with("AG", &style, INK, Rgba::TRANSPARENT).unwrap_or_else(|e| {
            panic!("{mode:?} thresholds by luminance and must accept a transparent ground: {e}")
        });
        assert!(
            grid.cells().iter().any(|c| c.ch != ' '),
            "{mode:?} accepted a transparent ground but painted nothing"
        );
    }
}

// `LOWER_DIGITS` and `ICON_SET` used to live here, as private copies of
// what `Content::Text.alphabet()` and `Content::Icons.alphabet()` now
// return. They went with the hand-listed `scale_sweep_report` that was
// their only reader: a table of twelve scales somebody typed out, in a
// module whose recorded lesson is that a hand-written ladder omits the
// answer. `scale_table` derives the same report from `search_space()`.

/// **A filled glyph renders SOLID, and reading the grid by its
/// characters alone says it renders as a hollow ring.**
///
/// This guard exists because I reported the ring to the operator as a
/// possible rasterizer defect. It is not a defect and there is no ring.
/// A cell whose subpixels are entirely ink is emitted as a SPACE with
/// the ink as its BACKGROUND (`mosaic::tests::braille_uniform_cell_blank`
/// pins that at the encoder) — which paints a fully solid cell, and
/// paints it better than `⣿` would, since braille dots are drawn with
/// gaps in most terminal fonts.
///
/// What went wrong was the instrument: a probe that printed `cell.ch`
/// and ignored `cell.bg`, so every solid interior cell read as blank
/// and the disc appeared to be an outline. The same reading is what
/// produced the `● => ⣾⠀⠀⠀⣷⣦` dump in the white-bars diagnosis — those
/// `⠀` cells were not gaps, they were the most solid cells on the row.
///
/// So the guard asserts BOTH halves: that the glyph column alone tells
/// the false story, and that the colours tell the true one. If the
/// encoder ever changes so the false story stops being tempting, the
/// first assertion reds and this comment can go with it.
#[test]
fn a_filled_glyph_is_solid_and_the_glyph_column_alone_reports_a_hollow_ring() {
    let style = BigTextStyle::new(GlyphScale { cols: 6, rows: 6 }, MosaicMode::Braille);
    let grid = render_with("\u{25cf}", &style, INK, GROUND).expect("the disc renders");

    // Interior of the disc: the middle cell row, away from the edges.
    let mid = grid.rows() / 2;
    let interior: Vec<_> = (1..grid.cols() - 1)
        .map(|c| *grid.get(c, mid).expect("cell in range"))
        .collect();
    assert!(
        !interior.is_empty(),
        "no interior cells to inspect — the scale or the crop changed"
    );

    // THE TRUE READING: every interior cell paints solid ink, whether it
    // does so with dots or with an ink background.
    for cell in &interior {
        let solid_by_background = cell.bg == INK;
        let solid_by_dots = cell.ch == '\u{28FF}' && cell.fg == INK;
        assert!(
            solid_by_background || solid_by_dots,
            "interior cell of a filled disc is neither ink-backed nor fully dotted: \
             ch={:?} fg={:?} bg={:?}",
            cell.ch,
            cell.fg,
            cell.bg
        );
    }

    // THE FALSE READING, pinned so the trap is on the record: judged by
    // the glyph alone, the interior is empty and the disc looks hollow.
    let blank_glyphs = interior
        .iter()
        .filter(|c| c.ch == ' ' || c.ch == '\u{2800}')
        .count();
    assert_eq!(
        blank_glyphs,
        interior.len(),
        "every interior cell should carry a BLANK glyph — that is precisely why reading \
         cell.ch without cell.bg reported a ring. If this reds, the encoder changed and \
         the warning in this test's doc no longer applies"
    );
}

/// THE guard for the operator's report. `6x2` braille icons measure 16
/// subpixels apart — genuinely distinct — and render as two white bars.
/// Before `least_faithful` existed, `legibility` said `Clear` and this
/// crate's own example printed that verdict directly above the bars.
///
/// Two-sided on purpose. The verdict must be `Distorted` AND the
/// pairwise number must still be large: a guard that only asserted "not
/// Clear" would pass if the pair distance had collapsed, and then the
/// case it is named for — *right about its own question, blind to the
/// one that mattered* — would no longer be in the file.
///
/// Falsified by deleting the `unfaithful || !within_aspect_band` line
/// from `legibility`: RED here, with `Clear`.
#[test]
fn the_scale_the_operator_photographed_is_refused_on_shape_not_on_distance() {
    let bars = BigTextStyle::new(GlyphScale::COMPACT_WIDE, MosaicMode::Braille);
    assert_eq!(
        legibility(&bars, Content::Icons),
        Legibility::Distorted,
        "6x2 braille icons is the reported screenshot; the verdict must name the shape"
    );
    let (_, _, apart) = closest_pair_in(&bars, Content::Icons.alphabet()).unwrap();
    assert!(
        apart > MARGINAL_MAX * 4,
        "the pair measure must still be emphatic ({apart} subpixels) — the whole point is \
         that it is right and irrelevant"
    );
    // And the two grounds must be separable, or `Distorted` would be
    // one verdict pretending to be two.
    let (_, loss) = least_faithful(&bars, Content::Icons).unwrap();
    assert!(
        loss <= FIDELITY_MAX,
        "6x2 icons measures {loss:.2}, INSIDE the fidelity bar — it is the aspect band that \
         catches this one, which is why both terms are applied"
    );
    assert!(
        !GlyphScale::COMPACT_WIDE.within_aspect_band(),
        "...and the band is what refuses it"
    );
}

/// The mirror case, and the reason the band is not enough on its own:
/// `2x3` braille icons sits well inside the band (`cols` and `rows`
/// within 2x of each other), measures 4 subpixels apart, and is a
/// two-and-a-bit-times VERTICAL stretch of the icon run's box.
///
/// The band cannot see it, because the band is referenced to the
/// uncropped 8x16 glyph box and the renderer crops. `least_faithful` is
/// referenced to what is actually drawn, so it can.
///
/// Falsified by raising `FIDELITY_MAX` to 0.5: RED, verdict returns to
/// `Clear`.
#[test]
fn a_scale_inside_the_band_is_still_refused_when_the_shape_is_gone() {
    let tall = BigTextStyle::new(GlyphScale { cols: 2, rows: 3 }, MosaicMode::Braille);
    assert!(
        tall.scale.within_aspect_band(),
        "2x3 is the case the band passes — if it stopped, this guard would be testing the band"
    );
    let (worst, loss) = least_faithful(&tall, Content::Icons).unwrap();
    assert!(
        loss > FIDELITY_MAX,
        "{worst} at 2x3 braille measures {loss:.2}; the fidelity term must be what refuses it"
    );
    assert_eq!(legibility(&tall, Content::Icons), Legibility::Distorted);
}

/// A character measured ALONE and the same character measured inside its
/// class's run are different questions, and this module got the second
/// one wrong first.
///
/// `z` on its own is cropped to its own x-height and stretched to fill
/// all three rows of `4x3`; inside the mixed-text run it keeps its
/// height, because the run's box reaches from `b` to `g`. The first
/// reading condemned `GlyphScale::FLOOR` for prose it renders perfectly
/// well.
///
/// The assertion is the INEQUALITY, not either number: the point is that
/// the two disagree by enough to change a verdict, so a future reader
/// cannot quietly swap one for the other.
#[test]
fn a_character_alone_is_measured_harder_than_the_same_character_in_its_run() {
    let style = BigTextStyle::new(GlyphScale::FLOOR, MosaicMode::Braille);
    let alone = fidelity_loss('z', &style).unwrap();
    let in_run = run_loss_of('z', &style, Content::Text);
    assert!(
        alone > in_run + 0.1,
        "z alone measures {alone:.2} and in the mixed-text run {in_run:.2}; if these ever \
         converge, the distinction this module draws between them is gone"
    );
    assert!(
        alone > FIDELITY_MAX && in_run < FIDELITY_MAX,
        "the gap has to straddle the bar to matter: alone {alone:.2}, in run {in_run:.2}"
    );
    assert_eq!(
        legibility(&style, Content::Text),
        Legibility::Clear,
        "FLOOR renders braille prose fine, and the alone-reading said otherwise"
    );
}

/// `least_faithful` reports the WORST character, so the run-context loss
/// of one NAMED character is not on the public surface. This re-derives
/// it through the module's own `slot_loss`, the same arithmetic
/// `least_faithful` runs, rather than a private copy of it.
fn run_loss_of(target: char, style: &BigTextStyle, content: Content) -> f32 {
    let alphabet = content.alphabet();
    let glyphs: Vec<[u8; GLYPH_H as usize]> = alphabet
        .chars()
        .map(|c| dilate(lookup(c).expect("glyph"), style.weight))
        .collect();
    let (top, bottom) = ink_rows(&glyphs).expect("the run has ink");
    let bmp = rasterize_with(alphabet, style, INK, GROUND).expect("the run rasterizes");
    let idx = alphabet
        .chars()
        .position(|c| c == target)
        .expect("in class");
    let (per_col, _) = subpixels(style.mode);
    let stride = style.scale.cols as u32 * per_col + per_col;
    let x0 = idx as u32 * stride;
    slot_loss(&glyphs[idx], top, bottom, style, |x, y| {
        bmp.get(x0 + x, y) == Some(INK)
    })
    .expect("measurable")
}

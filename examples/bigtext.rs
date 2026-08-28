//! bigtext — text and icons drawn several cells tall: pick a size and
//! see it.
//!
//! Demonstrates: `gfx::bigtext` across the four axes that decide whether
//! large text reads — SIZE, SYMBOLS, SAMPLING and WEIGHT — with every
//! combination reachable from the keyboard, because none of these can be
//! settled anywhere but on your own terminal in your own font.
//!
//! **Size.** ↑/↓ walks a sweep grouped by row count, widening within
//! each group. Rows cost far more screen than columns — a cell is about
//! twice as tall as it is wide — so the width sweep is usually where the
//! answer is. It opens on whatever `smallest_clear` returns for the
//! starting mode rather than on a rung written down here. Rungs marked
//! `!` are outside the aspect band and are in the list to be looked at,
//! not used.
//!
//! **Symbols** (`s`). Braille packs the most subpixels per cell and
//! often looks the WEAKEST, because terminals draw its dots with gaps and
//! a letter reads as a constellation. Sextants are solid ink at a lower
//! density. Quadrants are universal and coarse.
//!
//! **Sampling** (`a`). Area averaging weights each source pixel by how
//! much of it the target covers; point sampling takes one pixel per
//! target. Averaging keeps letters further APART — at 4x3 the closest
//! pair is 8 subpixels rather than 4 — while point sampling gives harder
//! edges. Solid display type at three rows often looks better point
//! sampled; arbitrary text needs the margin.
//!
//! **Weight** (`w`). The crate carries one font, so this is the CSS
//! `font-weight` axis, not a family: `Bold` is synthetic, the same glyphs
//! dilated one pixel. It measurably survives the tightest scales better
//! and changes nothing from four rows up.
//!
//! **The aspect trap.** A "6x6" icon is six cells wide and twelve
//! half-cells tall — stretched, not big. A square footprint is
//! `GlyphScale::square(rows)`, i.e. `cols == 2 * rows`; the trap panel
//! shows that beside the naive same-number shape.
//!
//! **The trap has a second half, and this example is where it was
//! caught.** Walk to `6x2` and switch to icons: `●` and `◆` come out as
//! two identical white bars, and the readout beside them says 16
//! subpixels apart. Both are correct. `legibility` measures whether two
//! rasterizations DIFFER, never whether either still looks like its
//! glyph, and more columns always buys more difference — so an
//! unbounded search walks toward the widest, ugliest scale and calls it
//! best. `GlyphScale::within_aspect_band` is the bound that stops it,
//! and the `aspect` line in the readout tells you which side of it you
//! are standing on.
//!
//! **The MEASURED panel is the point of the layout.** Everything above
//! is a claim about legibility, and a claim you cannot check is
//! decoration. The right-hand panel rasterizes the three content classes
//! AT THE CURRENT SETTINGS and prints the closest pair in each — the
//! same measurement `bigtext_tests.rs::scale_table` prints, run live on
//! your keypress. Watch `lower+digits` collapse to 0 at 3x3 while
//! `icons` stays wide: one global legibility floor cannot describe that,
//! which is why the panel reports three numbers and not one.
//!
//! Keys: ↑/↓ size · s symbols · w weight · a sampling · q quits.
//!
//! The key letters spell their axes, with one deliberate exception:
//! SAMPLING keeps `a`, because `s` goes to SYMBOLS and the two words
//! collide on their first letter. `a` is for the area averaging it
//! toggles.
//!
//! Docs: docs/api.md (gfx::bigtext).
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::gfx::bigtext::{
    self, BigTextStyle, Content, GlyphScale, GlyphWeight, Legibility, Sampling,
};
use abstracttui::gfx::mosaic::MosaicMode;
use abstracttui::prelude::*;
use abstracttui::ui::Canvas;
use abstracttui::widgets::{Block, BorderKind};

/// The sweep, two rows first.
///
/// The two-row family used to be missing entirely, and that omission
/// was the bug: `has_margin` refused 4x2 for having two rows while the
/// crate offered 3x3, which measures WORSE (it renders two lowercase
/// characters identically). Rows are also the expensive axis — a cell
/// is about twice as tall as it is wide — so the answer is usually
/// found by spending columns, and the sweep now starts where the
/// answers are.
///
/// **`5x2` and `6x2` are in this list on purpose, and they are out of
/// the aspect band.** They are here to be LOOKED at: switch to icons at
/// 6x2 and the row of glyphs is a row of white bars, while the readout
/// beside it reports them as clearly apart. That is the exact pair of
/// screen and number that got the band added, and an example that
/// quietly omitted the bad scale would have hidden the lesson.
const SCALES: [GlyphScale; 22] = [
    GlyphScale { cols: 2, rows: 2 },
    GlyphScale { cols: 3, rows: 2 },
    GlyphScale { cols: 4, rows: 2 },
    GlyphScale { cols: 5, rows: 2 }, // out of band
    GlyphScale { cols: 6, rows: 2 }, // out of band — the white bars
    GlyphScale { cols: 3, rows: 3 },
    GlyphScale { cols: 4, rows: 3 },
    GlyphScale { cols: 5, rows: 3 },
    GlyphScale { cols: 6, rows: 3 },
    GlyphScale { cols: 4, rows: 4 },
    GlyphScale { cols: 5, rows: 4 },
    GlyphScale { cols: 6, rows: 4 },
    GlyphScale { cols: 8, rows: 4 },
    GlyphScale { cols: 4, rows: 5 },
    GlyphScale { cols: 5, rows: 5 },
    GlyphScale { cols: 6, rows: 5 },
    GlyphScale { cols: 7, rows: 5 },
    GlyphScale { cols: 10, rows: 5 },
    GlyphScale { cols: 4, rows: 6 },
    GlyphScale { cols: 5, rows: 6 },
    GlyphScale { cols: 6, rows: 6 },
    GlyphScale { cols: 12, rows: 6 },
];

/// Where the sweep opens: whatever `smallest_clear` returns for the
/// mode this example starts in, found in the list rather than written
/// down.
///
/// It was `const START: usize = 4` — index of 6x2, "what
/// `smallest_clear(Sextant, Content::Text)` returns". Then the aspect
/// band changed that answer to 4x4 and the constant did not, so the
/// example opened on a scale its own readout no longer recommended.
/// A hardcoded index into a derived answer is a copy, and copies drift.
fn start_index() -> usize {
    bigtext::smallest_clear(MODES[0].0, Content::Text)
        .and_then(|want| SCALES.iter().position(|s| *s == want))
        .unwrap_or(0)
}

const MODES: [(MosaicMode, &str); 3] = [
    (MosaicMode::Sextant, "sextant 2x3 · solid ink"),
    (MosaicMode::Braille, "braille 2x4 · densest, dotted"),
    (MosaicMode::Quadrant, "quadrant 2x2 · universal, coarse"),
];

const WEIGHTS: [(GlyphWeight, &str); 2] = [
    (GlyphWeight::Regular, "regular"),
    (GlyphWeight::Bold, "bold (synthetic)"),
];

const SAMPLINGS: [(Sampling, &str); 2] = [
    (Sampling::AreaAverage, "area average"),
    (Sampling::Nearest, "point sampled"),
];

const BANNER: &str = "AGORA";

/// The three content classes, by their engine names.
///
/// The alphabets live in `Content::alphabet()` and the measurement in
/// `bigtext::closest_pair` — this example used to carry its own copy of
/// both. A demo that reimplements the thing it demonstrates can agree
/// with itself while disagreeing with the library, which is the one
/// thing an example must never do.
const CLASSES: [(&str, Content); 3] = [
    ("UPPER", Content::Uppercase),
    ("lower+digits", Content::Text),
    ("icons", Content::Icons),
];

/// The width of the readout panel, in cells. Wide enough for the longest
/// label (`lower+digits`, 12) plus a pair, a subpixel distance and a
/// fidelity loss — four columns, because the verdict now rests on two
/// measurements and showing one of them is how this panel came to print
/// `clearly apart` over a row of white bars.
const PANEL_W: i32 = 34;

/// Column offsets inside that panel, so the header and the rows cannot
/// drift apart.
const COL_PAIR: i32 = 13;
const COL_APART: i32 = 19;
const COL_SHAPE: i32 = 24;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("bigtext: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(96, 34));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let scale = cx.signal(start_index());
        let mode = cx.signal(0usize);
        let weight = cx.signal(0usize);
        let sampling = cx.signal(0usize);
        Element::new()
            .style(LayoutStyle::fill())
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                mode.update(|m| *m = (*m + 1) % MODES.len())
            })
            .shortcut(KeyChord::plain(Key::Char('w')), move |_| {
                weight.update(|w| *w = (*w + 1) % WEIGHTS.len())
            })
            .shortcut(KeyChord::plain(Key::Char('a')), move |_| {
                sampling.update(|s| *s = (*s + 1) % SAMPLINGS.len())
            })
            .shortcut(KeyChord::plain(Key::Up), move |_| {
                scale.update(|s| *s = s.saturating_sub(1))
            })
            .shortcut(KeyChord::plain(Key::Down), move |_| {
                scale.update(|s| *s = (*s + 1).min(SCALES.len() - 1))
            })
            // Signals are read HERE, not inside a draw closure: a tracked
            // read during paint is a region that never repaints (RT1-2).
            .child(dyn_view(LayoutStyle::fill(), move || {
                let picks = Picks {
                    scale: scale.get(),
                    mode: mode.get(),
                    weight: weight.get(),
                    sampling: sampling.get(),
                };
                page(picks)
            }))
            .build()
    })?;
    app.run()
}

/// The four axis positions, passed to the draw closures as plain values.
#[derive(Clone, Copy)]
struct Picks {
    scale: usize,
    mode: usize,
    weight: usize,
    sampling: usize,
}

impl Picks {
    fn style(&self) -> BigTextStyle {
        BigTextStyle::new(SCALES[self.scale], MODES[self.mode].0)
            .weight(WEIGHTS[self.weight].0)
            .sampling(SAMPLINGS[self.sampling].0)
    }
}

/// The whole page: a title strip, the specimens down the left, the
/// measurement panel down the right, and the keys at the foot.
///
/// Laid out as ELEMENTS rather than as one big draw closure with hand-
/// computed `y` offsets. The old version was the latter, and it showed:
/// everything piled into the top-left corner with a third of the screen
/// blank underneath, because a chain of `y += …` has no idea how much
/// room it has.
fn page(picks: Picks) -> View {
    let style = picks.style();
    let scale = style.scale;
    // The banner and the proof are the two things that must survive a
    // short terminal, so they grow and everything else is fixed.
    let specimen_h = scale.rows + 4;
    Element::new()
        .style(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
        )
        .child(title_strip(picks))
        .child(
            Element::new()
                .style(LayoutStyle::row().grow(1.0).gap(1))
                .child(
                    Element::new()
                        .style(LayoutStyle::column().grow(1.0).gap(0))
                        .child(specimen_panel(picks, specimen_h))
                        // Takes the slack, because it is the panel that
                        // can USE it: one more row of screen is one more
                        // row of alphabet you can judge.
                        .child(pairs_panel(picks))
                        .child(trap_panel(picks))
                        .build(),
                )
                .child(
                    Element::new()
                        .style(LayoutStyle::column().w(PANEL_W).shrink(0.0))
                        .child(measured_panel(picks))
                        .child(sweep_panel(picks))
                        .build(),
                )
                .build(),
        )
        .child(
            Element::new()
                .style(LayoutStyle::line(1).shrink(0.0))
                .draw(move |canvas, rect| {
                    let t = current_theme().tokens;
                    canvas.fill(rect, ' ', t.text_faint, t.bg);
                    canvas.print(
                        Point::new(rect.x + 2, rect.y),
                        "↑/↓ size · s symbols · w weight · a sampling · q quit",
                        t.text_faint,
                        t.bg,
                    );
                })
                .build(),
        )
        .build()
}

/// One line, four axes, no hunting: what you are looking at right now.
fn title_strip(picks: Picks) -> View {
    let scale = SCALES[picks.scale];
    let where_in_sweep = format!(
        "{}x{}  [{} of {}]",
        scale.cols,
        scale.rows,
        picks.scale + 1,
        SCALES.len()
    );
    let axes = format!(
        "{}   ·   {}   ·   {}",
        MODES[picks.mode].1, SAMPLINGS[picks.sampling].1, WEIGHTS[picks.weight].1
    );
    Element::new()
        // shrink(0.0) on both one-row strips: at 6x6 the panels below
        // asked for more than the window had, and the layout took the
        // rows from HERE — the title and the key legend vanished while
        // every panel still looked right. The engine says so out loud
        // (`collapsed to 0 under overflow pressure`); listen to it.
        .style(LayoutStyle::line(1).shrink(0.0))
        .draw(move |canvas, rect| {
            let t = current_theme().tokens;
            canvas.fill(rect, ' ', t.text, t.surface_raised);
            canvas.print(
                Point::new(rect.x + 2, rect.y),
                &where_in_sweep,
                t.accent,
                t.surface_raised,
            );
            canvas.print(
                Point::new(rect.x + 22, rect.y),
                &axes,
                t.text_muted,
                t.surface_raised,
            );
        })
        .build()
}

/// The banner at the current settings, with the layout budget it costs
/// printed on the line BELOW it rather than beside its top row — the old
/// placement put a caption inside the letterforms' own rows.
fn specimen_panel(picks: Picks, height: i32) -> View {
    let t = current_theme().tokens;
    let style = picks.style();
    let scale = style.scale;
    let size = bigtext::measure(BANNER, scale).unwrap_or(Size::new(0, 0));
    let cost = format!("\"{BANNER}\" costs {} cols x {} rows", size.w, size.h);
    Block::new()
        .title("specimen")
        .border(BorderKind::Rounded)
        .fill(t.surface)
        // shrink(0.0): the column would otherwise steal these rows back
        // for the growing panel below, and the caption under the banner
        // was the first thing to vanish when it did.
        .layout(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .h(height)
                .shrink(0.0),
        )
        .child(
            Element::new()
                // Height 100% as well as width: a column with no sized
                // child solves to ZERO rows, and a draw closure on a
                // zero-height element paints nothing while every panel
                // still frames correctly. That is the blank-render shape
                // this module has now produced four times.
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .draw(move |canvas, rect| {
                    let t = current_theme().tokens;
                    canvas.fill(rect, ' ', t.text, t.surface);
                    blit(canvas, Point::new(rect.x, rect.y), BANNER, &style);
                    let y = rect.y + scale.rows;
                    if y < rect.bottom() {
                        canvas.print(Point::new(rect.x, y), &cost, t.text_faint, t.surface);
                    }
                })
                .build(),
        )
        .element(&t)
        .build()
}

/// The HARDEST PAIR in each content class, drawn at the current size.
///
/// A banner reading `AGORA` proves nothing about a size: five capitals
/// are the easiest glyphs the font has. The decision this example
/// exists to support is "will MY text read at this size", and the
/// honest answer lives in the two characters that came out closest
/// together — `E` against `F`, `o` against `0`, `●` against `◆`.
///
/// So this panel draws exactly the pairs the readout measured, at the
/// settings in force. The number says how far apart they are; this says
/// what that number LOOKS like, which is the half a number cannot
/// carry. (Drawing the whole alphabet was the first attempt: at 4x3 it
/// cut twenty-two characters and crowded out the specimen, and every
/// glyph in it except two was a glyph nobody needed to inspect.)
fn pairs_panel(picks: Picks) -> View {
    let t = current_theme().tokens;
    let style = picks.style();
    let pairs: Vec<(String, char, char, u32)> = CLASSES
        .iter()
        .filter_map(|(label, content)| {
            let (a, b, d) = bigtext::closest_pair(&style, *content)?;
            Some((label.to_string(), a, b, d))
        })
        .collect();
    Block::new()
        .title("the hardest pair in each class, at this size")
        .border(BorderKind::Rounded)
        .fill(t.surface)
        .layout(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .grow(1.0),
        )
        .child(
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .draw(move |canvas, rect| {
                    let t = current_theme().tokens;
                    canvas.fill(rect, ' ', t.text, t.surface);
                    let mut y = rect.y;
                    let mut cut = 0usize;
                    // The last row is reserved for the shortfall note, so
                    // a note can never land on top of a glyph — which is
                    // what happened when it was written wherever `y` had
                    // got to.
                    let floor_y = rect.bottom() - 1;
                    for (label, a, b, d) in pairs.iter() {
                        if y + style.scale.rows + 1 > floor_y {
                            cut += 1;
                            continue;
                        }
                        let pair: String = [*a, *b].iter().collect();
                        blit(canvas, Point::new(rect.x, y), &pair, &style);
                        y += style.scale.rows;
                        let verdict = match d {
                            0 => "identical — this size cannot carry it",
                            1..=3 => "close: judge it in YOUR terminal",
                            _ => "clearly apart",
                        };
                        canvas.print(
                            Point::new(rect.x, y),
                            &format!("{label}  {a}/{b}  {d} subpixels — {verdict}"),
                            t.text_faint,
                            t.surface,
                        );
                        y += 1;
                    }
                    if cut > 0 {
                        canvas.print(
                            Point::new(rect.x, floor_y),
                            &format!(
                                "{cut} more class(es) need a taller window at {}x{}",
                                style.scale.cols, style.scale.rows
                            ),
                            t.warn,
                            t.surface,
                        );
                    }
                })
                .build(),
        )
        .element(&t)
        .build()
}

/// The aspect trap, with each shape LABELLED under it. Two unlabelled
/// blobs side by side was the old version, and a reader had no way to
/// tell which one was the point.
fn trap_panel(picks: Picks) -> View {
    let t = current_theme().tokens;
    let style = picks.style();
    let square = GlyphScale::square(2);
    let naive = GlyphScale { cols: 2, rows: 2 };
    // ICONS, not letters: this is the shape an icon has to hold, and the
    // trap is what bites a theme button in a status bar. Two columns on
    // FIXED origins — sizing the second off the first's width put the
    // two captions hard against each other and they read as one word.
    const TRAP: &str = "★◆";
    Block::new()
        .title("the aspect trap — a cell is twice as tall as it is wide")
        .border(BorderKind::Rounded)
        .fill(t.surface)
        .layout(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .h(6)
                .shrink(0.0),
        )
        .child(
            Element::new()
                // Height 100% as well as width: a column with no sized
                // child solves to ZERO rows, and a draw closure on a
                // zero-height element paints nothing while every panel
                // still frames correctly. That is the blank-render shape
                // this module has now produced four times.
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .draw(move |canvas, rect| {
                    let t = current_theme().tokens;
                    canvas.fill(rect, ' ', t.text, t.surface);
                    let right = rect.x + 30;
                    let icon = |s: GlyphScale| BigTextStyle { scale: s, ..style };
                    blit(canvas, Point::new(rect.x, rect.y), TRAP, &icon(square));
                    blit(canvas, Point::new(right, rect.y), TRAP, &icon(naive));
                    let y = rect.y + 2;
                    if y < rect.bottom() {
                        canvas.print(
                            Point::new(rect.x, y),
                            "square(2) — 4x2, square",
                            t.ok,
                            t.surface,
                        );
                        canvas.print(
                            Point::new(right, y),
                            "2x2 — half as wide as tall",
                            t.warn,
                            t.surface,
                        );
                    }
                })
                .build(),
        )
        .element(&t)
        .build()
}

/// The readout that turns the module's claims into numbers you can
/// watch move: the closest pair in each content class, rasterized at the
/// CURRENT settings.
///
/// This is the panel that makes the example worth running rather than
/// reading. `has_margin` is one boolean over a global floor; these are
/// two independent measurements per class — how far apart the closest
/// pair is, and how far the worst character is from its own shape — and
/// they disagree with each other constantly. 2x2 braille icons are 5
/// subpixels apart and a 2x vertical stretch of a disc.
fn measured_panel(picks: Picks) -> View {
    let t = current_theme().tokens;
    let style = picks.style();
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, u32, String, Legibility)> = CLASSES
        .iter()
        .filter_map(|(label, content)| {
            let (a, b, d) = bigtext::closest_pair(&style, *content)?;
            let worst = bigtext::least_faithful(&style, *content)
                .map(|(c, loss)| format!("{c}{loss:.2}"))
                .unwrap_or_default();
            Some((
                label.to_string(),
                format!("{a}/{b}"),
                d,
                worst,
                bigtext::legibility(&style, *content),
            ))
        })
        .collect();
    // The floor DERIVED for arbitrary text in the current symbols,
    // rather than one constant asserted for every class and mode.
    let smallest = bigtext::smallest_clear(MODES[picks.mode].0, Content::Text);
    // The number above cannot see distortion, so the panel says so
    // rather than letting three green verdicts speak for a row of
    // white bars. This is the line the readout was missing when the
    // white bars were reported.
    let scale = SCALES[picks.scale];
    let in_band = scale.within_aspect_band();
    let aspect = if in_band {
        format!("aspect: {}x{} in band", scale.cols, scale.rows)
    } else if scale.cols > scale.rows {
        format!("aspect: {}x{} STRETCHED WIDE", scale.cols, scale.rows)
    } else {
        format!("aspect: {}x{} SQUEEZED TALL", scale.cols, scale.rows)
    };
    Block::new()
        .title("measured now")
        .border(BorderKind::Rounded)
        .fill(t.surface_raised)
        .layout(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                // 18, not 14: the aspect line and its two-line warning
                // are what a caller at 6x2 most needs to read, and they
                // were the lines a short panel dropped first — silently,
                // because every `y >= rect.bottom()` guard here fails
                // closed. A readout that hides its own caveat is the
                // shape of the bug this panel was built to report.
                .h(18)
                .shrink(0.0),
        )
        .child(
            Element::new()
                // Height 100% as well as width: a column with no sized
                // child solves to ZERO rows, and a draw closure on a
                // zero-height element paints nothing while every panel
                // still frames correctly. That is the blank-render shape
                // this module has now produced four times.
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .draw(move |canvas, rect| {
                    let t = current_theme().tokens;
                    canvas.fill(rect, ' ', t.text, t.surface_raised);
                    let mut y = rect.y;
                    canvas.print(
                        Point::new(rect.x, y),
                        "class",
                        t.text_muted,
                        t.surface_raised,
                    );
                    canvas.print(
                        Point::new(rect.x + COL_PAIR, y),
                        "pair",
                        t.text_muted,
                        t.surface_raised,
                    );
                    canvas.print(
                        Point::new(rect.x + COL_APART, y),
                        "apart",
                        t.text_muted,
                        t.surface_raised,
                    );
                    canvas.print(
                        Point::new(rect.x + COL_SHAPE, y),
                        "shape",
                        t.text_muted,
                        t.surface_raised,
                    );
                    y += 1;
                    canvas.print(
                        Point::new(rect.x, y),
                        "0 apart = same picture",
                        t.text_faint,
                        t.surface_raised,
                    );
                    y += 1;
                    canvas.print(
                        Point::new(rect.x, y),
                        "shape over 0.35 = not a glyph",
                        t.text_faint,
                        t.surface_raised,
                    );
                    y += 2;
                    for (label, pair, d, worst, verdict) in rows.iter() {
                        if y >= rect.bottom() {
                            break;
                        }
                        let ink = match verdict {
                            Legibility::Collides => t.error,
                            // The verdict this panel used to be unable
                            // to print: measurably distinct, and not a
                            // glyph any more.
                            Legibility::Distorted => t.error,
                            Legibility::Marginal => t.warn,
                            Legibility::Clear => t.ok,
                        };
                        canvas.print(Point::new(rect.x, y), label, t.text, t.surface_raised);
                        canvas.print(
                            Point::new(rect.x + COL_PAIR, y),
                            pair,
                            t.text_muted,
                            t.surface_raised,
                        );
                        canvas.print(
                            Point::new(rect.x + COL_APART, y),
                            &format!("{d:>3}"),
                            ink,
                            t.surface_raised,
                        );
                        canvas.print(
                            Point::new(rect.x + COL_SHAPE, y),
                            worst,
                            ink,
                            t.surface_raised,
                        );
                        y += 1;
                    }
                    y += 1;
                    if y < rect.bottom() {
                        canvas.print(
                            Point::new(rect.x, y),
                            &aspect,
                            if in_band { t.text_muted } else { t.error },
                            t.surface_raised,
                        );
                        y += 1;
                    }
                    if !in_band && y < rect.bottom() {
                        canvas.print(
                            Point::new(rect.x, y),
                            "the pair column is true",
                            t.error,
                            t.surface_raised,
                        );
                        y += 1;
                        if y < rect.bottom() {
                            canvas.print(
                                Point::new(rect.x, y),
                                "and not the point",
                                t.error,
                                t.surface_raised,
                            );
                            y += 1;
                        }
                    }
                    y += 1;
                    if y < rect.bottom() {
                        canvas.print(
                            Point::new(rect.x, y),
                            &match smallest {
                                Some(s) => format!("cheapest clear text: {}x{}", s.cols, s.rows),
                                None => "no scale clears for text".to_string(),
                            },
                            t.accent,
                            t.surface_raised,
                        );
                        y += 2;
                    }
                    // That floor is per CLASS and per MODE, derived by
                    // bigtext::smallest_clear rather than asserted by a
                    // constant — press `s` and watch it move.
                    for line in [
                        "derived per class and per",
                        "symbols — press s and",
                        "watch it move.",
                    ] {
                        if y >= rect.bottom() {
                            break;
                        }
                        canvas.print(Point::new(rect.x, y), line, t.text_faint, t.surface_raised);
                        y += 1;
                    }
                })
                .build(),
        )
        .element(&t)
        .build()
}

/// Where you are in the size sweep, and what the next keypress gets.
///
/// ↑/↓ used to move a number in a corner and nothing else, so the only
/// way to know what 4x5 looked like was to walk there and remember. The
/// list is cheap and it is the one thing a size explorer actually needs:
/// the whole ladder, with your rung marked.
fn sweep_panel(picks: Picks) -> View {
    let t = current_theme().tokens;
    let here = picks.scale;
    Block::new()
        .title("sizes  ↑/↓")
        .border(BorderKind::Rounded)
        .fill(t.surface)
        .layout(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .grow(1.0),
        )
        .child(
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .draw(move |canvas, rect| {
                    let t = current_theme().tokens;
                    canvas.fill(rect, ' ', t.text, t.surface);
                    // Window the list around the current rung rather
                    // than truncating at the bottom: a 30-row terminal
                    // cannot show sixteen sizes, and a list that quietly
                    // stops at 6x4 says the ladder ends there.
                    let visible = (rect.h.max(1) as usize).min(SCALES.len());
                    let first = here.saturating_sub(visible / 2).min(SCALES.len() - visible);
                    for (row, i) in (first..first + visible).enumerate() {
                        let s = &SCALES[i];
                        let y = rect.y + row as i32;
                        if y >= rect.bottom() {
                            break;
                        }
                        let current = i == here;
                        let (fg, bg) = if current {
                            (t.bg, t.accent)
                        } else {
                            (t.text_muted, t.surface)
                        };
                        let cost = bigtext::measure(BANNER, *s).unwrap_or(Size::new(0, 0));
                        // Out-of-band rungs are marked, not hidden: the
                        // reader is meant to walk onto one and see why.
                        let line = format!(
                            " {}{}x{}{}  {:>2} x {} cells",
                            if current { "▶ " } else { "  " },
                            s.cols,
                            s.rows,
                            if s.within_aspect_band() { " " } else { "!" },
                            cost.w,
                            cost.h
                        );
                        canvas.fill(Rect::new(rect.x, y, rect.w, 1), ' ', fg, bg);
                        canvas.print(Point::new(rect.x, y), &line, fg, bg);
                    }
                })
                .build(),
        )
        .element(&t)
        .build()
}

/// Blit one rendered string at `origin`.
///
/// The ground passed in is the colour being drawn ONTO, not
/// transparency: quadrant and sextant choose their glyph by fitting two
/// colours against each other, and a transparent ground gives the fit
/// nothing to weigh — every cell comes back blank. `render_with` refuses
/// that combination rather than returning a correctly-sized empty grid.
fn blit(canvas: &mut dyn Canvas, origin: Point, text: &str, style: &BigTextStyle) {
    let t = current_theme().tokens;
    let grid = match bigtext::render_with(text, style, t.text, t.surface) {
        Ok(g) => g,
        // The API refuses a character it has no glyph for rather than
        // dropping it. Saying so beats a silently shorter banner.
        Err(e) => {
            canvas.print(origin, &format!("bigtext refused: {e}"), t.error, t.surface);
            return;
        }
    };
    for (pos, chr, fg, bg) in grid.cell_patches(origin) {
        let bg = if bg.is_transparent() { t.surface } else { bg };
        canvas.put(pos, chr, fg, bg);
    }
}

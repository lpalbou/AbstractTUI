//! Seeded random frame generation over `render::Surface` plus the
//! model-vs-surface comparison — the shared harness behind the
//! diff/present property tests (doctrine §1) and any future pipeline
//! property test (compositor layers, animation frames).
//!
//! OWNER: REDTEAM.
//!
//! The rig deliberately depends on render's PUBLIC api only (draw_text,
//! fill_rect, register_link, glyph_str, get): frames are built the way
//! applications build them, so a property failure is always a real
//! application-visible defect, never a private-API artifact.

use crate::base::palette::{SYSTEM_16, XTERM_256};
use crate::base::{Rect, Rgba, Size};
use crate::render::{Attrs, Cell, ColorDepth, PresentCaps, Style, Surface};

use super::fuzzish::Rng;
use super::grid;
use super::vt::VtScreen;

/// Text corpus: ASCII, CJK wide, VS16, ZWJ families, combining marks —
/// every width class the engine claims to handle.
pub const WORDS: &[&str] = &[
    "hysteresis",
    "abstract",
    "compositor",
    "damage",
    "日本語テスト",
    "中文字符",
    "한국어",
    "🎉🧪",
    "\u{2601}\u{fe0f} cloud",
    "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466} fam",
    "e\u{301}toile",
    "mixed 漢字 and text",
    "⚙ gears ⚙",
];

/// One draw operation; a frame is a list of these applied to a blank
/// surface. Ops carry link URIs out of band (ids are surface-local).
#[derive(Clone)]
pub enum Op {
    Text {
        x: i32,
        y: i32,
        word: usize,
        style: Style,
    },
    Fill {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        cell: Cell,
    },
}

pub fn random_color(rng: &mut Rng) -> Rgba {
    match rng.below(5) {
        0 => Rgba::TRANSPARENT, // terminal default
        _ => Rgba::rgb(rng.byte(), rng.byte(), rng.byte()),
    }
}

/// A palette-exact color for the given depth: quantization maps it to
/// itself, so downlevel frames stay byte-predictable through the model.
pub fn palette_color(rng: &mut Rng, depth: ColorDepth) -> Rgba {
    match depth {
        ColorDepth::TrueColor => random_color(rng),
        ColorDepth::Xterm256 => {
            if rng.chance(1, 5) {
                Rgba::TRANSPARENT
            } else {
                XTERM_256[16 + rng.below(240)] // never the themable 0..=15
            }
        }
        ColorDepth::Ansi16 => {
            if rng.chance(1, 5) {
                Rgba::TRANSPARENT
            } else {
                SYSTEM_16[rng.below(16)]
            }
        }
    }
}

pub fn random_attrs(rng: &mut Rng) -> Attrs {
    let all = [
        Attrs::BOLD,
        Attrs::DIM,
        Attrs::ITALIC,
        Attrs::UNDERLINE,
        Attrs::UNDERCURL,
        Attrs::BLINK,
        Attrs::REVERSE,
        Attrs::HIDDEN,
        Attrs::STRIKE,
    ];
    let mut a = Attrs::NONE;
    for bit in all {
        if rng.chance(1, 6) {
            a |= bit;
        }
    }
    a
}

pub fn random_style(
    rng: &mut Rng,
    depth: ColorDepth,
    links: bool,
) -> (Style, Option<&'static str>) {
    let mut style = Style::new()
        .fg(palette_color(rng, depth))
        .bg(palette_color(rng, depth))
        .attrs(random_attrs(rng));
    let uri = if links && rng.chance(1, 6) {
        Some(if rng.chance(1, 2) {
            "https://a.example"
        } else {
            "https://b.example"
        })
    } else {
        None
    };
    if uri.is_some() {
        style = style.attrs(Attrs::UNDERLINE);
    }
    (style, uri)
}

pub fn random_ops(
    rng: &mut Rng,
    size: Size,
    depth: ColorDepth,
    links: bool,
) -> Vec<(Op, Option<&'static str>)> {
    let mut ops = Vec::new();
    for _ in 0..rng.range(3, 14) {
        if rng.chance(1, 4) {
            let (style, _) = random_style(rng, depth, false);
            let cell = Cell::EMPTY
                .with_fg(style.fg.unwrap_or(Rgba::TRANSPARENT))
                .with_bg(style.bg.unwrap_or(Rgba::TRANSPARENT));
            ops.push((
                Op::Fill {
                    x: rng.below(size.w as usize) as i32,
                    y: rng.below(size.h as usize) as i32,
                    w: rng.range(1, 30) as i32,
                    h: rng.range(1, 6) as i32,
                    cell,
                },
                None,
            ));
        } else {
            let (style, uri) = random_style(rng, depth, links);
            ops.push((
                Op::Text {
                    // Deliberately allowed to overflow the right edge:
                    // draw_text must clip; wide glyphs at the margin blank.
                    x: rng.below(size.w as usize) as i32,
                    y: rng.below(size.h as usize) as i32,
                    word: rng.below(WORDS.len()),
                    style,
                },
                uri,
            ));
        }
    }
    ops
}

pub fn build_frame(size: Size, ops: &[(Op, Option<&'static str>)]) -> Surface {
    let mut s = Surface::new(size, Cell::EMPTY);
    for (op, uri) in ops {
        match op {
            Op::Text { x, y, word, style } => {
                let mut style = *style;
                if let Some(u) = uri {
                    let id = s.register_link(u);
                    style = style.link(id);
                }
                s.draw_text(*x, *y, WORDS[*word], style);
            }
            Op::Fill { x, y, w, h, cell } => {
                s.fill_rect(Rect::new(*x, *y, *w, *h), *cell);
            }
        }
    }
    s
}

/// Map a surface color to what the model should hold after presentation.
pub fn expect_color(c: Rgba) -> Option<Rgba> {
    if c.is_transparent() {
        None
    } else {
        Some(Rgba::rgb(c.r, c.g, c.b))
    }
}

/// The presenter's attr canonicalization (present.rs), mirrored: with
/// undercurl support, UNDERCURL absorbs UNDERLINE; without, UNDERCURL
/// degrades to UNDERLINE.
pub fn expect_attrs(a: Attrs, caps: &PresentCaps) -> grid::Attrs {
    let a = if a.contains(Attrs::UNDERCURL) {
        if caps.undercurl {
            a.without(Attrs::UNDERLINE)
        } else {
            a.without(Attrs::UNDERCURL).with(Attrs::UNDERLINE)
        }
    } else {
        a
    };
    let mut out = grid::Attrs::default();
    for (theirs, mine) in [
        (Attrs::BOLD, grid::Attrs::BOLD),
        (Attrs::DIM, grid::Attrs::DIM),
        (Attrs::ITALIC, grid::Attrs::ITALIC),
        (Attrs::UNDERLINE, grid::Attrs::UNDERLINE),
        (Attrs::UNDERCURL, grid::Attrs::UNDERCURL),
        (Attrs::BLINK, grid::Attrs::BLINK),
        (Attrs::REVERSE, grid::Attrs::REVERSE),
        (Attrs::HIDDEN, grid::Attrs::HIDDEN),
        (Attrs::STRIKE, grid::Attrs::STRIKE),
    ] {
        if a.contains(theirs) {
            out.set(mine, true);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Scroll-shaped workloads (cycle 4): the frame sequences RENDER's
// scroll-region optimization exists for. Each generator returns
// SUCCESSIVE frames whose content is a shifted copy of the previous one
// plus fresh rows — the property test drives them like any other
// sequence; the byte counts are the optimization's before/after metric.
// ---------------------------------------------------------------------------

/// Full-width, per-item-distinct line content: consecutive items share
/// no column runs, so a one-row scroll really changes every scrolled
/// cell (a weak generator would let common prefixes hide the cost the
/// scroll optimization exists to remove).
fn item_line(item: usize, width: i32) -> String {
    let mut out = format!("{item:05} ");
    let mut h = (item as u64).wrapping_mul(0x9e3779b97f4a7c15) | 1;
    while (out.chars().count() as i32) < width {
        h ^= h >> 13;
        h = h.wrapping_mul(0x2545f4914f6cdd1d);
        let c = b'a' + (h % 26) as u8;
        out.push(c as char);
        if h.is_multiple_of(7) {
            out.push(' ');
        }
    }
    out
}

/// A log view: every frame appends `lines_per_frame` new lines at the
/// bottom and the whole body shifts up (every visible row changes).
pub fn log_append_frame(
    size: Size,
    frame_no: usize,
    lines_per_frame: usize,
    style: Style,
) -> Surface {
    let mut s = Surface::new(size, Cell::EMPTY);
    let total = frame_no * lines_per_frame;
    for row in 0..size.h {
        // The line index visible at this row for this frame.
        let line = total as i64 + row as i64 - size.h as i64 + 1;
        if line >= 0 {
            s.draw_text(0, row, &item_line(line as usize, size.w), style);
        }
    }
    s
}

/// A list scrolled to `offset`: rows show items offset..offset+h, each
/// row full-width distinct.
pub fn list_frame(size: Size, offset: usize, style: Style) -> Surface {
    let mut s = Surface::new(size, Cell::EMPTY);
    for row in 0..size.h {
        s.draw_text(0, row, &item_line(offset + row as usize, size.w), style);
    }
    s
}

/// Fixed header/footer, scrolling middle band: the frame shape where a
/// scroll-region emission wins the most bytes.
pub fn banded_list_frame(size: Size, offset: usize, style: Style) -> Surface {
    let mut s = Surface::new(size, Cell::EMPTY);
    s.draw_text(0, 0, "HEADER — fixed chrome row", style);
    for row in 1..size.h - 1 {
        s.draw_text(0, row, &item_line(offset + row as usize, size.w), style);
    }
    s.draw_text(0, size.h - 1, "FOOTER — fixed chrome row", style);
    s
}

/// THE comparison: every cell of the model screen against the intended
/// surface — content, wide pairing, colors, attrs, link identity (by
/// URI, since ids are namespace-local on both sides).
///
/// # Panics
/// On the first mismatch, with the styled dump in the message.
pub fn assert_screen_matches(screen: &VtScreen, surface: &Surface, caps: &PresentCaps, ctx: &str) {
    let size = surface.size();
    for y in 0..size.h {
        for x in 0..size.w {
            let want = surface.get(x, y).expect("in bounds");
            let got = screen.cell(x, y).expect("model in bounds");
            let here = format!("{ctx} at ({x},{y})");

            if want.is_continuation() {
                assert!(
                    got.is_continuation(),
                    "{here}: surface continuation vs model {:?}\n{}",
                    got.content,
                    screen.to_styled_dump()
                );
            } else {
                let want_text = {
                    let s = surface.glyph_str(want);
                    if s.is_empty() {
                        " "
                    } else {
                        s
                    }
                };
                let got_text = if got.is_continuation() {
                    "\0"
                } else {
                    got.display()
                };
                assert_eq!(
                    got_text,
                    want_text,
                    "{here}: text mismatch\n{}",
                    screen.to_styled_dump()
                );
            }

            // Style: only leaders + narrow cells carry authoritative style
            // in the model (continuations mirror their leader by repair).
            if !want.is_continuation() {
                assert_eq!(got.paint.fg, expect_color(want.fg), "{here}: fg");
                assert_eq!(got.paint.bg, expect_color(want.bg), "{here}: bg");
                assert_eq!(
                    got.paint.attrs,
                    expect_attrs(want.attrs, caps),
                    "{here}: attrs"
                );
                let want_uri = if caps.hyperlinks {
                    surface.link_uri(want.link)
                } else {
                    None
                };
                let got_uri = got
                    .paint
                    .link
                    .and_then(|id| screen.link_target(id))
                    .map(|(_, u)| u);
                assert_eq!(got_uri, want_uri, "{here}: link");
            }
        }
    }
}

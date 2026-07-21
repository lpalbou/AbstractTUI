//! SGR machinery: pen state, color downlevel representations and the
//! deterministic parameter builders, plus the low-level CSI byte helpers.
//! Split from `present.rs` (file-size budget); crate-private — the
//! presenter is the only consumer, and REDTEAM's byte contract is pinned
//! at the presenter surface.

use crate::base::Rgba;

use super::cell::{Attrs, Cell};
use super::present::{ColorDepth, PresentCaps};

/// A color as the terminal will be told it: already downleveled. Pen
/// comparisons happen on THIS, so two truecolor values quantizing to the
/// same palette index emit nothing — and a background change that shifts
/// the pair-preserving nudge of an unchanged foreground re-emits the
/// foreground correctly (see `resolve_pen`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ColorRepr {
    /// Terminal default (SGR 39/49/59) — also the post-`SGR 0` state.
    Default,
    Rgb(u8, u8, u8),
    Idx256(u8),
    Idx16(u8),
}

/// SGR-relevant state: what the terminal's pen currently holds, in
/// emission representation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pen {
    pub(crate) attrs: Attrs,
    pub(crate) fg: ColorRepr,
    pub(crate) bg: ColorRepr,
    pub(crate) ul: ColorRepr,
}

impl Pen {
    pub(crate) const DEFAULT: Pen = Pen {
        attrs: Attrs::NONE,
        fg: ColorRepr::Default,
        bg: ColorRepr::Default,
        ul: ColorRepr::Default,
    };
}

/// Cell -> emission-ready pen: attrs canonicalized against caps, colors
/// downleveled (jointly for fg/bg — DESIGN request 3: independent nearest
/// lookups can collapse a deliberately-subtle theme pair into one palette
/// slot, erasing text; the pair quantizer nudges the foreground to the
/// nearest distinct entry preserving the light/dark ordering).
pub(crate) fn resolve_pen(cell: &Cell, caps: &PresentCaps) -> Pen {
    let attrs = canonical_attrs(cell.attrs, caps);
    let (fg, bg) = match caps.color {
        ColorDepth::TrueColor => (repr_rgb(cell.fg), repr_rgb(cell.bg)),
        ColorDepth::Xterm256 => match (cell.fg.is_transparent(), cell.bg.is_transparent()) {
            (true, true) => (ColorRepr::Default, ColorRepr::Default),
            (false, true) => (
                ColorRepr::Idx256(super::color::nearest_xterm256(cell.fg)),
                ColorRepr::Default,
            ),
            (true, false) => (
                ColorRepr::Default,
                ColorRepr::Idx256(super::color::nearest_xterm256(cell.bg)),
            ),
            (false, false) => {
                let (f, b) = super::color::quantize_pair_256(cell.fg, cell.bg);
                (ColorRepr::Idx256(f), ColorRepr::Idx256(b))
            }
        },
        ColorDepth::Ansi16 => match (cell.fg.is_transparent(), cell.bg.is_transparent()) {
            (true, true) => (ColorRepr::Default, ColorRepr::Default),
            (false, true) => (
                ColorRepr::Idx16(super::color::nearest_ansi16(cell.fg)),
                ColorRepr::Default,
            ),
            (true, false) => (
                ColorRepr::Default,
                ColorRepr::Idx16(super::color::nearest_ansi16(cell.bg)),
            ),
            (false, false) => {
                let (f, b) = super::color::quantize_pair_16(cell.fg, cell.bg);
                (ColorRepr::Idx16(f), ColorRepr::Idx16(b))
            }
        },
    };
    // Underline color: only meaningful when an underline is drawn and the
    // terminal speaks SGR 58. 16-color terminals have no palette form for
    // it — the color drops, the underline stays (labeled downlevel).
    let ul = if cell.ul.is_transparent()
        || !caps.underline_color
        || !attrs.intersects(Attrs::UNDERLINE | Attrs::UNDERCURL)
    {
        ColorRepr::Default
    } else {
        match caps.color {
            ColorDepth::TrueColor => repr_rgb(cell.ul),
            ColorDepth::Xterm256 => ColorRepr::Idx256(super::color::nearest_xterm256(cell.ul)),
            ColorDepth::Ansi16 => ColorRepr::Default,
        }
    };
    Pen { attrs, fg, bg, ul }
}

fn repr_rgb(c: Rgba) -> ColorRepr {
    if c.is_transparent() {
        ColorRepr::Default
    } else {
        ColorRepr::Rgb(c.r, c.g, c.b)
    }
}

/// Folds UNDERCURL according to caps: unsupported terminals get UNDERLINE;
/// supported ones drop a redundant UNDERLINE when both are set (SGR 4:3 is
/// itself an underline style — emitting both would fight).
fn canonical_attrs(attrs: Attrs, caps: &PresentCaps) -> Attrs {
    if attrs.contains(Attrs::UNDERCURL) {
        if caps.undercurl {
            attrs.without(Attrs::UNDERLINE)
        } else {
            attrs.without(Attrs::UNDERCURL).with(Attrs::UNDERLINE)
        }
    } else {
        attrs
    }
}

/// (bit, set-code) in deterministic emission order. UNDERCURL's "4:3" is
/// handled out of band.
const SET_CODES: [(Attrs, &[u8]); 9] = [
    (Attrs::BOLD, b"1"),
    (Attrs::DIM, b"2"),
    (Attrs::ITALIC, b"3"),
    (Attrs::UNDERLINE, b"4"),
    (Attrs::UNDERCURL, b"4:3"),
    (Attrs::BLINK, b"5"),
    (Attrs::REVERSE, b"7"),
    (Attrs::HIDDEN, b"8"),
    (Attrs::STRIKE, b"9"),
];

/// Incremental transition: targeted resets, then re-adds, then additions,
/// then color deltas. SGR 22 clears BOLD+DIM together and 24 clears
/// UNDERLINE+UNDERCURL together, so a survivor sharing a reset code with a
/// removed attribute must be re-added after the reset.
pub(crate) fn build_incremental(cur: &Pen, want: &Pen, buf: &mut Vec<u8>) {
    buf.clear();
    let removed = cur.attrs.without(want.attrs);
    let mut re_add = Attrs::NONE;

    if removed.intersects(Attrs::BOLD | Attrs::DIM) {
        push_param(buf, b"22");
        re_add = re_add.with(want.attrs & (Attrs::BOLD | Attrs::DIM));
    }
    if removed.contains(Attrs::ITALIC) {
        push_param(buf, b"23");
    }
    if removed.intersects(Attrs::UNDERLINE | Attrs::UNDERCURL) {
        push_param(buf, b"24");
        re_add = re_add.with(want.attrs & (Attrs::UNDERLINE | Attrs::UNDERCURL));
    }
    if removed.contains(Attrs::BLINK) {
        push_param(buf, b"25");
    }
    if removed.contains(Attrs::REVERSE) {
        push_param(buf, b"27");
    }
    if removed.contains(Attrs::HIDDEN) {
        push_param(buf, b"28");
    }
    if removed.contains(Attrs::STRIKE) {
        push_param(buf, b"29");
    }

    let to_set = want.attrs.without(cur.attrs).with(re_add);
    for (bit, code) in SET_CODES {
        if to_set.contains(bit) {
            push_param(buf, code);
        }
    }
    if cur.fg != want.fg {
        push_color(buf, ColorSlot::Fg, want.fg);
    }
    if cur.bg != want.bg {
        push_color(buf, ColorSlot::Bg, want.bg);
    }
    if cur.ul != want.ul {
        push_color(buf, ColorSlot::Ul, want.ul);
    }
}

/// Reset-based transition: `0` then everything the target needs. `SGR 0`
/// also resets the underline color (SGR 59 state), so `ColorRepr::Default`
/// needs no explicit param here.
pub(crate) fn build_reset(want: &Pen, buf: &mut Vec<u8>) {
    buf.clear();
    push_param(buf, b"0");
    for (bit, code) in SET_CODES {
        if want.attrs.contains(bit) {
            push_param(buf, code);
        }
    }
    if want.fg != ColorRepr::Default {
        push_color(buf, ColorSlot::Fg, want.fg);
    }
    if want.bg != ColorRepr::Default {
        push_color(buf, ColorSlot::Bg, want.bg);
    }
    if want.ul != ColorRepr::Default {
        push_color(buf, ColorSlot::Ul, want.ul);
    }
}

fn push_param(buf: &mut Vec<u8>, param: &[u8]) {
    if !buf.is_empty() {
        buf.push(b';');
    }
    buf.extend_from_slice(param);
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ColorSlot {
    Fg,
    Bg,
    /// Underline color. Extended forms use ISO-8613 COLON sub-parameters
    /// (`58:2::r:g:b`, `58:5:n`) — the form kitty documents as canonical;
    /// every terminal that advertises underline color parses it, while the
    /// legacy semicolon form is ambiguous with following parameters.
    Ul,
}

/// Emits one color parameter from its resolved representation.
fn push_color(buf: &mut Vec<u8>, slot: ColorSlot, repr: ColorRepr) {
    if !buf.is_empty() {
        buf.push(b';');
    }
    match (slot, repr) {
        (ColorSlot::Fg, ColorRepr::Default) => buf.extend_from_slice(b"39"),
        (ColorSlot::Bg, ColorRepr::Default) => buf.extend_from_slice(b"49"),
        (ColorSlot::Ul, ColorRepr::Default) => buf.extend_from_slice(b"59"),
        (ColorSlot::Fg, ColorRepr::Rgb(r, g, b)) => push_rgb(buf, b"38;2;", b';', r, g, b),
        (ColorSlot::Bg, ColorRepr::Rgb(r, g, b)) => push_rgb(buf, b"48;2;", b';', r, g, b),
        // Colon form with empty colorspace id: 58:2::r:g:b.
        (ColorSlot::Ul, ColorRepr::Rgb(r, g, b)) => push_rgb(buf, b"58:2::", b':', r, g, b),
        (ColorSlot::Fg, ColorRepr::Idx256(n)) => push_idx(buf, b"38;5;", n),
        (ColorSlot::Bg, ColorRepr::Idx256(n)) => push_idx(buf, b"48;5;", n),
        (ColorSlot::Ul, ColorRepr::Idx256(n)) => push_idx(buf, b"58:5:", n),
        (ColorSlot::Fg, ColorRepr::Idx16(n)) => push_u32(
            buf,
            if n < 8 {
                30 + n as u32
            } else {
                90 + n as u32 - 8
            },
        ),
        (ColorSlot::Bg, ColorRepr::Idx16(n)) => push_u32(
            buf,
            if n < 8 {
                40 + n as u32
            } else {
                100 + n as u32 - 8
            },
        ),
        // resolve_pen never produces a 16-palette underline color (SGR 58
        // has no 16-color form); reaching here is a presenter bug.
        (ColorSlot::Ul, ColorRepr::Idx16(_)) => {
            debug_assert!(false, "underline color has no 16-color form");
            buf.extend_from_slice(b"59");
        }
    }
}

fn push_rgb(buf: &mut Vec<u8>, prefix: &[u8], sep: u8, r: u8, g: u8, b: u8) {
    buf.extend_from_slice(prefix);
    push_u32(buf, r as u32);
    buf.push(sep);
    push_u32(buf, g as u32);
    buf.push(sep);
    push_u32(buf, b as u32);
}

fn push_idx(buf: &mut Vec<u8>, prefix: &[u8], n: u8) {
    buf.extend_from_slice(prefix);
    push_u32(buf, n as u32);
}

// -- low-level byte emission ---------------------------------------------

/// `ESC [ <n> <final>`, eliding `n == 1` per ANSI defaults.
pub(crate) fn csi_n(out: &mut Vec<u8>, n: u32, final_byte: u8) {
    out.extend_from_slice(b"\x1b[");
    if n != 1 {
        push_u32(out, n);
    }
    out.push(final_byte);
}

/// `ESC [ <row+1> ; <col+1> H`, with the 3-byte home shorthand.
pub(crate) fn cup(out: &mut Vec<u8>, x: i32, y: i32) {
    if x == 0 && y == 0 {
        out.extend_from_slice(b"\x1b[H");
        return;
    }
    out.extend_from_slice(b"\x1b[");
    push_u32(out, (y + 1) as u32);
    out.push(b';');
    push_u32(out, (x + 1) as u32);
    out.push(b'H');
}

/// Minimal integer formatter — `write!` would work without allocating but
/// drags core::fmt machinery into the hottest byte path.
pub(crate) fn push_u32(out: &mut Vec<u8>, mut n: u32) {
    let mut digits = [0u8; 10];
    let mut i = digits.len();
    loop {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[i..]);
}

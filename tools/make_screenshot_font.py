#!/usr/bin/env python3
"""Regenerate src/render/screenshot_font_data.rs — the 8x16 bitmap font
the PNG screenshot writer draws text with.

    pip install pillow matplotlib      # matplotlib only ships the font
    python3 tools/make_screenshot_font.py

Source font: DejaVu Sans Mono, under the Bitstream Vera Fonts Copyright
(a permissive, MIT-style licence: redistribution and embedding allowed
with the notice, no copyleft). Only the glyph BITMAPS land in the crate,
credited in ACKNOWLEDGEMENTS.md.

Rasterization is 1-bit at 13px into an 8x16 cell, thresholded at 80/255
— the settings that keep thin stems ($, %, i, l) intact without letting
heavy glyphs (@, W, M) bleed into the neighbouring cell. Deterministic:
re-running writes byte-identical output.

Ranges that TILE (box drawing, block elements, braille, sextants) are
deliberately NOT in this table: they are drawn geometrically by
screenshot_png.rs so their strokes meet exactly at every cell boundary,
which no font rasterization can guarantee.
"""
import os
import sys

import matplotlib
from PIL import Image, ImageDraw, ImageFont

CELL_W, CELL_H = 8, 16
SIZE, THRESHOLD, BASELINE = 13, 80, 12

# ASCII plus the symbols a terminal UI actually reaches for. Anything
# outside this table and the geometric ranges draws a labeled
# placeholder — never a wrong glyph, never a silent blank.
SYMBOLS = (
    " §°±·×÷"          # nbsp § ° ± · × ÷
    "‐–—‘’“”"          # dashes, curly quotes
    "†•…‰‹›«»"    # † • … ‰ ‹ › « »
    "←↑→↓↔↵⇒"          # ← ↑ → ↓ ↔ ↵ ⇒
    "−≈≠≤≥∞"                # − ≈ ≠ ≤ ≥ ∞
    "─│"                                        # ─ │ (see note below)
    "■□▪▫▲▶▸▾"    # ■ □ ▪ ▫ ▲ ▶ ▸ ▾
    "▼◀◆◇○●◦"          # ▼ ◀ ◆ ◇ ○ ● ◦
    "★☆☑☐✓✗⚠"          # ★ ☆ ☑ ☐ ✓ ✗ ⚠
    "☾✧➤⌘⏎␈⦿"          # ☾ ✧ ➤ ⌘ ⏎ ␈ ⦿
    "◎▤▷⌸◈"                # the graph/mermaid badge sigils
)
# U+2500/U+2502 appear above only so a caller inspecting the table finds
# them; screenshot_png.rs draws the whole box-drawing range itself and
# never consults these entries.


def is_notdef(font, ch, notdef_rows):
    """True when the font has no glyph and substituted its .notdef box.

    Embedding .notdef would be a lie: the table would claim a glyph and
    draw a hollow box, which is EXACTLY what the renderer's own
    honest placeholder looks like — but without the honesty. Skip it
    and let the placeholder path own the case.
    """
    return render(font, ch) == notdef_rows


def render(font, ch):
    cell = Image.new("L", (CELL_W, CELL_H), 0)
    ImageDraw.Draw(cell).text((0, BASELINE), ch, font=font, fill=255, anchor="ls")
    rows = []
    px = cell.load()
    for y in range(CELL_H):
        bits = 0
        for x in range(CELL_W):
            if px[x, y] > THRESHOLD:
                bits |= 1 << (CELL_W - 1 - x)   # bit 7 = leftmost pixel
        rows.append(bits)
    return rows


def main(out="src/render/screenshot_font_data.rs"):
    ttf = os.path.join(
        os.path.dirname(matplotlib.__file__), "mpl-data", "fonts", "ttf", "DejaVuSansMono.ttf"
    )
    font = ImageFont.truetype(ttf, SIZE)
    chars = [chr(c) for c in range(0x20, 0x7F)] + sorted(set(SYMBOLS))
    chars = sorted(set(chars), key=ord)
    # U+E000 is private-use: no font ships a glyph for it, so whatever
    # comes back IS this font's .notdef.
    notdef_rows = render(font, "\ue000")
    missing = [c for c in chars if c != " " and is_notdef(font, c, notdef_rows)]
    if missing:
        print("skipped (font has no glyph): " + " ".join(
            f"U+{ord(c):04X} {c}" for c in missing))
    chars = [c for c in chars if c not in missing]

    lines = [
        "//! 8x16 bitmap glyphs for the PNG screenshot writer (generated).",
        "//!",
        "//! Regenerate with `python3 tools/make_screenshot_font.py`; the",
        "//! script documents the font, the licence, and the rasterization",
        "//! settings. Each entry is a codepoint and 16 rows of 8 pixels,",
        "//! bit 7 = leftmost. The table is sorted by codepoint so lookup is",
        "//! a binary search with no allocation and no map.",
        "//!",
        "//! Glyphs come from DejaVu Sans Mono (Bitstream Vera Fonts",
        "//! Copyright — permissive, attribution only; see",
        "//! ACKNOWLEDGEMENTS.md). Box drawing, block elements, braille and",
        "//! sextants are absent ON PURPOSE: `screenshot_png` draws those",
        "//! geometrically so they tile exactly.",
        "",
        "/// Glyph cell size in pixels.",
        "pub const GLYPH_W: u32 = 8;",
        "pub const GLYPH_H: u32 = 16;",
        "",
        f"/// Codepoint + 16 pixel rows, sorted by codepoint ({len(chars)} glyphs).",
        f"pub static GLYPHS: [(char, [u8; 16]); {len(chars)}] = [",
    ]
    for ch in chars:
        rows = render(font, ch)
        body = ", ".join(f"0x{r:02x}" for r in rows)
        esc = {"\\": "\\\\", "'": "\\'"}.get(ch)
        name = f"'{esc}'" if esc else (f"'\\u{{{ord(ch):04x}}}'" if ord(ch) > 126 or ord(ch) < 33 else f"'{ch}'")
        lines.append(f"    ({name}, [{body}]),")
    lines.append("];")
    lines.append("")
    with open(out, "w") as f:
        f.write("\n".join(lines))
    print(f"wrote {out}: {len(chars)} glyphs")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "src/render/screenshot_font_data.rs")

# 0490 — PNG screenshots: the faithful capture

Status: completed 2026-08-21
Owner: engine (render/screenshot_png)
Effort: M

## The field ask

Operator, 2026-08-21, of the SVG captures in `docs/captures/`: "I don't
believe they work correctly (especially font size and layouts). Couldn't
we create snapshots in png directly?"

They were right, and the cause is structural: `to_svg` names a font
family and hopes the viewer resolves one whose advance matches the
grid. When it does not, `lengthAdjust="spacingAndGlyphs"` stretches the
glyph outlines and box-drawing strokes stop meeting. An SVG of a
terminal cannot be faithful, because the writer does not control the
font.

## Shipped

`Screenshot::to_png()` / `to_png_with(PngOpts)` / `to_bitmap()` /
`write_png()`. Every pixel is decided by the engine: text from an
embedded 8x16 bitmap (DejaVu Sans Mono, Bitstream Vera licence —
permissive, attribution only, credited in ACKNOWLEDGEMENTS.md), and the
ranges that must TILE — box drawing, block elements, braille, sextants
— drawn GEOMETRICALLY, which is strictly better than a font could do
because strokes then meet exactly at every cell boundary. Deterministic
integer math; the output round-trips through the engine's own decoder.

The measurement that sized the work: across all 38 captures, 95.3% of
characters are ASCII, 4.6% are those geometric ranges, and there is no
CJK or emoji at all.

## Evidence

Nine tests, and writing them caught two defects before release: the
left-eighths block range was off by one (`▌` rendered 5/8 wide instead
of half, which would have skewed every mosaic image and bar), and
braille sat at 25% coverage — reading as punctuation rather than ink.
The font generator refuses a `.notdef` substitution rather than
embedding it, which then revealed that two badge sigils the mermaid
crate had chosen are absent from a complete monospace font and would
have drawn as tofu in a terminal.

## Limits kept

Coverage is ASCII plus the geometric ranges plus ~60 symbols; anything
else (CJK, emoji) draws a labeled placeholder, never a wrong glyph.
Rounded corners draw square: at an 8x16 cell the arc and the corner
occupy the same pixels. `to_svg` stays, because GitHub renders it
inline in a README — the docs now say which artifact is which.

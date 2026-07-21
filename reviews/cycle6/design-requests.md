# DESIGN cycle-6 requests + notes

## Shipped this cycle (DESIGN)

- **Syntax token group** (`syntax_keyword/string/number/type/func/punct/
  comment` on `TokenSet`, doc §1.4a): derived per theme from the audited
  accent/semantic family, walked toward `text` until clearing floors
  against `surface_raised` (primary 4.5:1, comment 3:1) CAPPED at each
  theme's own text ceiling there (`registry::syntax_floor` — everforest-
  light's 4.25:1 is why the cap exists; zero new audit exceptions across
  all 24 themes). A chart-style de-clone pass keeps green-on-green
  families (everforest) tellable-apart. `shadow_ground` token added too:
  `shadow` pre-composited over `bg` at theme build — widgets never run
  `over()` (RT1-9b).
- **Rich-text widget trio**: `RichTextView` (span walk shared by all
  three; patch-style fg inheritance), `MarkdownView` (heading steps,
  quote bars, fence highlighting on the code ground, inline-code chips,
  `outline()` for TOC + `rows()` sharing the typeset fold with the
  renderer), `CodeView` (+ `code_token_color` — THE TokenKind->token
  mapping; gutter numbers `text_faint`, `Ident` deliberately body ink
  until a lexer distinguishes types/functions).
- **`Block::shadow(shadow_ground)`**: one-cell offset elevation strip,
  right+bottom, chrome shrinks to fit — one-time paint, no per-frame
  cost.
- **`examples/components.rs`** (the shareable-component reference) and
  **`examples/grid.rs`** (track-grid reflow).

## To RENDER

1. `RichLine::from_highlighted` + patch-style spans composed perfectly
   with theme inheritance — nice seam. One note: your `MdStyles::base`
   is stamped by `parse_inline` onto every plain span, so a base WITH an
   explicit fg defeats downstream block recoloring (my blockquote dim);
   I now pass `Style::EMPTY` base and inherit at draw. If that pattern
   is the intended one, a doc line on `MdStyles::base` would save the
   next consumer the same detour.
2. Your `rich`/`md`/`highlight`/`compositor` unit tests were red at my
   close (mid-wave, e.g. `strings_and_comments_edge_cases` expects
   `toks[1]` to be the string but the lexer emits `(` punct between) —
   my downstream widgets are green against the shipped behavior; ping if
   the lexer contract changes shape so I can re-pin.
3. ParticleField had not landed by my close — the splash afterglow
   still rides GFX3D's per-pixel trail merge. Happy to art-direct the
   particle pass when it exists; identity constants stay LOCKED
   meanwhile (two pty eyeballs across cycles say the timing reads well;
   changing constants now would red GFX3D's drift pin mid-wave for
   marginal gain).

## To REACT

4. `checkbox`/`radio` conformance: CLEAN — selection pair on focus,
   `accent` marks, `text_faint` disabled, no color arithmetic; both in
   the lint. Grid tracks + spans worked first try in `examples/grid.rs`
   (`Fr` largest-remainder tiling is visibly exact on resize). When
   `Callback<T>`/KeymapHelp land, `components.rs` is the adoption site
   (its `impl FnMut` props are the pre-Callback spelling) and the '?'
   help content is ready to go rich via `MarkdownView`.

## To REDTEAM

5. New surfaces: the syntax derivation (capped floors — try themes whose
   text barely clears 4.5 on raised), `MarkdownView::rows` vs rendered
   height (the fold is shared by construction; attack wrap edge cases),
   `Block::shadow` geometry at degenerate sizes, `cell_of` byte-vs-cell
   lesson (my own tests had `str::find` BYTE offsets indexing CELL
   grids — multi-byte gutter glyphs shifted everything right; the
   helper is in both test files, worth a rig-level assert-helper).

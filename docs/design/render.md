# render — cells, surfaces, compositor, diff, presenter

Owner: RENDER. Scope: `src/render/**`, `src/text/**`, `src/anim/**`.
Cycle-2 amendments are marked [C2]; the damage contract
(`docs/design/01-damage-contract.md`) is binding and this doc defers to it.

## 1. Research notes (cycle 1)

### 1.1 ratatui: buffer diff

Source studied: `ratatui-core/src/buffer/{buffer,diff}.rs` (v0.30 line) and the
v0.30.1 release notes.

- `Buffer` is a flat `Vec<Cell>`; `Cell` holds its grapheme in a
  `CompactString` (24-byte small-string, heap spill past that), plus
  fg/bg/underline color and a modifier bitfield.
- Diffing is a zero-allocation iterator (`BufferDiff`) yielding
  `(x, y, &Cell)` for each changed cell. Two counters drive wide-char
  correctness:
  - `to_skip = width(next_symbol) - 1`: after emitting a wide leader, the
    covered trailing cells are not emitted.
  - `invalidated = max(width(next), width(prev)) - 1`: when the *previous*
    frame had a wide glyph here, the following cells must be treated as
    changed even if they compare equal, because the terminal's ground truth
    was overwritten by the wide glyph.
- Special case: emoji presentation sequences containing VS16 (U+FE0F) get
  their trailing cells *explicitly* re-emitted, because several terminals
  fail to clear the second column of such glyphs on their own.
- There is no damage input: diff always scans the full buffer. There is no
  run structure either; the backend re-positions per contiguous cell and
  diffs style lazily per emitted cell.

### 1.2 notcurses: cells, planes, egcpools

Sources studied: `notcurses_cell(3)`, `notcurses_plane(3)`,
`notcurses_render(3)`, `egcpool.h`, `render.c`, `internal.h`, FOSDEM 2021
slides.

- `nccell` is exactly 16 bytes: `gcluster` (4B), `gcluster_backstop` (1B,
  always 0), `width` (1B, cached column width), `stylemask` (2B),
  `channels` (8B: two 32-bit channels, each 24-bit RGB + alpha/default/
  palette-index flag bits).
- EGC storage: if the cluster fits in 4 UTF-8 bytes (true for every single
  codepoint) it is inlined into `gcluster`; longer clusters spill into a
  per-plane `egcpool` (ring buffer, ≤16 MiB) and the cell stores a 24-bit
  offset, disambiguated by a 0x01 first byte (control bytes never enter
  cells, so 0x01 cannot appear in valid inline UTF-8 content).
- Width is *cached in the cell* so render/raster never re-measure.
- Two-phase pipeline: *render* flattens a z-ordered pile of planes into one
  cell matrix (top-down; the topmost glyph wins, backgrounds keep
  accumulating until opaque); *rasterize* compares that matrix against
  `lastframe` (a damage map) and emits minimal escapes while carrying a
  persistent `rasterstate`: physical cursor position, last fg/bg RGB, current
  attribute set, and "elidable" flags so redundant SGR/motion is skipped
  across frames.
- Why it is fast: POD 16-byte cells (2 per cache line pair), spill is rare,
  damage map bounds emission to changed cells, and the persistent raster
  state elides most escapes.

### 1.3 OpenTUI (Zig core): frame diff

Sources studied: `packages/core/src/zig/renderer.zig`, DeepWiki rendering
pipeline notes, commit 56ff129 ("pack color metadata into RGBA high bytes").

- Double buffer of struct-of-arrays cells (`chars: u32` codepoint/cluster
  id, fg/bg RGBA, attribute+link word), rendered into a pre-allocated
  multi-MB output buffer.
- Diff walks every cell of the frame; unchanged cells (`char`, attrs, fg,
  bg equal) emit zero bytes. Changed cells are grouped into *runs* with
  identical styling: position + SGR once per run, then raw glyph bytes.
- Colors were originally compared with a float epsilon; the project later
  moved to packed integer RGBA compared *exactly*, and to deterministic
  integer source-over blending — float color math in the diff path proved
  to be a correctness and determinism liability. This validates our choice
  of `u8`-channel `Rgba` end to end.
- Wide glyphs occupy leader + placeholder continuation cells, like ours.

### 1.4 What we do differently

| Concern | ratatui | notcurses | OpenTUI | AbstractTUI |
| --- | --- | --- | --- | --- |
| Diff scope | full frame | full frame vs lastframe damage map | full frame | **damage rects from the compositor**; only damaged rows are scanned |
| Cell size | 24B + heap spill (CompactString) | 16B, 4B inline EGC | SoA, u32 cluster id | 24B AoS, **10B inline EGC** + per-surface pool |
| Emission | per-cell, lazy style | persistent rstate, elision | per-run SGR | per-run SGR state machine + cursor motion economy + byte-cost compare (incremental vs reset) |
| Color model | u8 RGB | 24-bit channels + flags | f32 → packed u16 RGBA | u8 RGBA everywhere; alpha=0 means "terminal default" at the presenter |
| Compositing | none (single buffer) | top-down glyph lock-in | none (widgets draw into one buffer) | bottom-up source-over with per-layer opacity, wide-pair repair pass |

The bet: fine-grained reactivity gives us *small damage*, so a diff that
only scans damaged rows beats full-frame scans at scale, and the compositor
gives animations (move/fade a layer) without re-rendering content.

## 2. Decisions

### 2.1 Cell = 28 bytes [C2], `Glyph` inlines ≤ 10 bytes

```text
Cell { glyph: Glyph(12B), fg: Rgba(4B), bg: Rgba(4B), ul: Rgba(4B),
       attrs: Attrs(2B), link: u16 }
Glyph { data: [u8; 10], len_or_tag: u8, width: u8 }
```

- `size_of::<Cell>() == 28` (align 2, no padding; within the sanctioned
  24–32 budget). Statically asserted. [C2] `ul` is the underline color
  (SGR 58/59; alpha 0 = default). It is a VALUE, not an interned id —
  colors need no table, and a third id space would recreate the ambient
  ownership problem RT1-4 kills.
- 10 inline bytes cover: every single codepoint (UTF-8 ≤ 4B), combining
  stacks up to ~3 marks, CJK+VS16 (6B), emoji+VS16 (7B), keycaps (7B),
  flags (8B), skin-tone pairs (8B). Only ZWJ sequences (families, ≥ 11B)
  and exotic mark stacks spill to the per-`Surface` pool.
- Rejected: notcurses' 4-byte inline — it spills for *every* emoji
  presentation sequence and flag, which are common in modern UI content.
  Rejected: `CompactString`-style 24-byte glyph — the cell would grow to
  ~40 bytes and halve cache density for a case the pool covers.
- The `width` byte caches the cluster's display width (notcurses lesson:
  never re-measure in diff/present).
- Continuation cells (`TAG_CONTINUATION`) occupy the trailing column of a
  wide glyph. Invariant: a continuation always sits immediately right of
  its leader and **mirrors the leader's fg/bg/attrs/link**. Every write API
  repairs violated pairs by blanking the orphan half (space, style kept).
- Pool ids are *surface-local*. Cross-surface equality (diff) and adoption
  (blit/flatten) resolve through the owning surface's pool; inline-vs-pooled
  can never be content-equal because spill only happens past 10 bytes.
- [C2, RT1-4] Pool ownership is a type-level fact: every `Surface` owns
  exactly one pool + one link table (the flattened frame is a Surface, so
  it owns its own — flatten adopts foreign ids on write). The raw
  resolution primitives (`Glyph::new`, `Glyph::as_str`, `content_eq`) are
  crate-private; the public path is `Surface::glyph_str(&self, &Cell)`,
  which can only resolve through the owning pool. `Surface::debug_validate()`
  (`#[cfg(any(test, debug_assertions))]`) is the structural oracle for
  REDTEAM property tests: pairs intact, styles mirrored, pool/link ids in
  range.
- [C2, RT1-14] Growth caps with labeled degradation: the glyph pool caps
  at `GLYPH_POOL_CAP` (4096) unique long clusters — past it, new clusters
  render as visible U+FFFD and `GlyphPool::dropped()` counts them; the
  link table caps at 65535 URIs — past it, `register_link` returns 0
  (plain text, never a wrapped id: mislinking is worse than dropping) and
  `Surface::links_dropped()` counts. Compaction hook: `Surface::clear`
  resets the pool (the only moment ids are provably unreferenced); link
  ids are handed out to callers, so the link table deliberately survives
  `clear` — its bound is the cap.

### 2.2 Compositor

- `Layer { surface, origin, z, opacity, visible, blend: Normal }` with
  damage recorded in *frame coordinates at damage time* (so moving a layer
  damages both old and new bounds without coordinate skew).
- Flatten walks layers bottom-up per damaged cell:
  - effective color = channel alpha × layer opacity;
  - opaque bg replaces the accumulator wholesale (fast path);
  - translucent bg composites via `Rgba::over`; a glyph-bearing upper cell
    takes the glyph slot and blends its fg over the new bg; a glyph-less
    upper cell *veils* the lower glyph (`fg = upper_bg over fg`).
  - `Glyph::EMPTY` is see-through (compositing transparency); a space is a
    real glyph that erases. Widgets paint background with EMPTY + bg color.
- Damage rects are expanded ±1 column (wide pairs sliced by rect edges),
  coalesced, and capped: above `max_rects` they collapse to their union;
  a union covering most of the frame degrades to full-frame damage.
- After blending, a repair pass re-establishes pair invariants and
  re-mirrors continuation style from the leader — the diff and any VT model
  can then assume continuation style equals leader style.

### 2.2b Layer effects: blend modes, color transforms, cell shaders [C3]

Per-cell contribution pipeline, order contractual:

```text
src cell -> shader.shade(x, y, shader_t, cell) -> ColorTransform
         -> opacity (alpha scale) -> blend into accumulator
```

- **`Blend::Additive`**: saturating premultiplied add
  (`acc + src*src.a/255` per channel, alpha saturating too). Black adds
  nothing; light accumulates toward white — afterglow, particles,
  scanline highlights. A cell holds ONE glyph, so an additive glyph takes
  the glyph slot and its ink renders as light over the lit ground; a
  glyph-less additive cell brightens both the ground and any kept glyph
  ink. Adding onto an alpha-0 ("terminal default") accumulator treats the
  unknown color as black — the only honest choice without the theme
  ground.
- **`ColorTransform`** (one per layer): `None | Dim(f) | Tint(color, s) |
  Grayscale(s)` — the fixed-function grade for fades/disables. Alpha is
  never edited (coverage is opacity's job); alpha-0 slots pass through
  (no RGB to grade). Composing several grades belongs in a shader, not in
  stacked fixed-function state.
- **`CellShader`** (the post-processing hook):
  `shade(&self, x, y, t, cell) -> Cell`, applied to the layer's damaged
  cells at flatten. `x/y` are frame coordinates; `t` is per-layer
  (`Layer::set_shader_t`), driven by the app clock — replaying the same
  layer state composes the same frame, no hidden compositor time.
  Determinism contract: pure in `(x, y, t, cell)` + construction params;
  no interior mutability; built-ins use a triangle wave and an integer
  position hash, never libm transcendentals (bit-stable across
  platforms, REDTEAM goldens them). **Damage/billing rule**: shaders run
  only where damage exists; an animated shader is an ANIMATION —
  advancing `shader_t` damages the layer bounds and the driver requests
  frames for it; a static shader costs nothing after its first paint.
  Wide pairs: a shader that split-styles a leader/continuation pair is
  overruled by the repair pass — the pair renders with the leader's
  final style.
- **`CellShader::changed_region(t0, t1, bounds) -> Option<Rect>` [C7,
  RT6-3]**: the active-region hint. The returned rect is where output may
  differ between the two clocks; OUTSIDE it the shader guarantees
  bit-stability (`shade(x,y,t0,c) == shade(x,y,t1,c)` for every cell).
  `Layer::set_shader_t` damages only this region (intersected with the
  layer bounds) instead of the whole layer: a `Vignette` (ignores `t`) or
  a settled reveal ticks for FREE; a `Sweep`/`ScanlineFade` band damages
  its slab; `Shimmer`/`Rainbow` honestly return `None` (the wave moves
  every ink cell — only exact phase equality is bit-safe, since integral
  phase deltas round differently per cell). Default `None` = old
  whole-bounds behavior, so third-party shaders are unaffected. The
  contract is deliberately *stability*, not identity-with-source (a
  reveal's hidden cells are transparent at both clocks — stable is what
  damage needs). Implementations must be conservative — too-small rects
  are the one dishonesty the property test hunts
  (`changed_region_hints_are_honest_for_every_builtin`: exhaustive grid
  sweep outside the hint over mid-flight/settled/rewind/wrap clock
  pairs).
- Built-ins (`anim::shaders`, since effects are time-driven and anim sits
  above render): `Shimmer` (diagonal luminance ripple over ink, ground
  untouched), `ScanlineFade` (top-down reveal; ground fades in, glyph
  pops at half coverage), `HueDrift` (channel-rotation pulse for focus
  accents), `Dissolve` (seeded per-cell hash threshold; monotone per
  cell). All identity-off: defaults (no shader, `ColorTransform::None`,
  `Blend::Normal`, opacity 1) are byte-identical to the ungraded
  compositor — test-pinned.

### 2.2e Paint helpers: gradients + drop shadow [C6]

- `render::paint::fill_gradient(surface, rect, &GradientSpec)`: linear
  (visual angle, cell aspect 1:2 corrected) or radial (unit-rect center,
  t=1 at the farthest corner), N sorted color stops, sRGB lerp between
  stops (the engine's ONE lerp), background-only (glyphs/attrs/links
  preserved; both draw orders compose). Banding at cell resolution is
  broken by a 4x4 ordered Bayer dither on the interpolation fraction,
  bounded to one lerp step (`without_dither()` for exact-color asserts).
  One-time paint: damages the rect once, never a per-frame cost.
- `render::paint::drop_shadow(panel, offset, feather, color, z) -> Layer`
  — the elevation recipe: a translucent ramp layer (Chebyshev falloff
  over `feather` cells) placed BELOW the panel's layer; the compositor's
  Normal blend + theme ground make it honest over default-bg cells.
  Shadows are layer CONTENT (move/fade via handles), not a post-effect.

### 2.2f Damage visualizer [C6]

`Compositor::set_debug_damage(true)` outlines every composed frame's
damage rects in a magenta bg tint — the minimal-damage claim made
visible for devs and REDTEAM. The outline is real frame content (diffed
and emitted like anything else), so it is a DIAGNOSTIC mode: bytes and
pixels change while on; never enable in byte-golden tests. Stale
outlines persist until damage covers them (they are pixels); wide pairs
stay consistent (leader-tint mirrors, repair pass runs).

### 2.3 Diff

`FrameDiff::compute(prev, next, damage) -> &[Run]`,
`Run { y, x, len }` = contiguous same-row span of changed cells.

- Damage rects → per-row column intervals (merged, ±1 col expansion), so
  overlapping rects never produce overlapping runs.
- Wide handling: a changed wide leader forces its continuation into the run
  (`force` counter — ratatui's `to_skip`, inverted); a changed continuation
  with an (impossibly) equal leader starts the run one cell left so the
  leader is re-emitted. Because continuation cells are first-class values,
  the prev-wide→next-narrow case falls out naturally: the old continuation
  compares unequal to any narrow cell, so the stale half is always re-emitted
  (ratatui's `invalidated` counter, without the counter).
- Size mismatch between prev/next → full-frame damage (resize path).
- Scratch (`runs`, row intervals) is owned by `FrameDiff` and reused;
  steady-state compute allocates nothing once buffers are warm.

### 2.4 Presenter

Deterministic byte emission; REDTEAM snapshots exact bytes.

- **Cursor motion economy**: same row → nothing / `CR` (to col 0) /
  `CUF n` / `CUB n`; row change or unknown → `CUP`. LF/VPA tricks are
  rejected: they depend on ONLCR/origin-mode state we do not own.
- **Virtual cursor** advances by the emitted glyph's cached width.
  Continuation cells inside a run emit nothing (the leader's advance covers
  them).
- **Last-column strategy (the wrap hazard)**: we *do* write the last column
  (including bottom-right) and rely on deferred autowrap (xterm and every
  modern VT descendant: the wrap is pending until the *next* glyph is
  printed). After any last-column write the virtual cursor is invalidated,
  so the next emission always begins with an absolute `CUP`, which clears
  the pending-wrap state — no glyph is ever printed while a wrap is
  pending, so the screen can never scroll. Wide glyphs never start in the
  last column (surface invariant degrades them to a blank at write time),
  so the pending-wrap analysis only involves width-1 emissions.
- **SGR state machine**: persistent across frames. For each style change it
  builds both candidate encodings — incremental (attr removals via
  22/23/24/25/27/28/29 with bold/dim (22) and underline/undercurl (24)
  shared-reset re-adds, then additions 1,2,3,4,4:3,5,7,8,9, then fg, bg) and
  full reset (`0` + everything) — and emits the shorter (tie → incremental).
  Deterministic parameter order = declaration order of the attr bits.
  Undercurl is SGR `4:3`; when UNDERLINE and UNDERCURL are both set only
  `4:3` is emitted.
- **Color**: fg/bg with alpha 0 map to SGR 39/49 ("terminal default") —
  which is also exactly the state after `SGR 0`, so the reset state is
  representable. Truecolor uses `38;2;r;g;b`/`48;2;r;g;b`. [C2] The pen
  stores colors as *resolved emission representations* (default / RGB /
  palette index): comparisons happen post-downlevel, so two truecolor
  values on one palette slot emit a single SGR, and a background change
  that moves a pair-preserving nudge correctly re-emits an "unchanged"
  foreground. Downlevel:
  - **Palette data** [C2, RT1-7]: `base::palette::{SYSTEM_16, XTERM_256,
    CUBE_LEVELS}` — the ONE table shared with the testing VT model. This
    module owns only policy; a drift test pins the midpoint thresholds to
    the base levels.
  - **xterm-256**: cube levels `[0,95,135,175,215,255]` (midpoint
    thresholds 48/115/155/195/235), index `16 + 36r + 6g + b`; gray ramp
    `232..=255` with value `8 + 10(i-232)`, candidate index from luma
    `(2126·R + 7152·G + 722·B) / 10000`; pick the smaller squared RGB
    distance, tie → cube.
  - **16-color**: nearest `SYSTEM_16` entry by squared distance (tie →
    lower index), emitted as 30–37/90–97 and 40–47/100–107.
  - **Pair contrast preservation** [C2, DESIGN req 3]: when both fg and bg
    are concrete, they quantize JOINTLY (`quantize_pair_256/16`): if the
    per-color nearest indices collide while the originals differed (dark
    theme faint-text pairs), the fg re-picks the nearest *distinct*
    palette entry whose integer luma keeps the original light/dark
    ordering against the bg (falling back to nearest-distinct when the bg
    sits at the palette extreme). Genuinely identical colors still
    collapse.
- **Underline color** [C2, DESIGN req 2]: SGR 58 in ISO-8613 colon form —
  `58:2::r:g:b` truecolor, `58:5:n` on 256-color — and `59` for the reset
  to default; emitted only when `caps.underline_color` AND the cell has an
  underline attr (colorless otherwise = zero bytes). Downlevel is labeled:
  without the cap (or on 16-color, which has no SGR 58 palette form) the
  color drops and the plain underline stays. `SGR 0` also resets 59-state,
  so the reset encoding needs no extra param.
- **Risky-cluster cursor discipline** [C2, RT1-7]: after emitting any
  cluster containing VS16, ZWJ, or an East-Asian-Ambiguous character, the
  presenter forgets its virtual cursor; the next emission (same run
  included) re-anchors with absolute CUP. Terminals genuinely disagree on
  these widths (xterm renders ZWJ families at component width; ambiguous
  chars go wide under CJK configs) — re-anchoring confines the damage to
  the risky cluster itself. Deliberate carve-out: U+2500–U+25FF (box
  drawing, blocks, geometric shapes) are ambiguous by UAX #11 but NOT
  flagged — they are the fabric of TUI chrome (per-border-cell CUPs would
  dominate output exactly where it is densest), they render from native
  monospace fonts rather than the emoji fallback that widens symbols in
  practice, and an ambiguous-wide terminal breaks cell layout globally in
  a way cursor re-anchoring cannot repair anyway.
- **Hyperlinks**: OSC 8 with `id=<n>` param, ST-terminated
  (`ESC ] 8 ; id=N ; uri ESC \` … `ESC ] 8 ; ; ESC \`). Link ids are
  surface-local; the presenter tracks the open link by URI string.
- **DEC 2026**: `CSI ? 2026 h` / `l` wrap the whole frame when the caps say
  the terminal supports it. Zero runs → zero bytes (no empty brackets).
- **Frame trailer**: close any open hyperlink, `SGR 0`, park the cursor at
  the bottom-left cell (via normal motion economy; bottom-*right* would arm
  the wrap hazard). The trailer means no SGR/link state ever crosses a
  frame boundary — cross-frame presenter state is just the virtual cursor
  and "pen is at defaults"; `invalidate()` forgets both after external
  writes (gfx protocols, suspend).

### 2.4b External bytes + the flush contract [C2]

- **Presenter custody (damage contract §6)**: pixel-protocol payloads
  (kitty APC, iTerm2 OSC 1337, sixel DCS) reach the terminal ONLY through
  `Presenter::external_write(out, bytes, at)`: close any open hyperlink,
  `SGR 0` (payloads must not be interpreted under text attributes),
  absolute CUP to `at` (never relative — mid-composition the virtual
  cursor may already be stale), payload verbatim, then full `invalidate()`
  (protocol payloads move the real cursor in protocol-specific ways).
  GFX3D never writes to the terminal directly; REDTEAM asserts byte
  custody with `CaptureTerm`.
- **One flush per frame (RT1-16a)**: the presenter EMITS (appends bytes to
  the caller's per-frame buffer — `emit` and any `external_write` brackets
  compose into the same buffer, in phase-P order); the App loop WRITES the
  buffer and calls `Terminal::flush` exactly once at frame end. The
  presenter never flushes, never owns the terminal handle. Zero runs and
  zero external writes ⇒ zero bytes ⇒ the App skips write+flush entirely
  (idle frames stay free).

### 2.4d API freeze pass [C8]

- **Drawing vocabulary, one canonical path**: `Surface::draw_text` is THE
  rich-draw call for direct-surface users; `ui::Canvas::put/print` and
  `ui::StyledCanvas::print_styled/fill_styled` are the WIDGET vocabulary
  (draw closures over `&mut dyn`); on a `Surface` every one of them
  funnels into `draw_text`. Documented on the `Surface` type.
- **`StyledCanvas` resolution**: the render-side duplicate trait
  (`render::bridge::StyledCanvas`, same name/drifted signature/zero
  consumers) is DELETED; `ui::StyledCanvas` is the one styled trait and
  `Surface` now implements it DIRECTLY (full fidelity, grapheme-correct)
  — `&mut Surface` slots into widget paint closures without the
  `SurfaceCanvas` wrapper. Breaking: `render::StyledCanvas` no longer
  exists (nothing consumed it).
- **Style terse builders**: `.bold() .dim() .italic() .underline()
  .strike() .reverse()` — the six high-traffic attributes get one-word
  spellings (`Style::new().fg(ink).bold()`); rare attrs stay behind
  `.attrs(...)`. Equivalence test-pinned.
- **Snapshot helpers**: `render::snapshot(&Surface) -> String` (bordered
  char grid) and `render::snapshot_styles` (grid + per-row style-run
  annotations: fg/bg/attrs/ul/link) — the draw-debugging tools; never on
  frame paths, never in byte goldens.
- **Prunes**: `text::is_risky_cluster` → `pub(crate)` (presenter cursor
  defense, not a width oracle — `width`/`cluster_width` are the public
  truth). Kept deliberately despite zero consumers today:
  `wrap_with`/`WrapOpts` (hanging indent is docs-worthy), paint
  gradients/drop_shadow (DESIGN's C6 ask — flagged for the demo cycle
  to claim or they get pruned), caret helpers
  `next_boundary`/`prev_boundary` (the input-widget pair to `segments`),
  pool/link cap constants (documented degradation contract).
- **Rustdoc**: missing-docs count in render/text/anim: 207 → 0; doc-link
  warnings in my modules: 0. Compiling doctests on the six entry points
  (Surface, Style, Compositor+Layer, the full pipeline in `render`
  module docs, markdown→cells in `md`, Tween/Transition/Timeline in
  `anim` module docs).

### 2.4c Bridges (ui Canvas, gfx mosaic) [C2]

- `Surface` implements `ui::Canvas` (REACT req 2) in `render/bridge.rs`:
  `put`/`print` route through `draw_text` (grapheme-correct widths, pair
  invariants, control stripping — `print` returns real columns), `fill`
  through `fill_rect`. Bridge writes are ATOMIC (attrs cleared, no link,
  default underline color; trait-contract exception: bg alpha 0 keeps the
  underlying background) — inheriting a stale BOLD/link under fresh
  content would be a correctness surprise. Rich styling stays native
  `Surface` API. If widgets need attrs/links through the trait, that is a
  follow-up trait on ui's side (request filed), never a weakening of the
  cell model.
- `Surface::blit_mosaic(patches, origin)` consumes gfx mosaic output as
  plain `(Point, char, Rgba, Rgba)` tuples — `gfx::mosaic::CellPatch`'s
  fields in order, one `map` away; render deliberately does not import
  gfx (siblings). Fully-transparent patches write the see-through
  `Cell::EMPTY` (the gfx contract: "image empty here" clears stale
  content); visible patches replace the cell wholesale. Damage is one
  bounding rect (`set_quiet` + a single `add_damage`), not per-cell.

### 2.5 text

- `cluster_width`: 0 for control clusters; 1 if the cluster carries VS15
  (text presentation); 2 if it carries VS16 or ZWJ (emoji presentation and
  joined sequences render double-wide on terminals that support them);
  otherwise `unicode_width::UnicodeWidthStr` capped at 2. One policy shared
  by `text`, `Surface::draw_text` and the diff — width disagreements between
  measurement and rendering are the classic corruption source.
- `wrap`: word wrap at whitespace (break consumes the whitespace), long
  words broken at grapheme boundaries; `\n` respected; width clamps to ≥ 1.
- `truncate_ellipsis`: grapheme-accurate, `…` (width 1), wide glyphs at the
  cut point drop cleanly.
- [C2] `measure(&str, avail: Size) -> Size` (REACT req 4): wrapping-aware
  layout measurement — wrap into `avail.w` (≤ 0 = unconstrained), width =
  widest wrapped line (may honestly exceed `avail.w` only when one cluster
  is wider than the window), height = line count, never clamped to
  `avail.h` (the solver decides visibility; empty text is 1 line).
  Newline handling: `\n` and `\r\n` split logical lines (one cluster each,
  width 0); other control clusters are stripped by wrap/draw and measure 0.
- [C2] `is_risky_cluster` — the RT1-7 classifier (see §2.4).
- [C3/C4] Cursor math for input fields (RT3-2): `segments(&str)` yields
  `(cluster, byte offset, width)` exhaustively (controls kept at width 0
  so every reachable byte boundary is representable);
  `next_boundary/prev_boundary(s, byte_idx)` step carets by whole
  clusters, snapping mid-cluster inputs outward — backspace deletes
  exactly `prev_boundary(s, i)..i`.

### 2.8 Rich text, markdown-lite, syntax tint [C6]

- **Model** (`render::rich`): `Span { text, style: Style, link:
  Option<String> }` / `RichLine` (coalesces same-ink pushes) /
  `RichText`. Links carry URLs (surface-independent); ids resolve at
  draw via `register_link`. Lives in render, not text: the span currency
  is `Style` and text must stay render-import-free (render → text is the
  one arrow). All measurement routes through `crate::text` (one width
  policy).
- **Wrap** preserves spans across boundaries (a word straddling a bold
  edge keeps per-cluster styles), same contract as `text::wrap`
  (test-pinned parity: whitespace consumed at breaks, long words split
  at clusters, empty lines survive). **Draw** into a rect: per-line
  H-alignment, height clipping, ellipsis truncation for overwide lines
  (ellipsis wears the style at the cut); patch semantics keep panel
  grounds.
- **Markdown-lite** (`render::md`): supported EXACTLY — inline `**bold**`
  `*italic*` `` `code` `` `[text](url)` + backslash escapes
  (`\* \` \[ \] \( \) \\ \#`); blocks `#`..`######`, `-`/`*`/`+` and
  `N.` lists (2-space indent depth), `>` quotes (nesting folds to one
  level), ``` fences (verbatim, EOF-closes), `---`/`***` rules,
  blank-line paragraph separation with soft-join. NOT supported (literal
  text, never an error): nested emphasis, `_`/`__`, setext headings,
  tables, HTML, images, reference links, task lists. Styles are
  patches via `MdStyles` (defaults attribute-only; themes override);
  inline merges ONTO block style. [C7] `MdStyles::with_ink(code_fg,
  code_bg, link_fg)` is the canonical theme mapping (render sits below
  `theme`, so tokens arrive as plain `Rgba` — the markdown widget
  resolves its `TokenSet` and calls this); `base` is documented fg-less
  BY CONTRACT: the inline parser stamps `base` on every plain span, so
  an explicit fg there defeats block recoloring (DESIGN's blockquote-dim
  detour, now a doc'd rule + test). [C7] Wrap emission appends borrowed
  cluster text into the tail span (`push_run`) instead of minting a
  `Span`/`String` per cluster — wrap is one pass over clusters with O(1)
  amortized emission (merge checks the LAST span only, never scans), so
  cost is linear in clusters regardless of input span count; test-pinned
  (`wrap_scales_linearly_in_span_count_structurally`) and measured 3.18
  ms for the 800-para doc on a quiet box (budget 20 ms; filed at 15.9 —
  part churn, part the same loaded-box measurement story as RT6-3).
- **Syntax tint** (`text::highlight`): `trait Highlighter { spans(line)
  -> Vec<(Range<usize>, TokenKind)> }`, kinds =
  keyword/string/number/comment/ident/punct. Built-in `CLikeLexer`
  (rust()/c() presets, `Default` = rust): line-at-a-time, C-family
  surface syntax, HONESTLY approximate (no cross-line state — multi-line
  strings/comments mis-tint from line 2). Bridge:
  `RichLine::from_highlighted(line, lexer, base, map)` with the theme
  mapping as a closure — render stays theme-agnostic.

### 2.6c Timeline seek [C7]

`Timeline::seek(t) -> Seek` binds one clock position for scrubbing (the
boot player's test rig, effects demos): `Seek::progress(track)` /
`is_finished()` sample the board at the bound instant without threading
`t` through every call. Pure — a `Seek` is a Copy view, never a
playhead; the timeline stays stateless (the C3 decision stands).
`seek_reversed(t)` gives reverse playback as a clock mirror (board
position `duration − t`, clamped at zero): one pass runs backward with
each track's own easing evaluated at the mirrored clock — deliberately
NOT easing reversal (rebuilding tracks with flipped curves would change
curve identity, and no consumer asked for it).

### 2.6b anim additions [C3]

- `Transition<T: Lerp>`: retargetable ease toward a moving goal —
  `set_target` samples the LIVE in-flight value as the new origin (no
  jumps), redundant retargets are no-ops (a restarted ease has a visible
  velocity hiccup), `tick(now)` settles exactly on the target. Engine-only
  (no reactive imports); REACT binds it to signals next cycle via
  tick-then-`request_frame`-while-unsettled.
- `Timeline`: a storyboard of eased tracks on one clock — `track` /
  `track_after` (sequence) / `stagger`, `LoopMode::{Once, Loop, PingPong}`
  folding the whole board. Tracks yield eased PROGRESS (0..=1), not
  values: storyboards mix types, so values stay with the consumer
  (`from.lerp(to, progress)`). Pure over `t` (frame drops repeat/jump `t`
  per the splash-source seam); evaluation allocates nothing.

### 2.6 anim (skeleton, cycle-2 adjustments)

`Clock` (monotonic, virtual mode for tests), `Easing` (linear, cubic
in/out/in-out, `CubicBezier(x1,y1,x2,y2)` solved by Newton + bisection like
CSS timing functions), `Tween<T: Lerp>` for `f32/i32/Point/Rgba`.

- [C6] `Easing::{Bounce, Elastic(period), Spring(bounciness)}` — feel
  curves from exact polynomials + a Bhaskara-I polynomial sine (|err| ≤
  ~0.0017, no libm — bit-stable; documented as FEEL approximations, not
  physics). Spring(0) cannot overshoot by construction (raised-cosine
  envelope ∈ [0,1]); overshoot scales with bounciness (~21% at 1).
- [C6] `anim::particles::ParticleField` — seeded, deterministic garnish
  (bursts, gravity/drag, explicit-Euler `step(dt)`, life-faded cell
  plotting; aspect-corrected directions via the same polynomial sine).
  Same seed + same steps = same pixels (goldenable).
- [C6] `text::wrap_with(s, w, WrapOpts { hanging_indent, max_lines })` —
  log-pane/chat knobs: continuations indent (single-pass per-row
  budgets, no re-wrap approximation), caps end with a visible `…`.
- [C2] `FrameRequester` moved to `base` (damage contract §7); `anim`
  re-exports it. The local duplicate is deleted.
- [C2] `Easing::bezier(x1, y1, x2, y2)` — `const` named constructor so
  DESIGN's identity curves live as constants. The evaluator clamps TIME
  only: y control points outside [0,1] produce intermediate outputs beyond
  the range (overshoot/settle, anticipation dips). `Lerp` impls follow:
  f32/i32/Point extrapolate; `Rgba` saturates per channel (documented —
  color channels must not wrap).

### 2.2c App-facing layer handles [C4; RETIRED C5]

Cycle 4 shipped `render::LayerStack` (generational ids) as the frozen
handle store; REACT's `app::overlays` landed the same cycle on the SLICE
flatten path with its own monotonic-u64 registry — which carries the same
stale-handle safety (u64 ids never reuse, lookups return None after
removal, Weak-backed handles no-op after app death) plus the reveal
damage rule (`damage_root_under`). Two compositor entry points for one
job is maintenance drag with no consumer on the stack side, so
`LayerStack`/`flatten_stack` were retired in cycle 5 (decision + rationale
in reviews/cycle5/render-requests.md). The compositor keeps exactly one
flatten; the app's overlay store owns handle identity.

### 2.7 Scroll-region optimization — SHIPPED default-OFF [C4]; measurement [C3]

Workload: 200x60 log viewer, every line shifts up, new content enters at
the bottom (truecolor caps, warmed presenter, measured through the real
diff+present pipeline, 2026-07-20):

| Case | cell-diff bytes | scroll-path bytes (ctl + repaint) | ratio |
| --- | --- | --- | --- |
| 1-row scroll | 5,374 (60 runs) | 279 (20 + 259) | **19.3x** |
| 5-row scroll | 5,421 (60 runs) | 1,199 (20 + 1,179) | **4.5x** |

Scroll-path control cost is `DECSTBM(region) + CUP + SU n + DECSTBM(reset)`
≈ 20 bytes; the repaint of scrolled-in rows dominates it. The win is real
and grows with region height; it caps at the full-frame byte budget
(which DEC 2026 already makes tear-free), so this is a bandwidth
optimization (ssh links), not a correctness or latency one.

**Status [C4]: SHIPPED behind `PresenterOpts { scroll_optimization }`,
default OFF.** The cycle-3 verdict's three blockers, resolved or
contained:

1. **Detection soundness is now referee-independent**: whatever shift
   `FrameDiff::compute_scrolled` chooses, the residual runs are computed
   against the virtually-shifted prev (moved rows read their source row;
   entering rows read as BCE-erased blanks), so a wrong candidate costs
   bytes, never pixels. The decomposition property (shift + runs
   reconstruct `next` cell-wise) is pinned over seeded random
   scroll+mutation frames in `render/scroll_tests.rs`.
2. **Guards**: full-width damage union only (DECSTBM scrolls full rows;
   DECSLRM not assumed), band ≥ 8 rows, ≥ 4 rows must become diff-clean
   (the byte-win floor), candidates from row-fingerprint anchors
   (FNV-1a prune, exact row compare verify — collisions cost time only).
3. **BCE**: the presenter's scroll prelude emits `SGR 0` BEFORE
   DECSTBM/SU/SD, so vacated rows erase to the default ground; entering
   cells equal to `Cell::EMPTY` legitimately need no repaint. Prelude
   bytes: `SGR 0`, `CSI top;bottom r`, `CSI n S|T`, `CSI r`; DECSTBM and
   its reset home the cursor (absolute addressing — origin mode never
   enabled), so the virtual cursor re-syncs at (0,0). Snapshot-pinned.

**[C5] Default flipped ON, referee-verified.** REDTEAM's VtScreen gained
DECSTBM + region-scoped SU/SD (+ IL/DL); the byte-level property (emitted
bytes applied to the model reproduce `next` exactly) holds with the
optimization engaged across randomized scroll+mutation sequences and
REDTEAM's published workloads (in-module: `vtscreen_replays_scrolled_
frames_exactly`, `redteam_workload_bytes_with_optimization_on`;
integration: their adv_scroll suite). The pairing is now TYPE-LEVEL:
`compute_scrolled` returns a `ScrolledRuns` token (private fields) that
only `Presenter::emit_scrolled` can consume — shift-relative runs cannot
reach the plain emitter, so the wrong-pairing hazard is unrepresentable
rather than documented. `PresenterOpts::default()` enables the
optimization; detection's byte-win guard (full-width band ≥ 8 rows, ≥ 4
rows made diff-clean, else plain runs) means enabling can only reduce
bytes.

### 2.2d Theme ground [C5]

`Compositor::set_ground(Option<Rgba>)`: the theme's background stands in
for "terminal default" (alpha-0) accumulator cells AT BLEND TIME —
additive light adds onto the theme ground instead of black, translucent
veils blend against it instead of passing through translucent. Cells
nothing blends against keep alpha 0 and still present as SGR 49; the
ground never leaks into untouched content. Default `None` is
byte-identical to the pre-ground compositor (test-pinned). The app owns
wiring the theme signal and the repaint on switch (damage_all — contract
§5).

### 2.10 The render pipeline for app authors [C8 — the docs-cycle page; C9 handoff filed]

One frame, four moves. You own the buffers; the engine owns the rules.
(The COMPILING version of this walkthrough is the `render` module doc
example — CI-checked; the guide should lift that one. Doc-ready prose
for every guide chapter: reviews/cycle9/render-docs-handoff.md.)

```text
1. DRAW    widgets/you write into layer Surfaces   (surfaces record damage)
2. FLATTEN Compositor::flatten(&mut frame, &mut layers) -> &[Rect]
3. DIFF    FrameDiff::compute(&prev, &frame, damage)    -> &[Run]
4. PRESENT Presenter::emit(runs, &frame, &caps, &mut out); flush ONCE;
           prev.blit(&frame, ..)                    (remember what's on screen)
```

**Damage is automatic.** Every `Surface` write records a surface-local
rect; layer mutations (`set_origin`/`set_opacity`/...) record frame
rects for old∪new geometry. You never call `add_damage` for ordinary
drawing — only after mutating cells through some out-of-band path.
Damage over-approximates honestly: the diff re-checks equality, so a
stale rect costs microseconds, never wrong pixels. An idle frame
(nothing drawn, no layer mutated, no shader clock advanced) flattens to
an empty damage list, diffs to zero runs, emits zero bytes and
allocates nothing — idle apps burn nothing, test-pinned.

**Layers vs the root.** One root layer is the whole story for most
apps: widgets draw, the pipeline runs. Reach for MORE layers only for
content that moves/fades/appears INDEPENDENTLY of what is under it —
toasts, modals, a splash overlay, particle glow. A layer buys
independent geometry (move without repainting the world underneath —
the compositor repaints only old∪new bounds), opacity/blend/grade
(`set_opacity`, `Blend::Additive`, `ColorTransform`), and a per-layer
[`CellShader`]. It costs one Surface of memory and its compose share
per damaged cell. Do NOT use layers as a widget system — that is what
the ui tree is for; a dozen layers is a smell, three is a dashboard
with a toast and a modal.

**Shader billing (the rule that keeps effects honest).** A shader runs
only where damage exists. A STATIC shader (installed, clock never
advanced) is paid once at install and never again. An ANIMATED shader
is an animation: advance `Layer::set_shader_t(t)` each frame it should
move — that damages what the shader declares changeable
(`CellShader::changed_region`, default: the whole layer) and your
frame scheduler requests the next frame, exactly like a tween. Settled
reveals and `t`-independent shaders (Vignette) tick for free. Never
advance the clock of an effect nobody can see.

**Bytes reach the terminal once per frame.** `Presenter::emit` appends
to your buffer; you flush the buffer in ONE write. Anything that
bypasses the presenter (image protocols, bells) goes through
`Presenter::external_write` so cursor/pen custody survives. If any
foreign code touched the terminal without telling you:
`presenter.invalidate()` and the next frame re-syncs from absolute
state.

**Debugging a frame.** `render::snapshot(&surface)` prints the char
grid; `render::snapshot_styles` adds per-run style annotations (the
"why isn't this bold" tool). `Compositor::set_debug_damage(true)`
outlines every repaint region in magenta on screen — the minimal-damage
claim made visible. Both are diagnostics: bytes/pixels change, never
use them in golden tests.

### 2.9 Performance envelope [C7, RT6-3]

Phase-split profile, 200x60 + Shimmer full re-shade every frame,
release, isolated (`render::profile` in-module harness, medians over 31
frames): **flatten (compose+shade) 137 µs, diff 53 µs, present 210 µs —
total ~400 µs**, ~7.5x inside the 3 ms budget. REDTEAM's own harness
shape (which also times its per-frame bookkeeping `prev.blit`) medians
430 µs isolated on a quiet box — matching their CYCLE-4 filing (421 µs)
within noise: **the pipeline never regressed**. The cycle-6 3.57 ms
reading was measurement environment, layered: ambient box load (other
teams' builds) lifts the isolated median to ~1.16 ms, and in-binary
parallel test threads lift it past 3.5 ms (`cargo test` co-schedules the
wall-clock perf tests with parser-soup/VT-model iterations). Evidence:
their reported best 1.16 ms equals the loaded-box isolated median
exactly; a full parallel run here reproduced 3.56 ms median / 1.16 ms
best on the same binary that measures 430 µs alone. Perf binaries must
run `--test-threads=1` — filed with REDTEAM, along with the same-class
repro for the gltf alloc-counter test (fails parallel, 8/8 serial).

Envelope for effect authors: one full 200x60 re-shade (12k cells) costs
~137 µs in the compose path ⇒ ~88 shaded kcells/ms; `changed_region`
(§2.2b) is the lever that keeps real workloads from paying it when the
effect is settled or banded.

SGR economy measurement [C7 ask 2]: a 64x20 dashboard frame of repeated
two-color fg toggles emits 7,515 bytes — 321 SGRs, 6,098 bytes, 19.0
avg, exactly one fg-only `38;2;r;g;b` per toggle (opening transition
aside; test-pinned, `dashboard_fg_toggles_emit_only_the_irreducible_sgr`).
A 1-entry last-pen cache was CONSIDERED AND DECLINED: SGR has no
pen-restore instrument, so a truecolor toggle's 38;2 payload is the
floor — there is nothing shorter for a cache to emit. The remaining
lever for byte-sensitive links is 256-color caps (5-byte `38;5;n`), which
downlevel already provides.

`text` segmentation cost [C7 ask 5]: 1.9 µs per `segments()` walk of a
mixed line, 6.3 µs per wrapped `measure()` (release, measured by
`text::tests::profile_segments_and_measure_per_keystroke_cost`). Caret
math runs once per keystroke — a grapheme LRU (eviction, edit
invalidation, ownership) would cost more than it saves. Declined until a
profile shows segmentation hot.

## 3. Known gaps / deferred

- [C2 resolved: caps + drop counters landed, RT1-14] Pool/link dedup is
  still a linear scan — bounded by the caps now; REDTEAM's churn bench
  decides whether generation-based compaction is warranted.
- `Blend::Normal` only; no per-layer effect hooks yet (DESIGN's additive
  blend for afterglow is a cycle-3 candidate).
- VS16 trailing-cell explicit re-emit (ratatui's workaround for buggy
  terminals) is *not* replicated: our continuation model already re-emits
  trailing cells whenever content changes, and [C2] the risky-cluster CUP
  discipline now bounds the residue class; if REDTEAM's terminal matrix
  still shows artifacts, add the targeted re-emit.
- Presenter assumes deferred autowrap. If KERNEL's ConPTY verification
  (RT1-5a) finds a terminal without it, the fallback is a caps bit to skip
  the last column entirely (documented, not implemented).
- [C2] `blit_mosaic` damages the bounding rect of the patch set — precise
  per-row damage for sparse patch sets is a possible refinement if the 3D
  viewport bench shows over-scanning.
- [C2] Underline-color emission uses colon sub-parameter forms; if
  REDTEAM's terminal matrix finds a supported terminal that only parses
  the legacy semicolon form, that becomes a caps nuance (KERNEL detects,
  presenter switches form).

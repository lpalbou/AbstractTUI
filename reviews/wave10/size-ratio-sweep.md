# Wave 10 — size/ratio adversarial sweep (SIZE)

Operator mandate (laurent, 2026-07-24 17:53): "the engine must be
flexible in rendering layouts in terminals of different sizes and
ratios." Relayed with field evidence from the gateway-console seat
(their apps run on this engine): a fixed 1-line title bar VANISHED
once real data loaded — flex crushed it to zero AND the zero-area
child's draw closure still ran, overpainting the sibling row. Both
halves proven console-side; console-side findings 1020/1030 live in
the gateway-console seat's own tracker (this report cross-references
them, it does not restate them).

Tests: `tests/wave_size_sweep.rs` + `tests/wave_size_sweep_parts/`
(21 tests) + 1 unit pin in `src/ui/draw.rs`. Size matrix:
{80x24, 100x24, 60x16, 200x20 wide-short, 60x50 tall-narrow,
40x12 brutal}. All scenes drive HEAVY fixtures (300-400 content rows)
through the real `Driver`/`CaptureTerm`/`VtScreen` pipeline — the
console lesson: light fixtures never see this class; the crush has a
data-volume threshold.

## 1. The fusion-class fix (engine defect, FIXED)

Root cause, verified in-engine: `UiTree::draw_node` culled on
`!rect.intersects(clip) && !rect.is_empty()`. An empty rect never
intersects anything, so zero-area nodes FELL THROUGH the cull and
their draw closures ran with degenerate rects. A hand-rolled closure
that clips on one axis only (truncate to `rect.w`, paint at `rect.y`)
painted a full row onto whichever sibling owned that y once flex
crushed the bar to zero height. `Paint::Text` had an is-empty guard;
`Paint::Draw` did not.

Failing-first: `fusion_zero_crushed_bar_must_not_paint_the_sibling_row`
(tree-level, BufferCanvas) and
`fusion_console_shape_header_page_footer_at_100x16` (their exact
scenario shape — header line(1) + grow page + footer at 100x16 with
200 content rows, real pipeline) reproduced the fusion byte-exact
before the fix: row 0 read
`data-row-000 lorem ipsum dolor sit amet====…` — page content fused
with the crushed header's fill. Post-fix both flip to clean absence.

The fix (src/ui/draw.rs): a node whose rect `is_empty()` skips its OWN
paint (`Paint::Draw` and `Paint::Text`), with two verified edges:

- **`probe_when_culled` exemption preserved** — a measurement probe
  must READ the collapse: a measured-mode Scroll whose content shrinks
  to zero rows has an EMPTY wrapper rect, and the extent probe riding
  its draw is the only thing that can publish `(w, 0)` — starving it
  freezes the extent and the held-offset repair never fires (the
  first-app/0281 void state through a new door). Pinned twice:
  `scroll_extent_probe_still_reads_a_zero_area_collapse` (public API,
  passes pre- AND post-fix — the regression trap) and
  `ui::draw::tests::empty_rect_skips_draw_but_probes_still_read`
  (crate-internal, direct).
- **Children of an empty parent still walk and can paint** — the
  honest answer under this solver is that they CAN be non-empty:
  absolute children size independently against the parent's content
  box, and flow children with an explicit min on the parent's MAIN
  axis hold their extent through the freeze loop (cross-axis extents
  cap at parent content, so a zero-height Row parent's flow children
  are always empty — also pinned). Test:
  `empty_parent_children_with_own_extent_still_paint`.
- **The zero-collapse notice still fires** — it rides the solver
  (`layout/solve.rs` → `note_zero_collapse`), not the skipped draw;
  asserted in the console-shape test and across the whole matrix.
- **Damage across the empty↔non-empty threshold** — the incident's
  lifecycle (light content → data loads, bar crushes → data clears,
  bar returns) repaints correctly in both directions, cell-for-cell
  equal to fresh-paint oracles
  (`crush_transitions_repaint_cleanly_both_directions`).

## 2. Second engine defect found and fixed: Modal never re-clamped

`Modal::open` clamped its panel inside the viewport at OPEN (correct,
pinned) but kept the at-open bounds forever. Failing-first
(`oversized_modal_reclamps_on_resize`): a 78x27 modal opened centered
on a 200x20 terminal sits at x=61; shrinking to 40x12 left the layer
entirely off a 40-column screen — an INVISIBLE focus-trapped panel
owning every key: the app reads as locked. Fixed in
`src/app/popups.rs`: a `modal-reclamp` effect on the modal's scope
rides `use_viewport` (the Drawer contract, extended) — re-solving the
SAME size request per resize (one `modal_bounds` helper shared with
open, so the two can never disagree), resizing the layer surface +
tree viewport, re-centering, and damaging the vacated footprint (the
drawer F1 rule). Bare rigs without a published viewport keep at-open
bounds (empty-viewport guard). All pre-existing modal/overlay/drawer
suites stay green.

## 3. Engine guarantees vs app-side recipes

(Verbatim for the gateway-console seat.)

| # | ENGINE GUARANTEES (any size, any content volume) | Pinned by |
|---|---|---|
| G1 | A child crushed to zero area paints NOTHING — its draw closure does not run; collapse is clean absence, never a smear onto a sibling row. | `fusion_*` tests; `ui::draw` unit pin |
| G2 | An empty parent's children with their own extent (absolute; main-axis min) still paint; the skip is per-node, never per-subtree. | `empty_parent_children_with_own_extent_still_paint` |
| G3 | Every zero-collapse of a declared fixed size is NAMED (debug builds) into the startup-notices lane, once per situation. | `unpinned_chrome_collapses_cleanly_and_is_named` (all 6 sizes) |
| G4 | Rects crossing the empty↔non-empty threshold (data load/clear, resize) repaint cell-for-cell equal to a fresh paint. | `crush_transitions_*`; `resize_live::*` |
| G5 | `shrink(0.0)` chrome holds at EVERY size; if pinned chrome exceeds the viewport it clips honestly in document order — first rows win, nothing fuses, no panic. | `pinned_chrome_survives_every_matrix_size`; `pinned_chrome_taller_than_viewport_clips_honestly` |
| G6 | `Scroll` (default `basis(0)`) exerts no content pressure: an unpinned shell around a scrolled page keeps its chrome at every size. | `scroll_absorbed_page_keeps_unpinned_chrome_at_every_size` |
| G7 | PageHost tab strips window under overflow: `‹`/`›` zones reserved, sticky anchor, active tab always windowed in, single oversized titles ellipsize — usable at 60 and 40 columns (goldens). | `pagehost::*` + 5 goldens |
| G8 | `Modal` clamps inside the viewport at open AND re-clamps on every resize (re-centering, recovering its requested size when room returns); fixed chrome rows inside survive via the 0240 floor; the oversized middle stays scrollable. | `modal_drawer::oversized_modal_*` |
| G9 | `Drawer` extents clamp at open and re-solve on resize: `Percent` rounds against the axis (0.42 of 60 → 25; of 40 → 17), `Cells(30)` on 25 columns → 25, un-clamping when room returns. | `drawer_extents_clamp_at_small_widths` |
| G10 | Live resize across breakpoints with chrome + PageHost + drawer + modal ALL open (single steps and coalesced bursts, both directions) lands cell-for-cell equal to a fresh paint; the composed-frame screenshot equals the bytes the terminal saw. | `resize_live::*` (garbage-prefilled referees) |
| G11 | Wide glyphs never tear: at truncation cuts (Table cells, tab titles, Disclosure headers) a straddling cluster drops whole (spare column blank, ellipsis policy), and every settled frame keeps leader/continuation pairing — walked cell-by-cell at glyph-splitting widths and across 40→19→40 one-column resize ladders. | `unicode_narrow::*` + 2 goldens |

| # | APP-SIDE RECIPES (what the engine deliberately does NOT decide) | Why it stays app-side |
|---|---|---|
| R1 | Chrome you want INCOMPRESSIBLE gets `shrink(0.0)` (or an explicit `min`) — bare `Cells(n)` is a starting size, not a promise, outside `Modal` (which floors declared sizes for you, 0240). | Zero-sized children are legitimate flexbox (spacers, collapsed panels); the engine cannot know which fixed rows are sacred. |
| R2 | Put oversized middles in a `Scroll` — its default `grow(1) basis(0)` absorbs any content volume with no pressure on siblings. This is what the zero-collapse notice text itself recommends. | Content policy: truncate vs scroll is a product decision. |
| R3 | Render `use_startup_notices` somewhere visible (a status line `dyn_view`). The engine names every zero-collapse into that lane — the console seat found nobody reads it. One api.md line now says so explicitly. | The engine must not steal screen rows for its own diagnostics. |
| R4 | Draw closures should still guard `rect.is_empty()` defensively and clip on BOTH axes — the engine now makes the zero-area case unreachable, but a partially-crushed rect (h=1 of a 3-row widget) still reaches the closure with less than it asked for. | The closure owns its rect's interior. |
| R5 | `Toast` keeps its at-open placement (top-right of the open-time viewport) — transient by design, clipped by the compositor if the terminal shrinks mid-flight. Re-show after a resize if placement matters. | 3-second chips repositioning mid-flight buys motion complexity for no operator value; deliberate non-fix this wave. |

## 4. Per-axis findings

- **(a) chrome survival** — engine-correct post-fusion-fix, all three
  arms across the matrix (pinned / unpinned-clean-absence+notice /
  Scroll-absorbed) plus the taller-than-viewport floor. No further
  engine defect; recipes R1-R3 documented.
- **(b) PageHost 6+ tabs at 60/40** — already-correct: windowing,
  sticky anchor, indicator clicks, chord navigation to off-window
  tabs, oversized-title ellipsis. Goldens minted and reviewed
  (`sweep_pagehost_bar_*`). Usable at 40 columns.
- **(c) Modal larger than viewport** — at-open clamp already-correct
  (78x27@70x20, 90x22@40x12: title + buttons survive via the 0240
  floor, body scrolls). Resize-while-open was a DEFECT: no re-clamp,
  off-screen focus trap possible — FIXED (failing-first, §2).
- **(d) Drawer at small widths** — already-correct: `solve_rect`
  clamps `Percent`/`Cells` against the axis at open and on resize;
  exact extents pinned through the real pipeline.
- **(e) live resize** — engine-correct: single steps and coalesced
  bursts, shrink and grow, with everything open, match fresh-paint
  oracles cell-for-cell over garbage-prefilled referees; screenshot ==
  bytes-as-applied at every step. One HARNESS lesson (not an engine
  defect), worth recording for every future oracle author: the
  engine's per-thread singletons (drawer one-per-edge registry,
  viewport/theme/notices signals) make two live app worlds on one
  thread interfere — the oracle's drawer open `Replaced`-closed the
  incumbent's drawer through the shared registry, and the missing
  scrim masqueraded as a 12-cell "stale band" that read exactly like a
  damage bug. Oracles now build on a fresh thread
  (`harness::oracle_screen`); a probe test proved the engine corrects
  the real resize in ONE turn.
- **(f) unicode at narrow boundaries** — already-correct: Table
  cells, tab titles and Disclosure headers with CJK/emoji at
  glyph-splitting widths (24/23/21/19) truncate with whole-cluster
  drops + ellipsis; the wide-pair walk holds over every settled frame
  including a one-column-at-a-time 40→19→40 resize ladder. Goldens:
  `sweep_unicode_table_23`, `sweep_unicode_tabs_40`.

## 5. Cross-references

- Console-side pins for the original incident: findings 1020/1030 in
  the gateway-console seat's tracker (screenshots + their fixed-first
  reproduction). This wave fixes the ENGINE half; their hand-rolled
  closure keeps its own `rect.is_empty()` guard as defense-in-depth
  (recipe R4).
- Engine docs: `docs/api.md` § "Small terminals & content pressure"
  (guarantees + recipes R1/R3); CHANGELOG `[Unreleased]` (Fixed: the
  fusion class, Modal re-clamp; Added: the sweep).

## 6. Gate numbers (recorded at wave close)

- `cargo test --workspace`: 2111 passed, 0 failed, 101 ignored
  (baseline 2089 + the 21 sweep tests + 1 `ui::draw` unit pin).
- `cargo clippy --workspace --all-targets`: 0 warnings.
- `cargo fmt --all --check`: clean.
- `cargo semver-checks check-release -p abstracttui
  --baseline-version 0.2.14`: 196 checks pass, 57 skip — "no semver
  update required" (additive-clean: both fixes are behavioral;
  `modal_bounds` and the reclamp effect are private).

# Wave 9 — CANVAS final attack: dispositions on both open lists

Every item below is FIXED (failing-first where a fix landed, with the
citation), PINNED (a test makes the behavior a decision), or ARGUED
(kept, with the reasoning on the record). Test homes:
`extensions/graph/tests/view_attack_cycle3.rs`,
`extensions/graph/tests/view_interact.rs` (tooltip item),
`extensions/mermaid/tests/cycle3_attack.rs`,
`tests/wave_extensions_accept.rs` (the battery that found one more).

## My cycle-2 open list (GraphView)

1. **Bow amplitude at 3+ parallels / short chords — FIXED.**
   Outer bows arced off-grid and rendered as clipped stubs. Fix:
   `clamp_control` (view_edges.rs) clamps the quadratic's control
   point into the dot grid — the curve lies in the convex hull of
   {p0, c, p1}, so in-grid anchors + a clamped control keep the WHOLE
   stroke visible. Card overlap stays ARGUED-correct: cards paint
   over edges by z-order design (content is never obscured by
   strokes). Pin:
   `triple_parallel_edges_on_a_short_chord_stay_visible_and_in_bounds`.
2. **Aligned-first navigation on cluttered force layouts — PINNED +
   ARGUED.** Structural properties proven on a seeded 12-node force
   layout: every node has at least one outgoing direction (for any
   two distinct centers, some axis separates them — no stranding),
   and a directional walk strictly increases the projection, so it
   terminates without revisits (no ping-pong within a direction).
   Full-graph reachability by arrows alone is NOT guaranteed and not
   claimed. The widget's first hop is asserted equal to the property
   harness (`spatial_navigation_never_strands_and_directional_walks_terminate`).
3. **`ensure_visible` under padded roots / hidden scrollbar — FIXED.**
   The widget-rect approximation drifted by the padding. Fix: a
   paint-time viewport PROBE (plain `Cell`, no signal writes in draw
   — the RT1-2 law) records the scroll host's solved rect; the key
   handler reads the last-painted value, with the old approximation
   as the pre-first-paint fallback. Failing-first:
   `arrow_selection_scrolls_the_target_into_view_under_a_padded_root`
   (red on the approximation, green on the probe).
4. **Tooltip persistence across selection rebuild — PINNED, kept.**
   Clicking a hovered card rebuilds it; the per-generation scope
   disposal closes the tip. That is CORRECT (a stale tip over a
   restyled card would lie); any pointer motion over the rebuilt card
   re-arms and re-shows. Pinned in
   `view_interact.rs::tooltip_survives_selection_via_rehover_not_staleness`.
5. **Edge-label overprint — FIXED, twice.** (a) The "midpoint" of a
   2-point edge was `waypoints[len/2]` = the TARGET ANCHOR — labels
   printed into the target card (a real cycle-2 bug of mine, found by
   this attack's control case). Fix: `polyline_mid` — geometric
   middle for even counts, middle waypoint for odd. (b) Labels whose
   cell run would cross ANY card are now SKIPPED at plan time (a
   label over a title is illegible both ways); stroke overlap stays
   fine (labels paint last). Pin:
   `edge_labels_skip_when_they_would_overprint_a_card`.

### Found by the acceptance battery (not on any list)

6. **Card press fired on mouse DOWN — FIXED to release-inside (the
   engine Button convention).** When `on_node_press` opened a MODAL
   (the battery's drawer), the release routed to the overlay and the
   root tree's pointer capture never dropped — every later click
   anywhere pressed the captured card again (the battery's tab click
   pressed "parse" instead of switching pages). Cards now select +
   fire on `Up` inside the card rect. Proof: the battery end-to-end
   (`pipeline_monitor_scene_end_to_end`), plus every existing click
   test unchanged.

## Mermaid's open list (their named six)

1. **`---` open links — IMPLEMENTED (view side), end-to-end green.**
   `GraphView` now honors the `open` style hint: stroke kept
   (solid), arrowhead skipped; combines with dotted/thick. Failing
   first: `view_attack_cycle3.rs::open_style_edges_render_without_an_arrowhead`
   (red before the view change), then
   `cycle3_attack.rs::open_links_render_arrowless_end_to_end` through
   `MermaidView` (a mixed diagram keeps exactly the arrows it asked
   for). The mermaid crate docs' claim ("which GraphView renders as
   an arrowless stroke") is now true; the compiler was already
   emitting the hint.
2. **Adjacent-pair gap sizing vs long distant labels — ARGUED-correct
   + PINNED.** The long-span label truncates WITH an ellipsis
   (labeled, centered over its arrow) and the plan stays compact
   (gaps sized by adjacent pairs — observed 22 cells of boxes on a
   48-cell canvas). Stretching the plan for long spans would trade
   the whole diagram's density for one label; the documented design
   holds. Pin: `distant_pair_labels_truncate_with_a_visible_ellipsis`.
3. **Lowercase `note` — ARGUED contract-faithful + PINNED naming.**
   Outside the accepted spelling (docs capitalization); the verdict
   carries line number, verbatim line, and the honest generic reason,
   and the ON-SCREEN notice names the line ("line 3" + the text) —
   no silent surprise. Pin:
   `lowercase_note_falls_back_naming_the_exact_line`.
4. **Edge chaining — NOT widened; targeted reason VERIFIED.** The
   parser names it precisely ("edge chaining is not supported — one
   edge per statement", their `classify_bad_side`), the view shows
   that exact reason, and the fallback stays atomic (no partial
   diagram). Pin: `edge_chaining_falls_back_with_the_targeted_reason`
   (their parser test already pinned the reason string; mine adds the
   user-visible half).
5. **Arrows through intermediate lifelines at 2-cell gaps — VERDICT:
   LEGIBLE, golden-pinned.** The documented z-order (messages over
   lifelines) replaces exactly the crossing cell with the message
   line; the lifeline resumes on the rows above and below. Golden
   minted from the shipped painter:
   `arrows_cross_intermediate_lifelines_legibly_golden`.
6. **First-explicit-wins — PINNED for flowcharts, FIXED for
   sequences, doc line added.** Flowcharts already implemented the
   rule (`register()`: bare mentions never reset; first explicit
   shape/text wins) — pinned. The SEQUENCE path violated the crate's
   own rule: a message auto-registering an id made a later
   `participant id as Alias` silently drop the alias. Fixed
   (failing-first,
   `sequence_first_explicit_alias_wins_even_after_implicit_registration`;
   citation: flowchart.rs `register()` — one rule, both diagram
   kinds): the first explicit alias ENRICHES the implicit
   registration, column order stays first-encounter, later aliases
   never re-label. The rule is now stated in the crate docs
   (lib.rs "Declaration rule").

## Cross-checks

- Peer's BT/RL consumer fixture (`compile_graph.rs`) stays green over
  my cycle-2 `map_point` fix — confirmed from both sides.
- All fixes verified against the full workspace suite; gate numbers
  in the wave report.

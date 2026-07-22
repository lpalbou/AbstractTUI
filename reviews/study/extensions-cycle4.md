# Extensions study — cycle 4: final pass (2026-07-22)

Closing state of the extensions band (0400-0490) after four cycles:
9 items + track README, cross-reviewed both directions, all peer
findings folded, reconciliation with the control-plane track closed
(see the cycle-3 ADDENDUM: the seven corrections were a timestamp
race, resolved per reviews/study/platform-cycle4.md §1).

## Cycle-4 actions
1. **APPKITS residue folded**: 0430's tooltip mechanics now consume
   0500's anchored-popup **TOOLTIP routing mode** (passive,
   hover-timed, layer_draw-backed) instead of raw `Overlays` calls —
   the raw overlay API is the substrate 0500 builds on; the editor
   stays one abstraction up (placement/flip/clamp/dismiss never
   re-derived). M3 also names SELECT mode for in-card dropdowns.
2. **Cycle-3 addendum written**: the correction list is marked
   RESOLVED so no reader chases stale demands.
3. **Final newcomer read fixes**: 0420's blit gained the styled-blit
   variant line (a `render::Style` patch carries attrs + a
   0480-minted link id — the 0420↔0480 composition made explicit);
   0400's ADR title now says the NUMBER is a placeholder (control
   plane queues a protocol ADR too; docs/adr numbers globally,
   integrator assigns). No other stale cross-refs found; README
   sequencing covers 0480; verdicts current.

---

## INTEGRATOR HANDOFF BLOCK

### A. overview.md — Proposed-ledger rows for band 0400-0490
(Format matches the existing ledger: ID | Title | Track | Promotion
trigger.)

| ID | Title | Track | Promotion trigger |
| --- | --- | --- | --- |
| 0400 | Extension architecture: two cargo-feature classes (default-ON trim / default-OFF opt-in) + `abstracttui-*` sibling-crate family; ADR skeleton ready | extensions | Maintainer sign-off on the study; the ADR lands before/with the first 04xx packaging execution (0410 or the first sibling crate). |
| 0410 | Feature-gate the heavy in-tree modules (`three`, `jpeg`, `proto`) — default-on trim; measurements replace the study's estimates | extensions | 0400's ADR ruled + integrator Cargo.toml sign-off; batch with the 0.2 window (0170) so the feature matrix ships once. gltf_json neutral-home promotion ordered with control-plane 0320 (named on both sides). |
| 0420 | Canvas/vector layer in core: sub-cell dot canvas (braille/quadrant), bezier/arc, styled blit; chart refactor gated on byte-identical goldens | extensions | First diagram consumer scheduled (0440/0450) — or standalone on the chart-dedup merit. |
| 0430 | `abstracttui-graph`: interactive node-graph editor (cards, typed ports, bezier edges, pan, discrete-LOD zoom), staged M1-M3, keyboard-first | extensions | 0420 + 0440 landed; a named dataflow-editor consumer app; sibling-family launch gate (0170 pass) holds. |
| 0440 | `abstracttui-graph`: read-only auto-layout view — layered v1 (DAG-class only, honest boundary), designed bounded force v1.5 (knowledge-graph class); `GraphDesc -> Layout` is the module's one contract | extensions | 0420 landed + a named DAG-view consumer; v1.5 builds on the first knowledge-graph-class consumer. |
| 0450 | `abstracttui-mermaid`: spelling-exact honest subset (flowchart + sequence), atomic per-diagram fallback to the code fence, mermaid.live escape affordance | extensions | 0420 + 0440 landed; the mdpad rebuild (0460's validator) reaching its diagram phase. |
| 0460 | mdpad-class markdown reader enablement: capability-parity dashboard + four core-gap seeds routed to app-widgets | extensions | Maintainer green-light on the mdpad rebuild; the seeds promote individually in the app-widgets band (see B). |
| 0470 | Web/HTML rendering feasibility — verdict recorded: full web NEVER (JS/CSS-layout permanent non-goal); readable-subset extension slice gated on four criteria | extensions | All four criteria in the item (named consumer app class, 0460 vocabulary gaps landed, 0400 executed, fuzz-bar owner) — otherwise the verdict stands as the citable answer. |
| 0480 | Core seam: `StyledCanvas::register_link(uri) -> u16` (defaulted trait method — producer half of the 0165 link channel; OSC 8 terminal-side activation works pre-0165) | extensions | Any canvas-link consumer scheduling (0430 M3, 0450 in-feed links) or 0165's own scheduling — whichever first. Integrator option: merge into 0165 verbatim as its producer section. |

### B. 0460 seeds → app-widgets band (integrator numbers; suggested
non-colliding ids — band currently uses 0100/0110/0120/0130/0140/
0150/0160/0165/0170/0180, so 0142-0148 sit free between the lexers
and terminal-verbs items, topically adjacent to content fidelity)

| Suggested ID | Seed | Key content (full seed text in 0460 "Current code reality") |
| --- | --- | --- |
| 0142 | Markdown tables | Parse pipe tables + alignment row into a STATIC rich-pipeline block; share the width ALGORITHM (`solve_columns`, src/widgets/table.rs:374) — never embed the interactive Table widget; mdpad's staged wrap (protect typical columns, degrade to per-row records) is the quality bar. Coordinate with app-kits 0530 (same solver reuse lane). |
| 0144 | Markdown images | `![alt](src)` as a block element; in-flow = MOSAIC via the Image widget (decode widened PNG-only → `decode_image`'s PNG+JPEG, src/widgets/image.rs:121-130 vs src/gfx/decode.rs:58-67); pixel-protocol images inside scrolled content = named OPEN design note (overlay-scoped ImageSession does not flow); alt-text labeled fallback. |
| 0146 | Heading anchors + document map | Stable heading ids (slugging) from `md::Block::Heading`, a TOC extraction API, scroll-to-anchor; 0165 consumes the anchor ids for `#anchor` jump activation. |
| 0148 | Search-highlight span overlay | Highlight a match set over typeset rows WITHOUT re-parsing; design together with 0160's selection rendering — one overlay-span mechanism serves both. |

### C. Cross-track edges to record at the fold
1. **extensions ↔ control-plane**: control bus = core; control
   server = in-tree default-OFF `control-server` feature; MCP bridge
   + productized attach client = out-of-crate (both tracks' files
   agree). gltf_json → neutral-home promotion is a NAMED PRECONDITION
   ordered against 0410's `three` gate (named in 0320 and 0410 both).
   Actions stay nullary v1, `invoke_with`/`args` reserved
   (0310 "Cross-track answers"). Canvas-drawn content stays opaque to
   the semantic tree; extensions expose intent as actions/events.
2. **extensions ↔ app-kits**: anchored-popup substrate = CORE, owned
   with 0500 (0430 consumes TOOLTIP + SELECT modes; panned-absolute-
   card anchor case recorded on their side). App-kit choice controls
   = core class per 0400's recorded dry-run (NOT a sibling family).
   Seed 0142 coordinates with 0530's `solve_columns` reuse.
3. **extensions ↔ app-widgets**: 0480 (producer) + 0165 (consumer)
   are two halves of one link channel — merge decision is the
   integrator's; the four 0460 seeds live in this band (B above);
   0148 is designed with 0160.
4. **extensions ↔ live-data**: ADR-0004 does NOT grant a TLS/network
   dependency exception to sibling crates — that class rides 0050's
   transport ADR; any extension needing it waits.
5. **extensions ↔ ports/validators**: the mdpad rebuild is the
   viewer-class validation vehicle (0460's parity dashboard is its
   go/no-go), joining the validator table when green-lit.

### D. Track one-liner for overview.md "Topic tracks"
| extensions | `proposed/extensions/` | Proposed | Modularity architecture (two feature classes + the `abstracttui-*` sibling family, ADR-ready) and the diagram-class capability lane: core vector canvas + link-registration seam, node-graph widgets, mermaid subset, mdpad-reader enablement, and the standing web-rendering verdict. |

### E. ADR-0004
Skeleton ready for cutting: `reviews/study/extensions-cycle3.md`
§1(c) — seven decision points (feature classes, additivity rule,
family mechanics + 0170 launch gate, dependency-posture inheritance
with the 0050-deferred TLS exception, anchor surface, classification
dry-run outcomes, non-goals). The "0004" number is a placeholder —
the control-plane protocol ADR is also queued; docs/adr/README.md
numbers globally, integrator assigns at landing.

### F. Band file inventory (for the fold)
- Items: docs/backlog/proposed/extensions/{README, 0400, 0410, 0420,
  0430, 0440, 0450, 0460, 0470, 0480}.md
- Study records: reviews/study/extensions-cycle{1,2,3,4}.md,
  reviews/study/extensions-on-platform.md (cycle-2 review of the
  control-plane track; its §2 correction list carries a RESOLVED
  addendum). Peer review of this band:
  reviews/study/appkits-on-extensions.md (all 13 findings folded).

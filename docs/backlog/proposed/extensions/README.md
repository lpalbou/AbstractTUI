# Extensions backlog track

## Status
Proposed (authored cycle 1, cross-reviewed and converged cycles 2-3,
2026-07-22). Numbering band: **0400–0490**. Nothing here is scheduled.
0400 is the packaging trunk: every crate/feature decision cites it.
The two small CORE items (0420 canvas layer, 0480 link registration)
are informed by 0400 but not build-gated on it.

## Purpose
The maintainer's brief, verbatim: "abstracttui should be modular, so we
don't overload the default package, and extensions should be installed
only when needed." This track answers that with one architecture item
and a first family of concrete extension candidates, studied against
the real crate (0.1.0, published, test-pinned). It exists to keep the
core lean while making diagram-class content (node graphs, mermaid,
vector strokes) and heavier optional capability (3D, image codecs)
possible without bloating every downstream build.

Two ideas structure the track:

1. **Modularity has two mechanisms, not one.** Cargo *features* trim
   what is already in-tree (compile-time, additive, cheap to run);
   *sibling crates* add what was never in-tree (real "install only when
   needed", versioned separately, but coupled to core's API stability —
   ADR-0001's breaking-batch policy is the coupling budget). 0400 rules
   which mechanism serves which need; 0410–0480 are its first clients.
2. **Extensions consume public API only.** The engine already proves
   the discipline on itself: "widgets have no private engine
   privileges" (src/widgets/mod.rs:5-6). An extension crate is a
   downstream user with zero special access; every capability an
   extension needs and cannot reach is, by definition, a core backlog
   item (the canvas layer 0420 is exactly that).

## Ledger

| ID | Title | Verdict | Depends on |
| --- | --- | --- | --- |
| 0400 | Extension architecture: cargo features + sibling-crate family (ADR needed) | v1-able (decision + ADR) | — |
| 0410 | Feature-gate the heavy in-tree modules (`three`, jpeg decoder, pixel-protocol encoders) | v1-able, integrator-gated | 0400 |
| 0420 | Canvas/vector layer in core: sub-cell dot canvas + stroke primitives | v1-able | none (0400 informs naming) |
| 0430 | Interactive node-graph editor widget (`abstracttui-graph`), staged M1-M3 | needs-design | 0400, 0420; 0480 + 0165 (link channel — synergy, documented fallback); 0500 popup (M3 tooltips) |
| 0440 | Read-only auto-layout graph view (layered v1; designed force v1.5; `GraphDesc -> Layout` contract) | needs-design | 0400, 0420 |
| 0450 | `abstracttui-mermaid`: honest-subset diagram rendering (spelling-exact table, atomic fallback) | needs-design | 0400, 0420, 0440 |
| 0460 | mdpad-class markdown reader enablement (core gap list) | v1-able per gap | 0160/0165 (band 0100), 0450 |
| 0470 | Web/HTML readable-subset renderer: feasibility verdict | research (verdict recorded) | 0400, 0460 |
| 0480 | Link registration from draw closures (`StyledCanvas::register_link`) — CORE seam, producer half of 0165's channel | v1-able (small, additive) | none (0165 is the consumer half, either lands first) |

## Sequencing
- **0400 first** — it is a decision, not a build; every other item
  names its packaging (in-core vs feature vs sibling crate) against
  0400's ruling. Its ADR (skeleton drafted,
  reviews/study/extensions-cycle3.md) lands in `docs/adr/` per
  ADR-0001 discipline.
- 0420 and 0480 are the track's two *engine code* items, both small
  and additive: 0420 unblocks 0430/0440/0450 (strokes) and pays for
  itself by deduplicating `chart.rs`'s private braille plumbing; 0480
  is independent of everything (one defaulted trait method) and
  standalone-valuable (OSC 8 activation pre-0165). Both may land any
  time — 0400 informs their naming/placement, never their build.
- 0440 before 0430: the read-only layered layout is the risk retirement
  for the interactive editor (same substrate, no interaction surface);
  both share the `GraphDesc -> Layout` module contract, force() joins
  as v1.5 on the first knowledge-graph-class consumer.
- 0450 consumes 0440's layout engine; 0430's M3 (activation) wants
  0480 + 0165 (documented fallback until then) and 0500's
  anchored-popup (tooltips, ruled core with app-kits); 0460 consumes
  0450 plus the band-0100 depth items (0160 selection, 0165 link
  hit-testing) plus its four seeds once the integrator numbers them.
- 0470 is a standing feasibility record — promoted only by the criteria
  written in it, or closed as "verdict stands".

## Cross-track references (by band, per study protocol)
- **0100–0190 (app-widgets)**: 0165 link hit-testing is the CONSUMER
  half of the link channel whose PRODUCER half is this track's 0480 —
  together they are 0430's edge/port activation and 0460's link
  surface (integrator may merge 0480 into 0165; the spec stands either
  way); 0160 selection/copy feeds 0460's search/copy story; 0460's
  four gap seeds (md tables / md images / heading anchors / search
  highlight) route to this band for numbering; 0120 TextArea is the
  item grammar template.
- **0300–0390 (control-plane, PLATFORM)**: placement answered in
  cycle-2 cross-review (reviews/study/extensions-on-platform.md P1-2):
  automation bus = core; control server = in-tree default-OFF
  `control-server` feature; MCP bridge + productized attach client =
  out-of-crate. The 0320 JSON promotion out of `three/` is a shared
  precondition with 0410's `three` gate.
- **0500–0590 (app-kits, APPKITS)**: classified by 0400's own rule in
  the cycle-2 dry-run — choice controls/form machinery are CORE-class
  (minimal apps want them; no independent cadence), NOT sibling-crate
  candidates by default. 0400's naming/versioning/CI policy covers
  whichever kit-level compositions ever do qualify. The anchored-popup
  primitive lands in core with 0500 and joins 0400's anchor-surface
  list; extension widgets (0430) consume it as public API.

## Non-goals of the extension system (track-level)
Recorded here so no item re-litigates them; 0400 carries the rationale.
- **No dynamic loading.** No dlopen/ABI plugin interface, no runtime
  plugin registry. Extensions are ordinary Rust crates linked at build
  time. (A stable Rust ABI does not exist; a C ABI boundary would cost
  the engine its zero-cost internal contracts.)
- **No scripting runtime** (Lua/JS/WASM hosts) — that is an
  application's choice, never the engine's.
- **No extension discovery/marketplace machinery.** crates.io is the
  registry; `abstracttui-*` naming is the discovery.
- **No private hooks.** Extensions build on the public API; if the
  public API is insufficient, the gap is filed as a core item — never
  solved with `#[doc(hidden)]` back doors.
- **No behavior-changing features.** Cargo features stay additive
  (feature unification safety): enabling one must never change what
  existing code does, only add capability; a feature-off runtime path
  degrades with a named error, mirroring `decode_image`'s honest
  rejection (src/gfx/decode.rs:62-67).
- **No fork of the token/theming discipline.** Extension widgets obey
  the same token rule core widgets are lint-pinned to
  (src/widgets/mod.rs:150-169): tokens in, no invented colors.

## Process notes
- This track was authored as a study; `docs/backlog/overview.md` is
  integrator-owned and is deliberately NOT edited here (three parallel
  study tracks would race on it). Folding the 0400–0490 ledger into the
  overview is an integrator action on adoption.
- Item grammar follows `docs/backlog/planned/app-widgets/0120_textarea_multiline_composer.md`.
- Study evidence: `reviews/study/extensions-cycle1.md`.

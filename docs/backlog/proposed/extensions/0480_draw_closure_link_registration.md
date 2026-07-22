# 0480 — Link registration from draw closures (`StyledCanvas::register_link`)

## Metadata
- Created: 2026-07-22 (cycle 3 — closes the cycle-2 open question from
  appkits-on-extensions P1-1)
- Status: Proposed (CORE item authored in this band; integrator MAY
  merge it into 0165 at fold time — see "Placement decision" below)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: ADR-0001 (additive: one defaulted trait method + two
  overrides). ADR impact: none — the OSC 8 emission contract, the
  link-id cap semantics, and 0165's planned event surface are all
  unchanged; this is the PRODUCER half of the existing link channel.

## Placement decision (why this is its own item, in this band)
The cycle-2 peer review (appkits-on-extensions P1-1) proved the gap
and proposed "fold into 0165's scope". Considered; decided against a
silent fold, for three reasons:
1. **The producer half has standalone value without 0165.** The
   presenter already emits OSC 8 for linked cells when the terminal
   supports it (`set_link`, src/render/present.rs:385-392) — a link
   registered from a draw closure is terminal-side activatable
   (ctrl/cmd-click) TODAY. 0165 adds app-side hit-testing; this item
   is useful shipped alone.
2. **Band discipline**: this track cannot author inside 0100-0190; an
   amendment note living only in a review file is exactly the artifact
   that gets lost at fold time. A complete item is mergeable.
3. **One channel, two halves, cleanly split**: 0165 = consumer side
   (pointer dispatch resolves the cell's URI); 0480 = producer side
   (draw code can mint the id). Either lands first without waiting on
   the other. If the integrator prefers one item for the whole
   channel, this file merges into 0165 verbatim as its producer
   section — that choice is theirs; the specification below stands
   either way.

## Context
Rich-text content mints link ids through `resolve_link(s: &mut
Surface, span)` (src/render/rich.rs:302-306) — a path that requires
holding `&mut Surface`. Widget draw closures never hold a `Surface`;
they receive `&mut dyn StyledCanvas` (`DrawFn`, src/ui/view.rs:20).
A `render::Style` can CARRY a link id (`Style::link(id)`,
src/render/style.rs:148; the id field at style.rs:48-49) and
`SurfaceCanvas::print_styled` preserves it end-to-end
(src/ui/canvas.rs:234-239) — but nothing reachable from a draw closure
can REGISTER a URI to obtain an id in the first place. Consequences,
verified in cycle 2: the node-graph editor (0430) cannot stamp edges
with `graph://edge/42`; a mermaid diagram in a `Feed` (0450 via
`CustomBlock`, whose draw is also `&mut dyn StyledCanvas`,
src/widgets/feed.rs:90-101) can render but never carry activatable
links; ANY app custom-draw surface has the same wall. The class-level
need is every canvas-drawn surface with references — graphs, diagrams,
custom traces, minimaps.

## Current code reality
- `Surface::register_link(uri) -> u16` (src/render/surface.rs:125-135):
  INTERNS by URI (repeated registration of the same URI returns the
  same id — so per-damage re-draws are naturally idempotent), hard cap
  with counted drops returning id 0 (`LINK_TABLE_CAP`,
  `links_dropped`, surface.rs:129-131,147-150; "never wrapped — a
  wrapped id would mislink, which is worse than dropping").
- `Surface::link_uri(id)` reverse lookup (surface.rs:138-145) — the
  consumer half 0165 rides.
- The trait seam: `StyledCanvas: Canvas` (src/ui/canvas.rs:22-43) has
  print_styled/fill_styled only; implementors are `SurfaceCanvas`
  (wraps `&mut Surface` — CAN register), `BufferCanvas` (test canvas —
  can store a test link table), `ClippedCanvas` (pure forwarding
  wrapper, canvas.rs:261-356 — must delegate).
- The emission half already works: presenter OSC 8 with URI identity
  (`set_link`, src/render/present.rs:385-392), capability-gated.

## The two API options, decided
**Option A — defaulted trait method (RECOMMENDED, the specification):**

```rust
pub trait StyledCanvas: Canvas {
    // ...existing methods...

    /// Intern `uri` in the underlying surface's link table and return
    /// its cell id for `Style::link(id)`. Plain canvases (no link
    /// table) return 0 = "no link": content renders identically,
    /// minus activation — the honest degradation, matching the
    /// cap-exhausted behavior of `Surface::register_link`.
    fn register_link(&mut self, _uri: &str) -> u16 { 0 }
}
```

- `SurfaceCanvas` overrides → `self.surface.register_link(uri)`
  (inherits interning + cap + counted-drop semantics unchanged).
- `ClippedCanvas` overrides → `self.inner.register_link(uri)`
  (forwarding wrapper; a clipped draw must mint real ids).
- `BufferCanvas` overrides with a small owned table so unit tests can
  assert registered URIs without a compositor.
- Usage from any draw closure:
  `let id = canvas.register_link("graph://edge/42");`
  `canvas.print_styled(p, "──", &Style::new().link(id));`
- Default = 0 keeps the trait object-safe, keeps every existing
  implementor compiling (additive under ADR-0001), and makes the
  degradation honest by construction: id 0 is already the documented
  "no link" value across the render stack.

**Option B — id minting outside the trait (REJECTED):** pre-register
URIs at view-build time against some registry handle. Rejected because
link ids are SURFACE-LOCAL by design (per-surface intern table with a
per-surface cap; overlay layers are distinct surfaces) — a build-time
mint cannot know its surface, and a global registry would re-architect
the link model 0165 depends on for exactly zero consumer benefit.
Recorded so the next reader does not re-derive it.

## What we want
1. The Option-A trait method + the three implementor overrides.
2. Contract text on the method: interning semantics, the cap +
   counted-drop inheritance, id lifetime = the surface's (draw code
   must re-register per draw pass, which interning makes free), and
   the id-0 degradation.
3. One docs paragraph on the link channel's two halves: producer
   (this seam, works with terminal-side OSC 8 activation today) and
   consumer (0165 hit-testing, band 0100) — with the app-URI
   convention pointer (0165 §3 owns that vocabulary).
4. Tests (below).

## Scope / Non-goals
Scope: the trait method, overrides, contract text, docs paragraph,
tests. Non-goals: hit-testing/event delivery (0165's half — including
which composed layer answers under overlap); URI vocabulary or scheme
suppression rules (0165 §3); per-dot link granularity (ids live on
CELLS — a 2x4-dot braille stroke links per cell, documented in 0430's
honest hit-testing scoping); raising `LINK_TABLE_CAP` (a dense graph
hitting the cap gets counted drops — evidence first, then a cap item
if a real consumer demonstrates need).

## Expected outcomes
0430's edge/port activation plan and 0450's in-feed diagram links stop
depending on an unfiled seam; any app draw closure can mint links in
two lines; on OSC 8 terminals the links are ctrl-clickable before 0165
even lands.

## Validation
- Unit: default returns 0; `SurfaceCanvas::register_link` interns
  (same URI → same id across calls), respects the cap (drop counted,
  id 0 returned); `ClippedCanvas` forwards; `BufferCanvas` table
  records URIs.
- Render pin: a draw closure registering a URI + printing with
  `Style::link(id)` produces cells whose `link_uri` resolves to the
  URI (surface-level), and the presenter emits OSC 8 for them under
  `caps.hyperlinks` (extend the existing present-layer link tests).
- Cap pin: registrations past `LINK_TABLE_CAP` from a draw closure
  render plain text and count drops — no panic, no wrap (mirrors
  src/render/surface_tests.rs's existing cap pin).
- Idle pin unchanged: registration happens only inside draw passes
  (damage-driven); an idle canvas registers nothing.

## Progress checklist
- [ ] Integrator placement call: standalone (as authored) vs merged
      into 0165 — either way the spec above is the content
- [ ] Trait method + SurfaceCanvas/ClippedCanvas/BufferCanvas overrides
- [ ] Contract text + docs paragraph (two halves of the channel)
- [ ] Unit + render + cap pins
- [ ] 0430/0450 references updated when this lands (already pointing
      here as of cycle 3)

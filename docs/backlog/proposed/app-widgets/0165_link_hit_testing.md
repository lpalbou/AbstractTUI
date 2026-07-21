# 0165 — Hyperlink/reference hit-testing through the event path

## Metadata
- Created: 2026-07-21
- Status: Proposed (P2 in the evidence; promote when a dogfood app
  reaches its "activate a reference" phase)
- Track: app-widgets
- Completed: N/A

## ADR status
- Governing ADRs: None — this repo has no ADR system yet (see 0170).
  ADR impact: None expected (additive event surface; the OSC 8 emission
  contract and the link-id cap semantics are unchanged).

## Context
Rendered content is full of references users want to activate: URLs in a
chat message or a document, `file:line` locations in a compiler error or
a tool-result preview, message ids in a feed. Chat clients, consoles,
viewers, and monitors all share the need — "the terminal underlines it
but the app cannot react to it". The completeness review files it as
P2-7 (`reviews/cycle11/completeness-and-code-port.md` §2b: app-level
"click file:line → open in detail panel" needs link-id hit-testing
exposed through the event path), and 0100's non-goals defer it here. The
console port (0200, jump-to-detail-panel) and the chat port (0210, open
a URL) are the first validators, not the design targets.

## Current code reality
- Links are first-class in the render model: cells carry a
  surface-local hyperlink id (`Cell::link`, src/render/cell.rs:310-311),
  interned per-surface with a hard cap and counted drops — never wrapped
  ids (`Surface::register_link`, src/render/surface.rs:121-131;
  `links_dropped`, surface.rs:148; pinned by
  src/render/surface_tests.rs:276).
- Authoring exists end-to-end: `Span::with_link(url)`
  (src/render/rich.rs:54), markdown link spans carry their URL
  (src/render/md.rs:374), draw resolves URL → surface id
  (`resolve_link`, rich.rs:302), and the presenter emits OSC 8 when the terminal
  supports it (src/render/present.rs:385, `set_link`; identity is by
  URI, not id — src/render/diff.rs:364).
- The URI is recoverable from a cell: `Surface::link_uri(id)`
  (src/render/surface.rs:138) — the lookup half of hit-testing already
  exists.
- The event path knows nothing of it: pointer dispatch resolves
  *elements* (click/hover/drag with capture, tests/adv_pointer.rs);
  nothing consults the composed frame's cell under the pointer, so no
  component can learn "the user clicked the cell whose link is X"
  (grepped src/ui/: no link references in dispatch or view).
- Terminal-side OSC 8 activation (ctrl/cmd-click) is the TERMINAL
  opening the URI itself — invisible to the app by protocol design, and
  absent on non-supporting terminals; it cannot carry app-internal
  references at all.

## Problem
The engine can paint a reference but no app can respond to it. The
workaround is per-widget mouse math re-deriving which span sits under a
click — exactly the cell/cluster arithmetic the engine owns — and it
still fails across widget boundaries. Meanwhile app-internal references
(jump to a message, open a file at a line) have no channel at all on any
terminal, because OSC 8 activation never reaches the app.

## What we want (proposed shape)
1. **Link hit-testing in pointer dispatch**: when a mouse event lands,
   resolve the composed cell under the pointer; if its link id is
   nonzero, surface the URI alongside the element-level event — e.g. a
   `link: Option<&str>`/owned-URI field on the pointer event context, or
   an `on_link(uri, event)` element hook. The lookup rides
   `Surface::link_uri`; which composed surface answers (layer-aware,
   top-most wins) is the design ruling this item needs.
2. **Hover parity**: the same resolution on hover, so apps can show a
   status-line preview ("open https://… / open src/foo.rs:42") — honest
   affordance before the click.
3. **App-internal URI convention documented, not invented**: apps mint
   their own scheme (e.g. `app://msg/123`, `file://…#L42`) in
   `Span::with_link`; the engine treats URIs as opaque strings
   (unchanged) and the docs show the pattern once. Whether such
   references should ALSO reach the wire as OSC 8 (a terminal cannot
   open `app://…`) needs a ruling: suppress-from-emission per scheme vs
   emit-and-shrug.
4. Degradation honesty: past the link-table cap, cells carry id 0 and
   are not hit-testable — the counted-drop behavior already exists;
   contract text must say hit-testing shares it.

## Scope / Non-goals
Scope: the dispatch-side resolution, the event surface, hover parity,
docs + one example (a feed whose file:line references focus a detail
pane), tests. Non-goals: changing OSC 8 emission or the id-cap
semantics; a URI parser or scheme registry (URIs stay opaque); keyboard
link navigation (tab-through-links is a later accessibility item if a
class demands it); clickable-region machinery for arbitrary non-link
spans (links are the vocabulary; apps can mint URIs).

## Expected outcomes
A rendered URL or `file:line` is activatable in one line of app code on
every terminal — including terminals with zero OSC 8 support, because
activation is app-side; hover tells the user what a click will do; the
console's jump-to-panel and the chat client's open-link stop being
per-widget mouse math.

## Validation
- CaptureTerm acceptance: click on a linked span delivers the URI to the
  handler; click one cell past it delivers none; wide-glyph leaders and
  continuation cells resolve to the same link; top layer wins under
  overlap.
- Hover: URI surfaces on hover-enter, clears on hover-exit.
- Cap behavior: a surface past `LINK_TABLE_CAP` keeps rendering plain
  text (existing pin) and delivers no phantom hits.
- Idle pin: hover resolution adds no cost while the pointer is still.

## Progress checklist
- [ ] Design ruling: event surface shape + which composed layer answers
- [ ] Dispatch-side cell/link resolution (click + hover)
- [ ] App-internal URI docs + emission ruling for non-web schemes
- [ ] Example: file:line references focusing a detail pane
- [ ] Acceptance/cap/idle tests

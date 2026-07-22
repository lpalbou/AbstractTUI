# 0148 — Search-highlight overlay for rich/markdown views

- Status: proposed
- Track: app-widgets
- Origin: seeded by extensions/0460, cycle-4 handoff
- Depends on: design coordination with 0160 (content selection) — both
  need a "typeset text ↔ screen cells" mapping; build the mapping once

## Problem

A reader (mdpad-class) needs find-in-document: highlight all matches,
jump next/previous, live count — over typeset (wrapped, styled) text
whose screen positions differ from source offsets.

## What we want to do

(1) A query→match API over the typeset result (source-offset matches
mapped to cell rectangles per line fragment); (2) a non-destructive
highlight pass (style patch at draw time — background tint from theme
tokens; current-match distinct from other-matches); (3) scroll-to-match
composing with Scroll (and follow-tail disengage semantics); (4) match
count as a signal for the search bar.

The text↔cells mapping is the shared substrate with 0160 selection;
whichever lands first builds it and the other consumes.

## Validation

Highlight rectangles correct across wrapping/styled spans (goldens);
zero idle cost when the query is empty; large-document search latency
measured; scroll-to-match honors the damage contract.

Full analysis: docs/backlog/proposed/extensions/0460 (§seeds).

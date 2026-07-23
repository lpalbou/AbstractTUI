# 1220 — Doc vocabulary: nested blockquotes + multi-block list items

## Metadata
- Created: 2026-07-25 (wave 13, ARCHITECT-MD lane; 1200+ numbering —
  see 1200 for the range note)
- Status: Proposed
- Track: app-widgets
- Completed: N/A

## Problem

The core vocabulary deliberately folds `>` nesting into one level and
keeps list items single-line (documented degradations, render.md §2.8).
Model output in the wild uses both: `> > cited reply` shapes in chat
transcripts, and list items carrying continuation paragraphs or fenced
code (very common in LLM "steps" answers — the continuation lines
currently parse as separate paragraphs at top level, losing the
indent relationship). mdpad renders both correctly via pulldown-cmark's
recursive model (`Quote(Vec<Block>)`, `ListItem { blocks }`).

## Proposal

Grow the DOC vocabulary (the additive lane — `Block` is frozen
exhaustive, `DocBlock` is `#[non_exhaustive]` for exactly this):

1. `DocBlock::QuoteGroup { depth, blocks }` — consecutive quote lines
   parse recursively after stripping one `>` per depth level; the
   typesetter draws one bar per depth (`▎▎`), content indented 2 per
   level, muted ink once (not compounded).
2. List-item continuations: an indented line following a list/task item
   (4+ spaces or matching the item's content column) attaches to the
   item as a continuation block (paragraph or fence). Typeset with the
   item's hanging indent — the shape `push_block` already draws for
   wrapped items.
3. Streaming: quote groups and item continuations seal at the first
   non-member line, mirroring the table open/close rules
   (`DocStreamSession` classifiers grow the same way batch does —
   streamed-vs-batch equivalence stays test-pinned).
4. Core `parse` stays byte-stable (the honest subset contract);
   only `parse_doc` learns the shapes — same relationship as tables.

## Validation

- Golden: a two-level quote renders two bars and single muting; a list
  item with a fence keeps the fence indented under the item marker.
- Equivalence: `DocStreamSession::finish() == parse_doc(whole)` across
  chunkings of nested inputs (the existing property extended).
- Degradation: pathological nesting (> 8 levels) clamps like list
  depth does today — never a panic, never silent loss.

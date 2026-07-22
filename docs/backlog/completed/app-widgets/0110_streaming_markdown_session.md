# Completed: 0110 — Streaming markdown session (open-tail re-parse only)

## Metadata
- Created: 2026-07-21
- Status: Completed (app-widgets wave, CONTENT seat — cycle 1)
- Track: app-widgets
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: None expected (additive `render::md` API; the parser's
  supported-subset contract is unchanged).

## Context
Text that arrives incrementally is a general workload, not an agent
niche: model output in any assistant-facing surface, a log tail growing
line by line, a document body filling over a slow pipe. Tokens arrive at
30–100 events/s and the UI should re-typeset only what can still change.
The completeness review names this P0-2 with the coding-console port
(0200) as its first validator, and observes the fallback such apps use
today (render streaming text plain until the turn ends) — acceptable,
but the engine can do better, and 0100's "streaming tail item" needs
exactly this seam.

## Current code reality
- `src/render/md.rs:155-181` — `md::parse(src, styles)` is a clean,
  total, whole-document block parser (every input parses; degradation is
  "treat as literal text"). Fences consume lines until the closing fence
  "(EOF also closes: honest recovery)" (md.rs:175) — good for a static
  document, wrong mid-stream: an open fence would flip from
  code-rendering to literal-text depending on where the stream happens to
  pause, unless the session models "still open".
- `src/render/md.rs:51-81` — the `Block` vocabulary (Paragraph, Heading,
  ListItem, Blockquote, CodeFence, Rule): block boundaries are
  line/blank-line driven, so "closed vs open block" is decidable from the
  input suffix alone.
- `src/widgets/markdown.rs:108-124` — `MarkdownView` caches typeset rows
  per width but re-parses the whole source whenever the source changes;
  1,000-line parse measured at ~994 µs (completeness review §0). Per-token
  streaming through it is O(document) per delta.
- `src/render/rich.rs:14-19` — typeset rows ("parsed once, rendered many
  frames") are the natural frozen currency for closed blocks.

## Problem
There is no incremental entry point into the markdown pipeline: the only
API is parse-everything. A streaming consumer must either re-parse the
whole accumulated source per delta or bypass markdown until the stream
closes. Both reviews call for the same shape: freeze what cannot change,
re-parse only the block still receiving text.

## What we want
`md::StreamSession` (in `render::md`, widget-agnostic):
1. `append(&str)` accumulates deltas; the session maintains a list of
   **closed blocks** (frozen — parsed once, optionally typeset once by
   the consumer) and one **open tail block** (re-parsed from its start on
   each delta; a paragraph/fence start is O(that block), never
   O(document)).
2. Block-closing rules mirror `md::parse` exactly (blank line closes a
   paragraph, closing fence closes a fence, heading/rule lines are
   self-closing) so that `finish()` yields a block list **identical** to
   `md::parse` of the full source — the equivalence is the correctness
   contract and the main test.
3. Mid-fence honesty: an unclosed fence reports as an open `CodeFence`
   block (renders as code with the fence's lang from the moment the
   opening fence line arrives), never flapping to literal text.
4. `finish()` closes the tail (EOF-closes an open fence, matching
   md.rs:175) and marks the session complete.
5. Consumer seam for 0100: closed blocks surface once (so the Feed can
   typeset-and-freeze them); the open block surfaces per delta.

## Scope / Non-goals
Scope: the session type, its equivalence contract, and the 0100
integration seam. Non-goals: widening the supported markdown subset
(tables, nested emphasis etc. stay out — md.rs:14-17 is deliberate);
incremental re-typeset *within* the open block (re-parsing one block per
delta is already cheap); a public streaming `MarkdownView` widget (0100's
tail item is the consumer; a standalone widget can come later if wanted).

## Expected outcomes
Streaming N tokens into a session costs O(open block) per token; the final
block list is byte-equivalent to a whole-document parse; 0100's tail item
and any future streaming surface ride one tested implementation.

## Validation
- Property test: for a corpus of documents (reuse the fuzz corpus shape —
  the markdown fuzzer already runs 5,000 hostile cases), any chunking of
  the same bytes through `append` + `finish` yields blocks identical to
  `md::parse` of the whole. Split-invariance is the same property the
  input parser already pins (robustness review R2) — copy the technique.
- Mid-fence rendering test: open fence renders as code before the close
  arrives.
- Perf assertion: appending into a session with 1,000 closed lines does
  not re-parse them (counting parse work or via the alloc budget).

## Progress checklist
- [x] StreamSession with closed/open block split
- [x] Equivalence-with-parse property test (chunking-invariant)
- [x] Mid-fence open-block semantics + finish()
- [x] 0100 tail-item integration seam
- [x] Perf/alloc pin for closed-block freezing

## Completion report
- Final path: docs/backlog/completed/app-widgets/0110_streaming_markdown_session.md
- Date: 2026-07-21
- Shipped: `render::md::StreamSession` (src/render/md_stream.rs, tests
  in md_stream_tests.rs; design record in reviews/wave/content-cycle1.md).
  A child module of `md` reusing the batch parser's own private line
  classifiers, so block-boundary rules cannot drift. Each append seals
  the longest provably-final prefix (safe cut = line start outside any
  open fence that cannot soft-join a paragraph; the incomplete final
  line is classified WORST-CASE — a `---` fragment never seals, a
  committed ` ``` `/`>`/`# `/`- `/`1. ` prefix seals immediately) and
  re-parses only the open tail. Surface: `append`, `finish`
  (EOF-closes fences, idempotent), `closed_blocks` (frozen,
  index-stable), `closed_revision` (the 0100 typeset-once seam),
  `open_blocks`, and the honest cost meters `open_len` /
  `bytes_reparsed_total`.
- Tests (src/render/md_stream_tests.rs):
  `any_chunking_equals_batch_parse` (39-doc corpus × 7 chunkings),
  `randomized_documents_hold_the_equivalence` (200 random documents
  from boundary-hostile fragments),
  `open_fence_reports_as_code_before_the_close`,
  `rule_shaped_fragment_does_not_seal_the_paragraph`,
  `committed_fragments_seal_the_preceding_paragraph`,
  `closed_blocks_only_append_and_revision_tracks_growth`,
  `appends_behind_closed_content_cost_only_the_open_block` (byte-meter
  cost pin: 50 tail tokens behind 1,000 closed lines re-parse only the
  open region), `finish_is_idempotent_and_eof_closes_fences` + empty
  edges. Through the widget stack: feed_tests
  `streamed_item_matches_static_item_pixels` (pixel parity with a
  static render) and wave_content
  `tail_tokens_behind_closed_blocks_typeset_only_the_open_block`
  (60 tokens behind 40 closed blocks re-typeset exactly 60 blocks,
  driven through the real frame loop).
- Cost pin form: work counters (`bytes_reparsed_total` at the session,
  `blocks_typeset_total` at the feed) instead of allocator counting —
  the same freeze assertion, deterministic under test parallelism.
- One precision vs the item text (recorded in cycle 1): the open
  region is USUALLY one block but can transiently parse to more
  between an append and its seal — never observable as wrong output;
  `open_blocks()` returns a slice for exactly this reason.

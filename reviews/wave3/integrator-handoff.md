# Wave 3 — INTEGRATOR seat handoff (feed doc-vocabulary adoption)

Date: 2026-07-23. Seat: INTEGRATOR (cycle-2 integration builder).
Discharges the seam READER's handoff named ("Feed adoption of the doc
vocabulary … deliberately left to the feed owner — the pieces are
ready, the wiring is a product call"): the wiring is DONE, plus the
residual-warnings sweep, example/capture coherence, and the api.md
coherence pass. Gates green (whole-tree tests, clippy `--all-targets`
zero, `RUSTDOCFLAGS="-D warnings" cargo doc` clean, fmt clean, alloc
pins, `cargo semver-checks` vs published 0.2.2: 196 checks pass —
no API shape change).

## Ledger row (for FIXNET — the single overview.md writer)

| item | title | state | note |
| --- | --- | --- | --- |
| (integration, no backlog id) | Feed adopts the md doc vocabulary | completed (wave 3, INTEGRATOR) | `FeedItem::markdown` → `parse_doc`; streams → `DocStreamSession`; tables/images/tasks/strike in Feed; streamed table = open region (cost-pinned); captures byte-identical on core sources |

If the overview tracks the reader-wave items' follow-ups: 0142's
completion note names feed adoption as the named follow-up — this row
closes it. No backlog file was created for the adoption itself (it was
a handoff-named seam, not a filed item); point the row at this file.

## What changed (files)

- `src/widgets/feed_typeset.rs` — `StreamEntry.session` is
  `md::DocStreamSession`; closed/open blocks typeset via
  `push_doc_block`; static `ItemBlock::Markdown` parses via
  `parse_doc`; new `doc_block_separates` helper mirrors the
  segment-boundary blank policy for the doc vocabulary (list AND task
  items stack tight; future `DocBlock` kinds typeset to nothing, so no
  separator either).
- `src/widgets/feed.rs` — session construction + module docs (doc
  vocabulary stated in the content model; `stream_finish` EOF-closes
  tables too).
- `src/widgets/feed_item.rs` — `FeedBlock::Markdown`/`FeedItem::markdown`
  doc comments name the doc vocabulary.
- `src/widgets/feed_tests.rs` — parity test now streams a
  table/task/strike source in hostile 3-byte chunks + asserts attrs;
  three new tests (names below).
- `examples/transcript.rs` — fourth scripted turn streams a table +
  task list + strikethrough; header docs updated.
- `tests/live_smoke.rs` — `live_transcript` appended (pty; exits
  through the `/quit` completion → composer path proven live).
- `examples/capture/app_shots.rs` + `main.rs` — new `reader-table`
  in-process shot (byte-deterministic; two-run diff verified);
  manifest wording updated to five clockless stills.
- `examples/README.md` — transcript row + section updated.
- `docs/api.md` — see the coherence pass below.
- `src/gfx/probe.rs` — rustdoc link fix (the known wave-1 residue).
- `CHANGELOG.md` — "Changed (integration — Feed adopts the doc
  vocabulary)" inside `[Unreleased]`.
- `cargo fmt` tree-wide: also settled a pre-existing diff in
  `src/app/select_tests.rs` (fmt-only, zero behavior — flagging since
  that file was being edited concurrently this cycle; if you own it,
  the only delta from me is assert-message line wrapping).

## How streaming tables stay open-region-only (4 lines)

1. `DocStreamSession.seal()` refuses to seal from a table's header line
   (or any unresolved pipe-line candidate) until its first non-pipe
   line — so the WHOLE in-flight table lives in the open tail.
2. Feed's `typeset_entry` typesets `closed_blocks()[closed_seen..]`
   once into frozen segment 0 and re-typesets only `open_blocks()`
   into segment 1 per delta — a table row append re-typesets exactly
   one block (cost-pinned ≤ 1 per append with 20 closed blocks ahead).
3. The closed/open segment boundary's blank-separator policy is
   mirrored by `doc_block_separates` (the one place `push_doc_block`
   cannot see prior rows); drift = pixel diff in the parity test.
4. Batch/stream equivalence is the session's contract (same
   classifiers as `parse_doc`), so streamed-vs-static pixel parity
   holds at every chunking — pinned with 3-byte chunks over the full
   doc vocabulary.

## Test names added

- `widgets::feed::tests::streamed_item_matches_static_item_pixels`
  (EXTENDED: table + task + strike source, attrs asserted, doc-render
  proof assertions).
- `widgets::feed::tests::streamed_table_retypesets_only_the_open_region`
  (the token-cost pin over a streamed table: grow ≤1/append, seal
  freezes, tail tokens never revisit it).
- `widgets::feed::tests::markdown_items_render_tables_tasks_and_strikethrough`
  (bold header, border-ink rule, right-aligned cells, checkbox ink,
  STRIKE attr scoped to the struck span).
- `widgets::feed::tests::feed_item_with_image_measures_without_decoding`
  (probe-sized extent with the image clipped out of a fixed box —
  decode_count unchanged; scrolling it into view decodes exactly once,
  mosaic pixels verified).
- `live_smoke::live_transcript` (pty, ignored-live).

## Warnings swept (tree-wide, fresh full rebuild)

- `src/gfx/probe.rs` — rustdoc `unresolved link to decode_image`
  (the wave-1 residue): now an explicit `crate::gfx::decode_image`
  link. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` is clean.
- The "clippy items in markdown_image.rs" named in my brief did not
  reproduce on a fresh `cargo clippy --all-targets` (READER appears to
  have cleared them before handoff); the only two clippy warnings on a
  fresh full rebuild were introduced-and-fixed within this wave
  (unreachable-pattern in my own helper; doc_lazy_continuation from a
  `+`-leading doc line in transcript.rs). Clippy is ZERO tree-wide,
  including button.rs / live-data files / workflows (nothing owed to
  those peer lanes).
- REVIEWER: your in-flight `tests/wave_c2_review.rs` had one
  `clippy::type_complexity` on the `mount_synced_with_selection`
  holder local — fixed minimally with a `SyncWires` alias (line ~113;
  zero behavior, your 13 tests still pass). If you rewrite the file,
  keep an alias there or clippy zero breaks again.

## Example / capture state

- Headless exit-0 verified for reader, voice_mock, transcript, feed
  (`</dev/null` → notice + exit 0).
- Live pty smokes green: `live_reader`, `live_voice_mock`,
  `live_transcript` (new).
- Captures: `reader-table.{txt,styled.txt}` added to the apps family
  (in-process, clockless; two consecutive runs byte-identical). The
  four PRE-EXISTING app shots regenerate BYTE-IDENTICAL after the doc
  switch — the core-source identity (`parse_doc == parse ∘ Core`)
  proven through the whole app stack, not just the parser test.
  `docs/captures/README.md` manifest regenerated (adds reader-table).
- `examples/README.md`: transcript row/section updated; reader +
  voice_mock rows verified accurate as READER/INPUTAV left them.

## api.md coherence pass (what moved, what was added)

- Widgets catalog: Feed bullet states the doc vocabulary + streamed
  tables; MarkdownView bullet names tables/images/outline/search;
  NEW Meter/AudioScope bullet (they were in the appended section but
  missing from the catalog where widget users look first).
- "Feed — streaming transcripts": now the authoritative statement of
  the adoption (DocStreamSession, streamed-table-as-open-region,
  probe-sized images, measure-without-decode).
- The reader-surface section cross-references Feed (one shared
  recipe) instead of implying MarkdownView-only.
- ORDERING FIX: "Stability and limits" (the closing section) had three
  wave-3 sections appended AFTER it; it now closes the document again.
  No content was rewritten in peer sections — pure move.

## Open/deferred

- `examples/feed.rs` (FIXNET's live-data lane) untouched; it renders
  plain-text items, no markdown surface to adopt.
- voice_mock stays out of the capture pipeline: meter ballistics are
  wall-clock-driven — not clockless-deterministic without a fixed-clock
  knob it doesn't have. Named, not attempted.
- `FeedBlock` stays exhaustive (0.3 budget unchanged); the adoption
  needed no public API change at all.

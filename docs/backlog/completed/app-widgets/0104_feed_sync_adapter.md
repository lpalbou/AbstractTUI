# 0104 — `FeedState::sync`: a diffing adapter from a slice source of truth

- Status: Completed (content wave, CONTENT2 seat — 2026-07-23)
- Track: app-widgets (band 0100–0190; extends the 0100 Feed trunk)
- Origin: FIELD study 2 consumer-tensions report
  (`reviews/study2/field-consumer-tensions.md` §3.6, top-tension #3:
  "Feed's sync burden belongs in the engine"). Filed by the convergence
  pass (cycle 2, 2026-07-22).
- Depends on: none (additive API over the existing `FeedState`).
  Coordinates with 0102/0660/0280's block-vocabulary pass only in that
  the render callback produces `FeedItem`s — no ordering constraint.
- Promotion trigger: the second fold-shaped consumer (any app whose
  transcript derives from a `Signal<Vec<T>>`/fold state rather than an
  append-only event stream) — the first already carries the machinery.

## Problem

Feed's API is imperative push/update over keyed items (`FeedState::
push`/`update`/`push_stream`, src/widgets/feed.rs:190-236) — appending
is O(1) and that is the right hot path. But every consumer whose source
of truth is a FOLD (a `Vec` of domain items recomputed by events) must
build the bridge itself: which keys changed, whether a change is an
append (fast path) or demands a rebuild, and whether a hidden item
broke the append-only tail assumption.

Consumer evidence (field-consumer-tensions.md §3.6, consumer paths):
~180 lines whose only job is that bridge — `wire_feed`
(transcript_view.rs:502-584), an FNV fingerprint per item
(transcript_view.rs:389-483), a visibility predicate
(transcript_view.rs:591-599) — plus a correctness obligation the
engine's design created and the app must uphold by hand: "feed order is
PUSH order, so a key may only be appended when it lands at the tail;
mid-list visibility changes force the rebuild path"
(transcript_view.rs:13-16), pinned app-side by a test that the hide
predicate stays byte-exact with the renderer (transcript_view.rs:
726-784). The report's verdict: transferable obligation "every
fold-shaped consumer will re-implement slightly wrong."

## What we want to do

An adapter that owns the diff so the fast path is the default path —
indicative shape:

```rust
feed.sync(cx, items_signal, SyncSpec {
    key: |item| item.id.clone(),          // identity
    fingerprint: |item| item.rev,          // cheap change detection
    visible: |item| !item.hidden,          // optional filter
    render: |item| FeedItem::from(item),   // build blocks on change
});
```

- Appended-at-tail keys take the O(1) push path; changed fingerprints
  take `update`; anything violating push-order (mid-list insert,
  visibility flip, removal) takes the rebuild path — INSIDE the engine,
  with the rebuild-on-shrink policy documented once.
- The visibility predicate lives in the spec, so the "mirror predicate
  must stay byte-exact with the renderer" obligation dissolves (one
  closure, one truth).
- Diff cost bounded: fingerprints are the only per-item work on
  unchanged tails (the consumer's amortization concern —
  their truncation work exists to keep this cheap, transcript.rs:
  312-340 per the report).

## Non-goals

Replacing the imperative API (push/update stay the hot path and the
streaming lane); generalized list-diffing (keys are identities, order
is source order — no LCS); reactive per-item projections (the render
closure runs on change, not per frame).

## Validation

- Scripted folds: append-only sequence never rebuilds (counter pin);
  mid-list change rebuilds exactly once; visibility flip mid-list
  rebuilds; tail-only visibility flip appends.
- Parity: a sync-driven feed and a hand-pushed feed render identical
  cells for the same end state (golden).
- The consumer-deletion acceptance: wire_feed + fingerprint + mirror
  test (~180 lines, paths above) become one `sync` call per the
  report's estimate.

## Completion report

- Final path: docs/backlog/completed/app-widgets/0104_feed_sync_adapter.md
- Date: 2026-07-23
- Shipped shape (src/widgets/feed_sync.rs, `#[path]` sibling of feed):
  `feed.sync(cx, items_signal, SyncSpec::new(key, fingerprint, render)
  .visible(filter)) -> Effect` — builder constructors instead of the
  item's struct-literal sketch (an optional closure field in a literal
  needs an unnameable `None` type ascription; `SyncSpec::new` + the
  `.visible` builder is the house style and reads the same).
  `fingerprint` is generic over any `PartialEq` value (a revision
  counter, a content hash, a tuple) — the honest general shape: the
  engine never dictates hashing. Returns the labeled `Effect`
  (`"feed.sync"`) so callers can stop the bridge early.
- Diff semantics (module-doc'd, the once-documented rebuild policy):
  pass 1 verifies the shown (key, fingerprint) sequence is an ORDERED
  PREFIX of the new visible sequence (keys are the only per-item probe);
  holding, pass 2 updates changed fingerprints in place
  (`FeedState::update`) and pushes tail keys (O(1)); ANY violation —
  shrink, removal, reorder, mid-list insert, mid-list visibility flip —
  rebuilds whole (`clear()` + re-push), because feed order is push
  order and the feed is append-only. Tail-only visibility flips are
  appends by construction. Contracts documented: one writer per synced
  feed; closures must not read the source signal reactively (the
  `Signal::update` rule); keys unique per visible snapshot.
- The mirror-predicate obligation dissolved: `visible` lives in the
  spec, so the consumer's "hide predicate must stay byte-exact with
  the renderer" test has nothing left to drift from.
- Tests (src/widgets/feed_sync_tests.rs):
  `append_only_folds_take_the_push_path_and_never_rebuild` (render
  counter pin: 10 appends = exactly 10 renders),
  `fingerprint_change_updates_in_place_without_rebuild`,
  `mid_list_insert_rebuilds_exactly_once` (2+3 renders, counted),
  `visibility_flips_mid_list_rebuild_and_tail_flips_append`,
  `shrink_and_reorder_take_the_rebuild_path`, and the parity bar —
  `parity_reorder_midlist_update_burst_append_full_replace` +
  `hidden_items_never_reach_the_feed_and_parity_holds_with_filter`
  (sync-driven vs hand-pushed feeds compared cell-by-cell for
  reorder, mid-list update, burst append, full replace, filtered
  mid-list).
- Measured (release, `perf_sync_burst_1k_into_10k`): a 1k burst append
  into a feed already mirroring 10k items folds in ~1.0 ms (11k
  fingerprint walk + exactly 1k renders/pushes — the counter asserts
  no rebuild); the whole scenario incl. the 10k initial fill and a
  windowed draw settles in ~9.7 ms median.

## Post-completion fixes (wave-3 cycle-3 close, CLOSER — 2026-07-23)

Cycle-2 review findings C-1/C-2/C-3 (`reviews/wave3/review-cycle2.md`):

- **C-1 (P2) — one-writer self-heal**: the one-writer contract was
  documented but unguarded, and violations were silently PERMANENT (a
  stray manual `push` survived every fast-path drain; a manually-pushed
  key the source appended later replaced in place at the old index, so
  feed order diverged from source order with `shown` claiming they
  agree). Fixed with the review's priced design: `FeedInner` carries a
  `mutations: u64` counter bumped by every ITEM mutation
  (push/update/push_stream/stream_append/stream_finish/clear — never
  theme rebinds or geometry publishes, which would false-positive);
  the bridge records the counter after each drain's own writes and
  takes the REBUILD path when it finds the counter moved (one u64
  compare per drain, no public API change). Self-heal semantics
  documented in the `sync` rustdoc. Tests:
  `foreign_push_between_drains_self_heals_with_a_rebuild` (stray
  evicted at the NEXT drain, exactly one rebuild, fast paths resume),
  `foreign_push_of_a_future_source_key_heals_to_source_order` (the
  order-divergence worst case), and the review probe flipped
  deliberately (`one_writer_violation_self_heals_at_the_next_drain`).
- **C-2 (P3) — NaN fingerprints**: doc-closed. `SyncSpec` rustdoc now
  states float fingerprints must compare by bits
  (`f32::to_bits`/`f64::to_bits` or a bits-comparing newtype) because
  IEEE `NaN != NaN` makes a NaN fingerprint re-render every drain
  (pixels stay correct; cost only). Judgment recorded: no engine-side
  code fix exists for a user-supplied `PartialEq` short of requiring
  `Eq` (which would reject float fingerprints wholesale), and a
  debug-mode diagnostic has no honest outlet (the engine never prints;
  no notices channel reaches `FeedState`; a `debug_assert` would turn
  a documented degradation into a crash and contradict the review's
  own pinned probe). The cost probe
  (`nan_fingerprint_rerenders_every_drain_but_stays_correct`) stands.
- **C-3 (P3) — rebuild-storm cost named**: the `sync` rustdoc now
  carries the cost sentence — a rebuild re-renders EVERY visible item,
  so a source that reorders on every change (most-recent-first sort,
  live-resorted leaderboard) pays O(visible) renders per drain,
  forever; sync a stable order and sort at render time, or accept the
  cost knowingly. The api.md §"Feed — syncing" twin sentence is handed
  to DOCS (this cycle's api.md owner) via
  `reviews/wave3/closer-handoff.md`.

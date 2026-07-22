# 0104 — `FeedState::sync`: a diffing adapter from a slice source of truth

- Status: proposed
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

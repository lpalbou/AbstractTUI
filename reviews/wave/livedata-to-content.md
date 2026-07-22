# LIVEDATA → CONTENT: Feed widget pairing (cycle-2 switch)

## Cycle-2 status update (appended; superseded same-cycle — see below)

The pairing is EXECUTED end to end, including your mid-cycle landings:
`examples/feed.rs` renders through `Feed` (slot-keyed replace,
`slot-0..slot-N` — your "keyed replace already works" line; bounded end
to end today) inside `Scroll` with **your `follow_tail` over the
measured extent** — my interim `content_size` hint and the End-to-tail
idiom were deleted the same cycle they were written. The endurance soak
(`tests/wave_livedata_soak.rs::soak_60_virtual_seconds_bursty_producer_through_feed`)
drives 27k events through FeedState/Feed/follow_tail with a dead-flat
allocation plateau AND asserts the newest event is on screen EVERY
cycle — including the full-window steady state where a drain replaces
content without changing the extent (the edge I flagged below): your
implementation holds it, and it is now pinned from my side too.

Still open, no urgency: `FeedState::clear()` — my window-sync body
becomes clear-and-repush when it lands (one marked spot in
examples/feed.rs + the docs snippet note). The slot-keyed form costs
O(window) pushes with O(window²) prefix churn per drain (flat in the
soak at 400; only matters for ≥10k windows).

Original cycle-1 note below, kept for the record.

`examples/feed.rs` (mine) renders the live-data lane with today's
Scroll + a hand-rolled follow-tail; its header promises the cycle-2
switch to your Feed widget. What the switch needs from Feed's API to be
a deletion of my hand-rolled parts (no adapters):

1. **Data in**: rebuild-per-change from a `Signal<Vec<T>>` read inside a
   `dyn_view` is what the bounded lane feeds naturally (one drain = one
   signal write = one rebuild = one frame). If Feed takes `Vec<String>`
   (List-style) or an iterator of rows, the pairing is direct. The
   window is already bounded upstream (`bounded_source` capacity), so
   Feed never sees unbounded growth.
2. **Follow-tail**: the idiom worth packaging — external
   `offset` signal (survives rebuilds), stick-to-bottom while at
   bottom, release on user scroll-up, re-arm at bottom. If Feed owns
   this internally, please expose `follow: Signal<bool>` (or a
   read/write pair) so a status line can render "following / scrolled"
   and a key can re-arm it — that is the part users see.
3. **Resize**: my version re-pins the tail on viewport change (the
   follow effect tracks `use_viewport`). Worth keeping as Feed-internal
   behavior.

Nothing here blocks your design — I adapt to whatever ships; this is
just the integration surface my example exercises today, so you know
what falls out for free. Sender/stats stay on my side of the seam
(`bounded_source` in `src/reactive/ingest.rs`; `IngestStats` is
`Copy + PartialEq` if you want a built-in dropped-badge).

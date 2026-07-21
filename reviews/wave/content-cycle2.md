# CONTENT cycle 2 — Feed widget (backlog 0100)

## Shipped

`widgets::Feed` + `widgets::FeedState` (src/widgets/feed.rs; tests in
feed_tests.rs). Re-exported from `widgets`; prelude untouched (not
mine).

```rust
let feed = FeedState::new(cx);              // cloneable handle
feed.push("m1", FeedItem::markdown("**hi**"));
feed.push("m2", FeedItem::text("log line"));
feed.push("m3", FeedItem::code("rust", "fn main() {}"));
feed.push("m4", FeedItem::new().block(FeedBlock::Custom(CustomBlock::new(h, draw))));
feed.update("m2", item);                    // keyed replace
feed.push_stream("ans");                    // streaming markdown item
feed.stream_append("ans", token);           // O(open block) per delta
feed.stream_finish("ans");
feed.total_rows() -> Signal<i32>;           // reactive content extent
Feed::new(&feed).gap(1).view(cx)            // content-sized (Scroll-ready)
Feed::new(&feed).layout(style).view(cx)     // fixed-box mode (clips)
```

## Design facts

- **Mutation model**: the app mutates the handle; the widget re-renders
  ONE dyn region keyed on a version signal. Append = typeset one item +
  extend prefix sums + one damaged region. No per-append rebuild of the
  item vector (the List-style rebuild would be O(n) per append).
- **Typeset sharing**: `markdown.rs` grew a crate-internal
  `BlockTypesetter` (extracted from `MarkdownView::layout_rows`, its
  chrome tests unchanged) — Feed items and MarkdownView cannot drift.
- **Streaming tail**: items wrap `md::StreamSession`; closed blocks
  typeset once into a frozen segment, only the open tail re-typesets
  per delta. Pixel-parity with a static item of the same source is
  test-pinned.
- **Geometry**: draw discovers width and re-typesets synchronously (a
  pure cache fill, the MarkdownView recipe); the reactive extent
  (`total_rows`, also the element's height style) never writes from
  draw (RT1-2) — appends at a known width sync it synchronously, width
  changes sync via a latched `reactive::after(0)` fixup one turn later.
- **Theme rebind**: `element()` compares tokens; a change re-typesets
  all and re-parses stream sessions once from retained raw source
  (parse-time inline styles carry token colors).

## Validation (8 new tests, 948 lib green, clippy 0, fmt clean)

- `markdown_text_and_code_items_render_with_gap_rows`
- `duplicate_key_replaces_and_update_reflows_later_items`
- `streamed_item_matches_static_item_pixels` — 3-byte chunks == batch
- `stream_appends_typeset_only_the_open_block` — 60 tokens behind 40
  closed blocks re-typeset ≤ 60 blocks (freeze contract, counter-based)
- `feed_10k_items_draws_only_the_window` — CountingCanvas, cost ≤
  viewport×3 (same budget as List's 10k pin)
- `width_change_retypesets_and_resyncs_the_extent` (viewport resize)
- `custom_blocks_occupy_their_height_and_draw` (deferred outside the
  state borrow — user paint code can never deadlock the RefCell)
- `appends_at_known_width_sync_the_extent_immediately` (the
  single-frame pin path cycle 3's follow-tail rides)

## Drift vs backlog 0100

- "Per-item typeset cache … cached per width": rows re-typeset on width
  change but are NOT kept per multiple widths (one width at a time —
  a feed lives in one pane; caching per width would hoard memory).
- Optional selection by key (item 6) deferred to a later cycle — ports
  don't need it for v1; windowing/streaming were the P0 halves.
- Item heights are eager for ALL items (prefix sums need them); rows
  are also eager. Memory at 100k small items measured in cycle 3's
  perf test; if heavy, the fix is height-only + windowed row
  materialization (internal, non-breaking).

## For LIVEDATA (reply to livedata-to-content.md)

- Ask 2 (follow signal app-visible both ways) is exactly cycle 3's
  `Scroll::follow_tail(Signal<bool>)` — pinned-state chrome and re-arm
  keys fall out.
- Ask 3 (resize re-pin) will be solver-structural (bottom-anchored
  content while pinned), no effect needed.
- Ask 1 (rebuild-per-drain from `Signal<Vec<T>>`): your bounded lane
  DROPS OLDEST, and Feed is append-only — cycle 3 adds
  `FeedState::clear()` so a drain can rebuild its bounded window
  (O(window), bounded by your capacity). Keyed replace already works.

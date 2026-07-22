# CONTENT cycle 3 — Scroll follow-tail + measured extent (backlog 0130), wave hardening

## Shipped

`Scroll` (src/widgets/scroll.rs; tests split to scroll_tests.rs) gained
the 0130 pair; `FeedState::clear()` landed (the cycle-2 promise);
`examples/transcript.rs` is the wave proof demo; `tests/wave_content.rs`
is the acceptance + measurement suite; docs/api.md gained Feed /
follow-tail / StreamSession sections; 0100/0110/0130 moved to
docs/backlog/completed/app-widgets/ with completion reports. File-size
discipline: feed.rs (801 lines) split — entry storage + typesetting now
live in feed_typeset.rs (a private child module, the md_stream
pattern), and the widgets color-lint list gained the sibling (the
count-proxy check in mod.rs became a membership check so private
shipped siblings stay covered).

```rust
let follow = cx.signal(true);                  // app-visible both ways
Scroll::new(Feed::new(&feed).view(cx))         // extent MEASURED (no hint)
    .follow_tail(follow)                       // pinned until scrolled up
    .view(cx);
// content_size(w, h) still exists and WINS when given (hint mode
// byte-identical to v1); follow works in both modes.
```

## Design facts

- **Measured extent**: no hint → the wrapper's scroll axis is `Auto`;
  the solver answers intrinsics per solve (`place_absolute` →
  `intrinsic_size`; standalone face: `layout::measure`). Feed answers
  O(1) via its reactive height style; text trees answer wrap-aware at
  viewport width. The solved size is read back by a draw-probe +
  latched `after(0)` into an extent signal (the Feed width-fixup
  pattern) feeding clamps, thumb, and the pin. A bare `MarkdownView`
  has no intrinsic height — wrap it in a one-item Feed or keep the
  hint (documented in the module doc, which replaces the v1 honesty
  note).
- **Follow-tail**: a pin effect keeps `offset = max(0, content − view)`
  while the signal is true (extent + viewport probes, both reactive);
  while ACTUALLY scrolled the wrapper anchors `bottom: 0` instead of
  top-offsetting, so the solver holds the tail pixel-exact through
  appends/shrinks/resizes with zero extent knowledge — and the wrapper
  can never scroll out of the clip and starve the probe (a
  clear()-rebuild deadlocked exactly there in development; the wave
  test pins it).
- **Disengage/re-arm**: only user gestures derive the signal
  (`new_offset >= max_offset` after wheel/keys/drag) — re-arm happens
  ON the bottom edge only; programmatic offset writes never disengage;
  `follow.set(true)` is the jump-to-latest affordance.
- **0240 follow-up #1 applied**: the default Scroll layout is now
  `grow(1.0).basis(Cells(0))` — a content-derived basis let long
  content starve fixed siblings to zero rows (the modal-overflow
  class). Explicit `.layout(..)` callers are untouched. NOTE: 0240's
  report points at reviews/wave/stability-to-content.md for the full
  spec list, but that file was never written; follow-ups #2 (one-row
  controls `shrink(0.0)`), #3 (zero-collapse debug notice), #4 (docs
  recipe) remain open.
- **CLOSURE UPDATE (final cycle): #2/#3/#4 landed too.** #2: default
  layouts of button/checkbox/radio/input/progress/badge/spinner/
  separator + the tabs bar gained `shrink(0.0)` (caller layouts
  untouched); pinned by `one_row_controls_survive_overflow_pressure`
  in tests/wave_content.rs (verified failing against the old
  defaults). #3: debug-build zero-collapse diagnostic — the solver
  reports (once per node, bounded, stderr + drainable via
  `LayoutTree::take_collapse_notices`) any child that DECLARED a fixed
  `Cells` main-axis size and got crushed to zero; explicit min (even
  `min_h(0)`, the 0240 opt-out) or percent/intrinsic sizing never
  watches; release builds record nothing. Pinned by
  `zero_collapse_emits_a_debug_notice_once_with_opt_outs` in
  src/layout/mod.rs. #4: docs/api.md "Modal content that can
  overflow" recipe beside the Scroll follow-tail section.

## Validation (whole tree green: 958 lib / all integration / 40 doctests; clippy 0; fmt clean)

Unit (scroll_tests.rs): the four v1 tests unchanged, plus
`measured_extent_scrolls_to_the_true_last_row_without_a_hint`,
`content_size_hint_wins_over_measurement`,
`default_layout_takes_leftover_not_content_basis`,
`follow_tail_pins_growth_disengages_on_wheel_and_rearms_at_bottom`,
`app_can_force_follow_to_jump_to_latest`,
`follow_tail_repins_across_resize`.

Wave (tests/wave_content.rs, real Driver/CaptureTerm loop, real SGR
wheel bytes, scripted resize):
`follow_tail_acceptance_appends_wheel_and_resize`,
`streaming_append_damage_stays_inside_the_pane_and_bytes_stay_bounded`,
`feed_10k_inside_measured_scroll_draws_only_a_screenful`,
`tail_tokens_behind_closed_blocks_typeset_only_the_open_block`,
`measure_100k_appends_and_full_feed_repaint`,
`clear_rebuilds_a_bounded_window_and_follow_repins`.
`cargo test --test alloc_budget -- --test-threads=1` stays green.

## Measured (release; debug in parentheses)

- 100k batched appends: 632 ms = 6.3 µs/item (debug 4.84 s, 48.4 µs/item);
  1k unbatched: 6.6 µs/item (56.8 µs/item).
- Steady token streaming through the full stack: ~104 bytes/token
  average, 1,000 max, chrome rows byte-identical, zero unmodeled bytes.
- 10k items pinned in a measured Scroll: one paint = 171 puts
  (budget 900); full windowed repaint over a 101k-item feed: 42 µs.

## For LIVEDATA

Everything docs/live-data.md already assumes now exists with those
exact names: `Feed::new(&feed).gap(0).view(cx)`, `follow_tail(follow)`,
measured extent (no `content_size` call), `FeedState::clear()` for the
clear-and-repush drain sync. `clear_rebuilds_a_bounded_window_and_follow_repins`
in tests/wave_content.rs is your drain shape, test-pinned. One baseline
observation from this seat: `wave_livedata.rs::soak_60_virtual_seconds_bursty_producer_through_feed`
failed once under full-suite parallel load (cadence 59 != 60) and
passes 3/3 in isolation — looks timing-sensitive under load; your call.

## Known-open in my lane

- Feed selection by key (0100 item 6) — deferred, neither port needs it.
- Feed rows are eager per item (one width at a time); 100k numbers
  above say it holds; height-only materialization is the internal fix
  if memory ever measures hot.
- Offset-signal sync while pinned lags the pixels by one turn (the
  bottom-anchored wrapper is exact; the signal catches up via the pin
  effect) — documented in scroll.rs; invisible in every test and demo.

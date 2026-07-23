# Wave 12 — perf budget status (CODE)

Date: 2026-07-25. Tree: v0.2.18 + wave-11 fixes, BEFORE the wave-12
file splits (splits are behavior-neutral; numbers were taken on the
pristine baseline so any failure would have attributed cleanly to the
week's draw-path changes — fusion skip, modal re-clamp, click
synthesis).

Command (serial, release, quiet host — the doctrine header of each
suite):

```sh
cargo test --release --test perf_budgets --test perf_app_surfaces \
  -- --ignored --test-threads=1 --nocapture
```

Raw log: `untracked/wave12_perf_baseline.log`.

## Verdict

**All 19 ignored perf tests PASS** (12 in `perf_budgets`, 7 in
`perf_app_surfaces` — the wave brief said 3; the suite has grown).
No wave-11/12 regression. Every byte ratchet landed exactly on its
recorded baseline (deterministic byte counts — zero emission drift
from this week's waves).

## perf_budgets (timings: median over runs x iters)

| test | measured | budget | verdict |
|---|---|---|---|
| perf_diff_present_200x60_full_change | 167.4 µs | 2 ms | PASS (12x headroom) |
| perf_parser_1mb_soup | 5.34 ms | 50 ms | PASS |
| perf_pool_churn_100k_unique_clusters | behavior: cap 4096 entries, 287,712 refusals, no growth after cap (churn 178.1 ms advisory) | behavioral | PASS |
| perf_link_churn_past_u16_space | behavior: 4,465 refusals past u16, early links resolve, no wrap | behavioral | PASS |
| perf_frame_with_active_cell_shader_200x60 | 432.5 µs | 3 ms | PASS |
| perf_splash_fallback_frame_100x30 | 35.2 µs | 2 ms | PASS |
| perf_brandmark_3d_frame_100x30 | 149.7 µs | 8 ms | PASS |
| perf_keystroke_to_frame_through_driver | 136.9 µs | 3 ms | PASS |
| perf_vt_model_referee_overhead | 460.2 µs | 3 ms | PASS |
| perf_grid_solve_large_tree | 37.3 µs | 3 ms | PASS |
| perf_markdown_parse_large_doc | 307.8 µs | 20 ms | PASS |
| perf_richtext_wrap_large_doc | 3.25 ms | 20 ms | PASS |

## perf_app_surfaces (time + byte ratchets; ratchets assert in every profile)

| test | time measured | time budget | bytes measured | byte budget | verdict |
|---|---|---|---|---|---|
| perf_feed_streaming_token_frame_90x30 | 272.7 µs | 3 ms | 73 B/frame median (max 427; first paint 9,670) | 110 | PASS |
| perf_select_popup_open_close_100x30 | 51.6 µs/cycle | 6 ms | open 301 B / close 254 B (first paint 3,720) | 452 / 381 | PASS |
| perf_selection_drag_full_screen_200x60 | 826.6 µs | 5 ms | 260 B/frame | 390 | PASS |
| perf_textarea_keystroke_with_completion_open_90x30 | 100.2 µs | 3 ms | 465 B/frame | 698 | PASS |
| perf_codeview_diff_scroll_100x40 | 373.7 µs | 5 ms | 255 B/frame (shift-detected) | 383 | PASS |
| perf_feed_scroll_with_parked_protocol_image_90x30 | bytes-only test | — | shift path 172 B / guard(plain-diff) path 1,758 B, ratio 10.2x (first paint 3,006) | 258 / 2,637 | PASS |
| perf_startup_time_to_first_frame | release warm: hello 43.0 ms, dashboard 51.5 ms (cold 280.2 / 304.0; debug warm 45.3 / 51.9) | 1,500 ms | — | — | PASS |

## Notes

- The byte medians (73/301/254/260/465/255/172/1758) are IDENTICAL to
  the baselines recorded in the test comments on 2026-07-23 —
  deterministic emission, so the damage contract's proportionality
  claim holds byte-for-byte through the week's changes.
- Timing headroom is wide everywhere (worst ratio: richtext wrap at
  ~6x). No budget is near flapping on a quiet host.
- The startup test builds debug+release examples inside the test; its
  numbers include a first-exec cold pass and a warm pass, warm is the
  asserted shape.

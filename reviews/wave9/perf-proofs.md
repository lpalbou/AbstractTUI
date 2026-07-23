# Wave 9 — extension performance proofs (CANVAS seat)

Measured 2026-07-24 on the dev machine (Apple Silicon, unoptimized
`test` profile — the same posture as the cycle-1 layout numbers).
Every number below PRINTS from a test that asserts a generous bound,
so regressions surface in CI while the report carries reality:
`extensions/graph/tests/perf_view.rs` (all three), plus the standing
pins cited at the bottom.

## (a) Layout scale — `layered()` + `force(budget 64)`

| Nodes | Edges | layered | force(64) |
| --- | --- | --- | --- |
| 100 | 132 | **1.4 ms** | **1.3 ms** |
| 500 | 674 | **9.9 ms** | **29.9 ms** |

(Consistent with the cycle-1 numbers from the layout half: 500 nodes /
718 edges — layered 13.9 ms, force 29.8 ms on a denser fixture.
Asserted bounds: 10 s / 20 s.)

## (b) Render cost — GraphView at 80x24, 30-node workflow

| Frame | Bytes | Fraction |
| --- | --- | --- |
| Full repaint (`request_full_redraw`) | **6,971 B** | 100% |
| One live badge change (damage frame) | **51 B** | **0.7%** |

Asserted: the damage frame is under a quarter of a full frame; the
measured reality is two orders of magnitude below the bound. The
damage path is the per-card `dyn_view` design: a badge signal change
re-renders exactly the badge's card region
(`render_full_frame_vs_badge_damage_bytes`).

## (c) Edge-pass allocation — edge-count-INDEPENDENT

| Edges | Allocations per repaint |
| --- | --- |
| 5 | **23** |
| 20 (incl. bowed parallel fans) | **23** |

Identical counts: the edge pass allocates a bounded constant (two dot
grids + per-card text shaping) — no per-edge, no per-dot heap traffic;
stroke planning is a build-time act, never a draw-time one
(`edge_pass_allocation_is_edge_count_independent`, counting-allocator
pattern from tests/alloc_budget.rs confined to the test binary).

## (d) Standing pins, cited

- **Zero idle**: a parked GraphView emits zero bytes and renders
  nothing over 16 idle turns —
  `extensions/graph/tests/view_interact.rs::parked_graph_view_idles_at_zero`;
  the fully composed scene (PageHost + Drawer + GraphView +
  MermaidView) repeats the pin at the end of
  `tests/wave_extensions_accept.rs::pipeline_monitor_scene_end_to_end`.
- **Stroke path zero-alloc** (core 0420): 8 frames of clear + lines +
  polylines + both beziers + a full-turn arc + far-off-grid clip +
  blit + both fills = 0 allocs / 0 reallocs —
  `tests/alloc_budget.rs::dot_canvas_stroke_and_blit_paths_allocate_nothing`.

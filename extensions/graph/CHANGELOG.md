# Changelog — abstracttui-graph

All notable changes to this crate are documented here (family crates
own their changelogs; core's CHANGELOG covers the engine). The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
SemVer.

## [0.1.0] - 2026-07-24

First release, published alongside `abstracttui` 0.2.13 (ADR-0004
family order: core first, family the same day). 79 crate tests green
at release. Measured on the dev machine (unoptimized test profile):
500 nodes / 718 edges lay out in 13.9 ms (`layered`) and 29.8 ms
(`force`, budget 64); a 30-node GraphView full repaint is ~7 KB and a
single badge-change damage frame 51 B (0.7%); the edge pass allocates
independently of edge count (`tests/perf_view.rs`).

### Added

- layout (cycle 1): the `GraphDesc -> Layout` contract — `layered()`
  (sugiyama-lite: longest-path ranks, bounded median crossing
  reduction, PAVA-packed aligned-median coordinates, waypoints
  through rank gaps, TD/LR/BT/RL), `force()` (bounded seeded
  alpha-cooled placement, settle freeze, optional rank bias),
  `grid()` (labeled fallback). Honesty markers: per-edge `broken` +
  `Layout::fallback`. Deterministic and bounded throughout;
  `dump::ascii` debugging aid.
- view (cycle 2): `GraphView` — read-only rendering of a `Layout`
  over the core canvas layer (0420). Node cards (title on the border,
  kind-tinted left accent, reactive badge slot, selection restyle),
  edges as canvas strokes (midpoint-smoothed beziers through
  waypoints, arrowheads at the target border with degenerate-segment
  fallback, dotted/thick styles from `EdgeDesc::style`, cycle-broken
  edges dotted in their own ink, canonical-frame bowing so
  parallel/opposite 2-point edges stay legible), the fallback label
  as a non-scrolling notice line, pan via `Scroll` (layout bounds =
  content size, offsets bindable), one-tab-stop keyboard vocabulary
  (Enter selects/presses, arrows walk nodes aligned-first or pan when
  nothing is selected, Escape deselects), `on_node_press`
  (disposal-safe), hover tooltips via the core anchored `Tooltip`,
  caller-resolved `GraphStyle` (`from_tokens` derivation). Zero-idle
  pinned through the real Driver; examples `workflow` + `network`.

- view (cycle 3): the `open` stroke-style hint — an edge whose
  `EdgeDesc::style` contains `open` keeps its stroke and skips the
  arrowhead (the undirected reading; mermaid's `---` compiles to it).
  Combines with dotted/thick. Pinned by
  `view_attack_cycle3.rs::open_style_edges_render_without_an_arrowhead`
  and mermaid's end-to-end twin.

### Fixed

- layout: BT/RL waypoint mirroring was off by one cell along the flow
  axis (`map_point` mirrored cell indices as `-f`; cells are
  half-open intervals, so the mirror is `-(f+1)` — exactly how
  `map_rect` mirrors rects). Source anchors landed INSIDE their card
  (painted over) and target anchors sat one cell off. Found by the
  view's BT arrowhead golden; pinned by
  `tests/view_attack_list.rs::bt_rl_waypoints_mirror_like_rects_and_stay_out_of_cards`.
- view (cycle-3 attack battery, all failing-first): bowed parallel
  edges on short chords clamped their curve control into the dot grid
  (outer bows of 3+ fans rendered as clipped stubs before); edge
  labels compute a true geometric midpoint (2-point edges printed
  their label into the TARGET CARD before) and are skipped at plan
  time when their cell run would overprint any card; arrow-key
  selection scrolls the target into view under padded roots (a
  paint-time viewport probe replaced the widget-rect approximation);
  card presses fire on mouse-release-inside instead of mouse-down
  (the engine Button convention — a press that opened a modal used to
  strand pointer capture and re-press the card on every later click).
  Dispositions + citations: `reviews/wave9/canvas-final-attack.md`.

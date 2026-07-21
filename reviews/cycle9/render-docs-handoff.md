# RENDER — docs-cycle handoff (cycle 9)

Doc-ready prose for the guide. Sources of truth: docs/design/render.md
(§2.10 is the pipeline page, final-polished this cycle), the rustdoc on
every public item (missing-docs count is ZERO in render/text/anim), and
the 7 compiling doctests (they run in CI — lift them verbatim; they
cannot rot).

---

## 1. The render pipeline (guide chapter; lift render.md §2.10)

One frame, four moves. You own the buffers; the engine owns the rules.

```text
1. DRAW    widgets/you write into layer Surfaces   (surfaces record damage)
2. FLATTEN Compositor::flatten(&mut frame, &mut layers) -> &[Rect]
3. DIFF    FrameDiff::compute(&prev, &frame, damage)    -> &[Run]
4. PRESENT Presenter::emit(runs, &frame, &caps, &mut out); flush ONCE;
           prev.blit(&frame, ..)
```

The compiling version of this diagram is the `render` module doc
example (`src/render/mod.rs`) — use THAT in the guide, it is
CI-checked. Key sentences the guide must keep:

- **Damage is automatic.** Every draw records its own damage;
  `add_damage` exists only for out-of-band cell mutation. Damage
  over-approximates honestly: the diff re-checks equality, so stale
  damage costs microseconds, never wrong pixels.
- **One flush per frame.** The presenter only appends to your buffer;
  you write it to the tty exactly once (partial flushes tear frames;
  the DEC 2026 sync bracket must arrive whole).
- **Foreign bytes go through `Presenter::external_write`** (image
  protocols, bells): flush-state → absolute CUP → payload → invalidate.
  If something touched the terminal without telling you:
  `presenter.invalidate()`.

## 2. Style is a PATCH (the most-misunderstood concept — own section)

A `Style` is a delta, not an appearance. `fg`/`bg` at `None` keep what
the target cell already has (text over a filled panel keeps the panel
ground); attrs are add/remove SETS (`.bold()` layers onto existing
content). `Style::absolute()` is the opt-out (remove everything first).
Two consequences worth calling out in the guide:

- `merge` is sequential application: `a.merge(b)` == apply a, then b;
  b's opinions win. (Doctest on the `Style` type shows it.)
- The hyperlink id is the ONE non-patch field: `apply` always
  overwrites it — inheriting a stale link under a fresh label would be
  a correctness hazard.
- Markdown corollary: `MdStyles::base` must stay fg-less or block
  recoloring (blockquote dims) silently breaks — doc'd on the field and
  on `MdStyles::with_ink` (the theme mapping constructor).

Terse spellings (freeze surface): `Style::new().fg(ink).bold()` — the
six common attrs have one-word builders; rare ones use `.attrs(...)`.

## 3. The damage promise (product guarantee — print it as one)

> **An idle AbstractTUI app costs zero: zero bytes written, zero heap
> allocations, zero shader work.**

Proof tests the guide can cite by name (all in-tree, all green):

| Claim | Test |
|---|---|
| idle frame emits zero bytes | `render::present::tests::zero_runs_zero_bytes` + `render::pipeline_tests::full_pipeline_small_damage_small_bytes` (frame 3) |
| no-change frame allocates nothing | `alloc_budget::presenter_no_change_frame_emits_and_allocates_nothing` |
| steady-state diff+present allocates nothing | `alloc_budget::diff_present_steady_state_allocates_nothing` |
| static shader on idle layer costs zero shade calls | `render::compositor::tests::shader_runs_only_for_damaged_cells_and_never_when_static` |
| idle flatten is free | `Compositor` doctest (second flatten returns empty damage) |

## 4. Shader billing rules (effects chapter)

- A shader runs ONLY where damage exists. Static shader = paid once at
  install, never again.
- An animated shader is an ANIMATION: advancing `Layer::set_shader_t`
  is the tick; it damages what `CellShader::changed_region(t0, t1,
  bounds)` declares (default: whole layer) and the app requests the
  next frame like any tween.
- Free ticks: `Vignette` (ignores t), settled reveals
  (`ScanlineFade`/`GradientReveal`/`Dissolve` past their end), equal
  wave samples (`Pulse`/`HueDrift`). Banded effects (`Sweep`, moving
  reveals) damage thin slabs.
- The hint contract is STABILITY outside the rect (bit-identical output
  at both clocks), property-tested per built-in
  (`changed_region_hints_are_honest_for_every_builtin`). Third-party
  shaders inherit the safe default (`None` = everything may change).
- Determinism: shaders are pure in `(x, y, t, cell)` + construction
  params. No libm anywhere in built-ins — REDTEAM byte-goldens them.

## 5. Scroll-optimization semantics (advanced chapter or appendix)

- ON by default (`PresenterOpts::default()`); the byte-win guard lives
  in DETECTION (full-width band ≥ 8 rows, ≥ 4 made diff-clean), so
  enabling can only reduce bytes.
- `FrameDiff::compute_scrolled` returns a `ScrolledRuns` TOKEN: shift +
  runs that are only valid AFTER the shift executes. The pairing is
  structural — plain `emit` cannot accept a token, `emit_scrolled`
  cannot take loose shift-relative runs; the cycle-4 wrong-pixels
  hazard does not type-check.
- Emission: DECSTBM set + SU/SD + margin reset, SGR 0 prelude, cursor
  invalidated after (absolute re-sync). Replayed byte-exactly on the VT
  referee incl. wide pairs at band edges
  (`wide_pairs_at_band_edges_survive_scroll_optimization`).
- Wins on the published workloads: log-append/list-scroll/banded run
  far under the plain-path baselines (cycle-5 filing: 2,318 / 1,607 /
  1,648 B/frame).

## 6. Perf envelope table (RENDER-owned rows; release, isolated,
`--test-threads=1` — parallel in-binary numbers measure the scheduler,
not the code)

| Path | Median | Budget | Test |
|---|---|---|---|
| diff+present 200x60 full-change | ~450 µs | 5 ms | `perf_budgets::perf_diff_present_200x60_full_change` |
| flatten+diff+present 200x60 + Shimmer full re-shade | ~430 µs | 3 ms | `perf_budgets::perf_frame_with_active_cell_shader_200x60` |
| — phase split: flatten 137 µs / diff 53 µs / present 210 µs | | | `render::compositor::profile` (ignored; `--nocapture`) |
| shading envelope | ~88 kcells/ms | — | derived from the phase split |
| markdown parse+rich, 1000-line doc | ~1.0 ms | 20 ms | `perf_budgets::perf_markdown_parse_large_doc` |
| richtext wrap, 800-para doc @ 60 cols | ~3.2 ms | 20 ms | `perf_budgets::perf_richtext_wrap_large_doc` |
| `text::segments` walk (76-byte mixed line) | ~1.9 µs | — | `text::tests::profile_segments_and_measure_per_keystroke_cost` (ignored) |
| SGR floor on fg-toggle dashboards | 19 B/toggle | — | `dashboard_fg_toggles_emit_only_the_irreducible_sgr` |

Honest caveats for the docs: budgets hold on an M-class laptop; the
irreducible byte cost of truecolor styling is the payload, not
overhead (no pen cache can shrink `38;2;r;g;b`); 256-color caps are the
lever for byte-constrained links.

## 7. Debugging tools (troubleshooting chapter)

- `render::snapshot(&surface)` — bordered char grid ("where did my text
  land"). `render::snapshot_styles` — adds per-row style-run
  annotations (`0..3 fg=#ff0000 attrs=B link=…` — "why isn't this
  bold"). Both have doctests.
- `Compositor::set_debug_damage(true)` — magenta outlines around every
  repaint region, on screen. Diagnostic modes all: bytes change, never
  use in golden tests.
- `Surface::debug_validate()` (test/debug builds) — structural oracle
  for wide-pair/pool invariants.

## 8. Known limits to state plainly (render-owned)

- Glyph pool cap 4096/surface, link table cap 65535/surface: past them,
  labeled degradation (U+FFFD / plain text) + counters
  (`pool().dropped()`, `links_dropped()`), never unbounded growth.
- Ambiguous-width characters follow unicode-width narrow; a terminal
  configured ambiguous-wide breaks cell layout for every TUI — the
  presenter's risky-cluster CUP discipline bounds drift, cannot erase it.
- `resize` retains pool entries only dropped cells referenced (bounded
  by unique long clusters; compaction hook = `clear`).
- Scroll optimization requires terminal DECSTBM/SU/SD compliance —
  every VT100 descendant has it; `PresenterOpts` can force it off.

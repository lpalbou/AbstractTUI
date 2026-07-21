# RENDER — cycle 7 requests + notices

## To REDTEAM

1. **RT6-3 root cause is the perf harness, not the pipeline — please run
   perf binaries with `--test-threads=1`.** Evidence: the shader frame
   medians 430-597 µs isolated (3 runs, release, your own
   `perf_frame_with_active_cell_shader_200x60`), but 3.56 ms when the
   whole perf binary runs with default parallel test threads — the same
   binary, same box, minutes apart. Your cycle-6 filing's "best 1.16 ms"
   equals the then-isolated median exactly; the median crossed the line
   because wall-clock tests co-scheduled with parser-soup (151 ms/iter)
   and the VT-model test measure the scheduler, not the code. The
   inflation grows as you add perf tests (cycle-4's 421 µs reading
   predates most of the file), so serial execution (or one
   `#[test]`-per-binary) is the durable fix. `tests/perf_budgets.rs` is
   yours — happy to review the harness change.
2. **Phase split for your books** (release, isolated, in-module
   `render::compositor::profile::profile_shader_pipeline_phases_200x60`,
   run with `--ignored --nocapture --test-threads=1`): flatten
   (compose+shade) 137 µs / diff 53 µs / present 210 µs ≈ 400 µs total
   for a full 200x60 re-shade per frame. Envelope: ~88 shaded kcells/ms.
   Present dominates — it writes ~50 KB of styled bytes for a
   full-change frame; that is payload, not overhead.
3. **New adversarial surface: `CellShader::changed_region(t0, t1,
   bounds)`** — the active-region damage hint (default `None` = old
   whole-layer behavior; third-party shaders unaffected). The contract
   to attack: outside the returned rect, `shade` must be BIT-STABLE
   between the two clocks (not identity-with-source — reveals hide cells
   at both clocks). All eight built-ins implement it; my property test
   (`changed_region_hints_are_honest_for_every_builtin`) sweeps a 60x24
   grid over mid-flight/settled/rewind/period-wrap clock pairs — extend
   with your own clock fuzzing; a too-small rect is the bug class.
   Behavioral wins to golden: `Vignette` ticks free (t unused), settled
   `ScanlineFade`/`GradientReveal`/`Dissolve` tick free, moving
   scanline/sweep/axis-wipe damage thin bands. `Shimmer`/`Rainbow`
   deliberately return `None` mid-flight (only exact phase equality is
   bit-safe). Your perf-budget Shimmer workload is therefore
   byte-identical before/after this change — the budget number is not
   gamed by the hint.
4. **`tests/alloc_budget.rs` gltf sampling test fails under default
   parallel threads, passes serial — decisive repro**: `cargo test
   --test alloc_budget` → gltf FAILED (twice this cycle); same command
   with `--test-threads=1` → 8/8 ok. The global allocation counter
   reads other test threads' allocations. Same fix as (1):
   `--test-threads=1` for measurement binaries (or thread-filtered
   counting). My presenter/diff alloc pins pass in BOTH modes.

## To DESIGN

1. **`MdStyles::with_ink(code_fg, code_bg, link_fg)` shipped** — the
   canonical theme mapping with your `base`-must-stay-fg-less rule
   encoded (doc'd on the field + constructor, test-pinned:
   `with_ink_maps_theme_colors_and_keeps_base_fgless`). It produces
   exactly your cycle-6 widget mapping (code chip = code_fg on code_bg,
   link fg + underline, emphasis/headings attribute-only so they inherit
   block ink). Your `md_styles(t)` in `widgets/markdown.rs` can collapse
   to `MdStyles::with_ink(t.text, t.surface_raised, t.link)` — or stay
   as-is; the constructor exists so the NEXT consumer doesn't rediscover
   the base-fg trap.
2. **`Timeline::seek(t)` / `seek_reversed(t)` shipped** (your re-open
   condition was "when a consumer exists" — the boot player + effects
   demo scrubbing counts): `tl.seek(t).progress(track)` samples the
   whole board at one instant; `seek_reversed` is a clock mirror
   (`duration − t`), NOT easing reversal — easings evaluate at the
   mirrored clock, curve identity untouched. Say if the splash scrub rig
   needs anything else shaped.
3. **Free ticks for your effect layers**: with `changed_region`, a
   layer whose reveal has settled — or that carries `Vignette` — can
   keep its clock advancing at zero repaint cost. If the boot
   storyboard holds shader layers past their settle point, you no longer
   pay full-layer damage per frame for them.

## To REACT

- FYI: `Layer::set_shader_t` now damages only the shader's declared
  active region (whole layer when the shader declines to declare, which
  is also the default for any shader you author without the hint). No
  API change on your overlay path; `set_shader`/install still damages
  the full bounds.

## Notices (no action needed)

- Presenter pen heuristic measured on a theme-heavy dashboard (64x20,
  ~320 fg toggles between two colors): one fg-only `38;2` per toggle,
  zero bg/attr churn, 19.0 bytes/SGR — pinned by
  `dashboard_fg_toggles_emit_only_the_irreducible_sgr`. A 1-entry
  last-pen cache was considered and declined: SGR has no pen-restore
  instrument, so there is nothing shorter for a cache to emit.
- Grapheme segmentation measured at 1.9 µs/`segments()` walk and 6.3
  µs/wrapped `measure()` (release) — a per-keystroke cost with hundreds
  of calls per millisecond of headroom. LRU cache declined until a
  profile shows it hot.
- Rich-text wrap emission de-churned (`push_run`: borrowed-text tail
  append, no per-cluster Span/String): your 800-para workload now
  medians 3.18 ms on a quiet box (best 3.16, worst 3.21 — tight), vs
  15.9 filed. Linearity in span count test-pinned
  (`wrap_scales_linearly_in_span_count_structurally`: merge checks the
  LAST span only; output spans track style changes, not input spans).
- Wide-glyph × scroll-opt audit: targeted property case added
  (`wide_pairs_at_band_edges_survive_scroll_optimization`) — CJK chrome
  rows trimmed off the band, half-pair clobber inside a shifted row,
  wide entering rows incl. the degraded last column, byte-replayed on
  VtScreen. Green.

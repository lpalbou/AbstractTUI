# RENDER — cycle 5 requests + verdicts

Author: RENDER.

## 1. Scroll optimization: default ON, referee-verified

- `PresenterOpts::default()` now enables `scroll_optimization`. The
  pairing is TYPE-LEVEL: `FrameDiff::compute_scrolled` returns a
  `ScrolledRuns` token (private fields); `Presenter::emit_scrolled` is
  its only consumer and `emit` cannot accept one — the cycle-4
  wrong-pairing hazard is unrepresentable now, not documented.
- Detection gained EDGE TRIMMING: unchanged rows at the damage union's
  edges (fixed chrome — headers/footers inside full-frame damage) are
  excluded before anchoring, so banded lists engage and the DECSTBM
  region covers exactly the moving interior. `saved` counting now also
  excludes rows that never moved (`sy != y`), keeping the byte-win floor
  honest.
- Byte-level property verified twice over: in-module
  (`vtscreen_replays_scrolled_frames_exactly` — 12 randomized rounds of
  scroll+edit sequences through emit_scrolled -> VtScreen ->
  assert_screen_matches, zero unknown sequences) and against your
  published workloads (`redteam_workload_bytes_with_optimization_on`).
- **Numbers vs your cycle-5 baselines (same builders, same sizes,
  property asserted every frame):**

  | workload | baseline (plain) | optimized | win |
  | --- | --- | --- | --- |
  | log-append 90x28 | 2,318 B/frame | 298 B/frame | 7.8x |
  | list-scroll 70x20 | 1,607 B/frame | 178 B/frame | 9.0x |
  | banded 70x22 | 1,648 B/frame | 202 B/frame | 8.2x |

## 2. To REDTEAM — adv_scroll finding, RESOLVED same-cycle

Your new `scrolled_banded_chrome_property_and_bytes_won` initially failed
on an UNDER-COVERING damage rect, independent of the scroll path:
`banded_list_frame` paints items on rows `1..h-1`, but the "band-tight"
damage was `Rect::new(0, 2, w, h - 4)` — rows 1 and h-3 changed outside
the declared damage, which the damage contract forbids any diff strategy
to repaint; the property assert correctly reported the stale row. You
landed the widened band (`0, 1, w, h-2`) before my cycle closed —
adv_scroll is 8/8 with the optimization defaulted ON, including your
randomized mutation rounds. One stale comment remains in the full-bleed
variant ("expected to decline"): edge trimming now ENGAGES there
(shifted 29/30, property holds) — a comment refresh next pass, nothing
behavioral. Your paired-path verdict lines at close: log-append 7.8x,
banded 8.1x, list up+down 9.0x — consistent with my in-module numbers.

## 3. To REACT — two one-liners + one adoption note

- **Theme ground**: `Compositor::set_ground(Some(theme.bg))` wherever
  the theme signal lands (and on switch — the damage_all your theme
  contract already does covers the repaint). Default `None` is
  byte-identical legacy; with a ground, additive light and translucent
  veils blend against the theme background instead of black/passthrough
  (DESIGN's flagged risk closed). Untouched cells keep terminal-default
  bg — the ground never materializes where nothing blends.
- **Scroll path adoption**: the driver's present phase can now use
  `diff.compute_scrolled(prev, next, damage)` +
  `presenter.emit_scrolled(token, ...)` unconditionally — the token is
  plain-compatible (detection declines to a plain token; opts stay
  consultable via `presenter.opts()` if you want the switch explicit).
- **LayerStack verdict (task closed, no limbo)**: RETIRED — deleted
  `render::layer_stack` + `flatten_stack`. Your `app::overlays` registry
  has the properties that mattered (monotonic u64 ids never reuse =
  stale-handle safety; Weak-backed handles; `damage_root_under` covers
  reveal), and one compositor entry point beats two. The generational-id
  design is preserved in this file's history if id-space reuse ever
  becomes real.

## 4. To DESIGN

- Theme-ground additive shipped (your afterglow-over-theme-bg concern):
  once REACT wires the signal, `Blend::Additive` layers read as light on
  the theme ground with zero changes on your side.

## 5. Cycle-close status

- Crate: 739 lib tests green / 0 failed (9 ignored = foreign perf pins);
  adv_scroll 8/8 (post their band fix), adv_render 13/13, alloc_budget
  5/5 (no-change frames still 0/0/0 with detection code in the tree —
  compute_scrolled allocates only its fingerprint scratch, reused).
- Clippy: zero warnings in render/text/anim (crate-wide cleanup share
  done).

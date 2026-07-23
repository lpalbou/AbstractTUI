# Wave 12 — CODE → VISUAL handoff

Received your `visual-to-code-handoff.md` (05:31) mid-session; item-0
collision notice honored — verified both edit sets coexist in
`widgets/{list,table,tabs}.rs` (your type docs + my wave-12 reentrancy
comments; nothing clobbered). Your `examples/widgets.rs` was mid-edit
when my final `cargo fmt --check` ran — I left it untouched; expect a
one-hunk rustfmt diff on your side.

## What changed under you this wave (src/, all behavior-neutral)

### 1. File splits (0990) — 12 files, `#[path]` sibling pattern

If you have doc references INTO these files by line number, they moved;
public rustdoc paths are all unchanged (re-exports pin them —
`cargo semver-checks` vs 0.2.18: 196/196 pass, "no semver update
required").

- `three/load.rs` → + `load_rig.rs` (Rig/Pose/sampling/skin-gate),
  `load_texture.rs` (texture decode), `load_tests.rs`
- `three/scene.rs` → + `scene_camera.rs` (Camera+Light),
  `scene_shading.rs` (mip/skin/lambert/winding helpers), `scene_tests.rs`
- `three/{extract,raster,doc,brandmark}.rs` → + `*_tests.rs` each
- `term/caps.rs` → + `caps_detect.rs` (passive env pass), `caps_tests.rs`
- `term/unix.rs` → + `unix_setup.rs` (construction/raw-mode/self-pipe)
- `ui/mod.rs` → + `mod_tests.rs`; `ui/tree.rs` → + `tree_dispatch.rs`
  (hit-test/hover/capture/dispatch — where the click-synthesis code
  you documented now lives)
- `app/overlays.rs` → + `overlay_input.rs` (event routing),
  `overlay_handles.rs` (LayerHandle/ImageHandle)
- `testing/vt.rs` → + `vt_print.rs` (print/wrap/line-feed plane)

Your doc-comment blocks in the split files travelled with their items
(none of the files you doc-touched were split this wave — list, table,
tabs, block, scroll, input, markdown, code, image, viewport3d,
page_host all remain single-parent + their pre-existing test siblings).

### 2. Reentrancy audit (wave-11 deferral)

No reachable same-slot reentrancy found; the two real sites were
already converted (Drawer `on_close` take-call-putback, GraphView
`fire_press`). The invariant is now WRITTEN DOWN on
`widgets::SharedCallback` (widgets/mod.rs) with one-line pointers at
all nine borrow-across-callback sites, and pinned by two new tests:
`callback_reentrancy_pin_controlled_write_and_flush_mid_callback`
(list_tests) and `callback_reentrancy_pin_on_change_opens_a_modal`
(select_tests). Docs implication for you: "callbacks may open modals /
write controlled signals / dispose scopes synchronously" is now a
TESTED promise you can state in guide prose.

### 3. Your capture example — windows warnings fixed

The new review family's `Shot` table + `PathBuf` import warned under
`--target x86_64-pc-windows-msvc` (unix-gated pty path leaves them
unused). Fixed with a `#[cfg(unix)]` import move + one
`#[cfg_attr(not(unix), allow(dead_code))]` on `Shot` — behavior
untouched, workspace is now windows-warning-free.

## Your handoff items — status on my side

- **#1 measure inflation crushing shrink(0.0)/fixed-h** — accepted as
  the top engine investigation for the next wave; not attempted
  mid-wave (layout-solver work under a splits wave would muddy the
  byte-neutrality proof). Needs a minimal repro pinned as a failing
  test first; your recipe is enough to write one.
- **#2 Scroll over measureless content** — same lane; likely
  document-or-fix decision (Feed has a measure fn, plain Element trees
  don't). Next wave.
- **#3 mermaid.live hyperlink** — agreed, extensions/mermaid should
  emit OSC-8 on the fallback URL. Small, next wave.
- **#4 GraphView force-layout auto-center** — reasonable `.center()`
  builder or first-render bbox centering. Next wave.
- **#5 debug-damage toggle** — a `RunConfig` knob is the honest shape;
  driver is deliberately last in the 0990 split queue, good moment to
  add the knob when it's touched.
- **#6 PageHost grow default / PushToTalk label / Block clip** — all
  recorded; PageHost default-layout change is a behavior change and
  needs its own wave slot + changelog line.

## Perf status

All 19 ignored perf tests pass with wide margins; byte ratchets landed
exactly on their recorded baselines. Numbers pinned in
`reviews/wave12/perf-status.md` — cite freely in docs.

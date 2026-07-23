# Proposed: File-size budget reconciliation — the >600-line inventory

## Metadata
- Created: 2026-07-25
- Status: Proposed (maintenance — wave-11 quality audit finding)
- Completed: N/A

## ADR status
- Governing ADRs: None. CONTRIBUTING.md declares the budget ("aim for
  under ~600 lines per file; split modules rather than growing one").

## Context

The wave-11 audit measured the tree against the declared budget. The
`#[path]` sibling pattern is established and working (drawer_open.rs,
page_host_bar.rs, choice_prompt_parts, driver_images/screenshot/suspend,
view_cards/edges/style in the graph crate) — but a set of files sits
well past 600 and predates the recent waves. Splitting them mid-audit
was deliberately NOT done (mechanical splits are churn-risky right
before release gates and belong to their owners), so this item is the
honest inventory for the next maintenance window.

## The inventory (2026-07-25, lines incl. comments)

Wave-12 progress (CODE, 2026-07-25): 12 of the worst offenders SPLIT
with the `#[path]` sibling pattern, zero behavior change (full package
suite green after each family; clippy zero; fmt clean). Post-split
line counts in parentheses.

DONE (wave 12):
- src/three/load.rs — 1205 → DONE (load.rs ~452; siblings load_rig.rs
  ~230 rig/pose/skin-sanitation, load_texture.rs ~90 texture decode,
  load_tests.rs ~485)
- src/three/scene.rs — 1084 → DONE (scene.rs ~500; scene_camera.rs
  ~125 camera+light, scene_shading.rs ~110 shading/winding helpers,
  scene_tests.rs ~380)
- src/term/caps.rs — 953 → DONE (caps.rs ~520; caps_detect.rs ~185
  passive env pass, caps_tests.rs ~275)
- src/ui/mod.rs — 943 → DONE (mod.rs 55; mod_tests.rs ~895 — a
  test-only sibling, within the non-goal's intent)
- src/ui/tree.rs — 886 → DONE (tree.rs ~460; tree_dispatch.rs ~455
  hit-test/hover/capture/dispatch plane)
- src/three/extract.rs — 810 → DONE (extract.rs 544; extract_tests.rs 271)
- src/app/overlays.rs — 825 → DONE (overlays.rs ~510;
  overlay_input.rs ~150 event routing, overlay_handles.rs ~205
  LayerHandle/ImageHandle)
- src/testing/vt.rs — 780 → DONE (vt.rs ~600; vt_print.rs ~205 the
  print plane — vt_csi/vt_dump/vt_state were already siblings)
- src/term/unix.rs — 761 → DONE (unix.rs ~557; unix_setup.rs ~230
  construction + raw-mode/session plumbing)
- src/three/raster.rs — 752 → DONE (raster.rs 453; raster_tests.rs 304)
- src/three/doc.rs — 729 → DONE (doc.rs 576; doc_tests.rs 158)
- src/three/brandmark.rs — 670 → DONE (brandmark.rs 495;
  brandmark_tests.rs 180)

REMAINING (next maintenance window; original counts):
- src/app/driver.rs — 1115 (deliberately LAST: damage-contract phases
  live in the turn loop; the phase structure is the natural seam)
- src/app/selection.rs — 688
- src/app/choice_prompt.rs — 687
- src/gfx/session.rs — 686
- src/reactive/runtime.rs — 678
- src/input/parser.rs — 668
- src/app/acceptance.rs — 651 (in-crate acceptance tests — test-only,
  non-goal territory)
- src/widgets/textarea.rs — 643
- src/widgets/input.rs — 639
- src/gfx/pipeline.rs — 636, src/gfx/mosaic.rs — 635, src/gfx/png.rs — 630
- src/widgets/image.rs — 627
- src/app/choice_prompt_view.rs — 611
- src/theme/registry.rs — 601

## Direction

Owner-by-owner splits using the established `#[path]` sibling pattern,
scheduled OUTSIDE feature waves (one module family per pass, suites
green between passes). The three/ family (load/scene/extract/raster/doc
≈ 4.5k lines) is the largest coherent chunk and should go first; the
driver's turn loop is the most delicate (damage-contract phases live
there) and should go last, with the phase structure as the natural seam.

## Non-goals

Test-only files near the budget (e.g. textarea_tests.rs 647) satisfy
the budget's intent through the sibling pattern already; splitting
tests for line count alone is churn.

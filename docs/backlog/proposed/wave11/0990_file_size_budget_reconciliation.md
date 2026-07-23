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

Engine (non-test):
- src/three/load.rs — 1205
- src/app/driver.rs — 1100 (already has three siblings; the core turn
  loop + enter/probe remain)
- src/three/scene.rs — 1084
- src/term/caps.rs — 953
- src/ui/mod.rs — 943
- src/ui/tree.rs — 886
- src/three/extract.rs — 810
- src/app/overlays.rs — 810
- src/testing/vt.rs — 780
- src/term/unix.rs — 761
- src/three/raster.rs — 752
- src/three/doc.rs — 729
- src/app/selection.rs — 688
- src/app/choice_prompt.rs — 687
- src/gfx/session.rs — 686
- src/three/brandmark.rs — 670
- src/input/parser.rs — 668
- src/reactive/runtime.rs — 652
- src/app/acceptance.rs — 651 (in-crate acceptance tests)
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

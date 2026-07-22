# Proposed: Feed custom blocks cannot host widgets — protocol images degrade to mosaic

## Metadata
- Created: 2026-07-22
- Status: Proposed (capability gap — first-app finding, 0.2.0 adoption wave)
- Completed: N/A

## ADR status
- Governing ADRs: None. ADR impact: none — widget composition surface.

## Context
`abstractcode-tui` adopted `widgets::Feed` for its whole transcript in the
0.2.0 upgrade (keyed items, windowed paint, measured extent — all wins).
One item class regressed: inline images. On 0.1.0 the hand-rolled transcript
column mounted the `Image` WIDGET per image item (`ImageFit::Contain`), which
rides the full graphics ladder — kitty / iTerm2 / sixel protocol images on
capable terminals. `FeedBlock::Custom` hands the app a draw closure over
`&mut dyn StyledCanvas`, and `StyledCanvas` has no graphics channel: no
protocol-image placement, no `blit_mosaic`, only `put`/`print`.

## Current code reality
- `FeedBlock::Custom(CustomBlock)` (src/widgets/feed.rs:88-116) is the only
  app escape hatch, and its draw signature is cell-only.
- The `Image` widget's protocol path needs element lifecycle (placement
  tracking, presenter custody for kitty payloads) that a paint closure
  cannot provide by design (the damage contract's byte-custody rule).
- `gfx::mosaic` is public, so the app-side workaround exists and ships:
  `render_to_cells(bitmap, rect, caps)` + per-cell `canvas.put` inside a
  custom block, aspect-corrected app-side for the ~1:2 terminal cell
  (abstractcode-tui `src/ui/transcript_view.rs::image_block`). The caps
  passed are synthesized (`unicode_ok + truecolor`), not the PROBED ones —
  there is no public "current terminal capabilities" accessor either.

## Problem or opportunity
A transcript is exactly where generated images land, and Feed is exactly the
transcript widget. Apps adopting Feed silently trade protocol-grade images
for mosaic. Two composable gaps:
1. Feed items cannot host widget-grade blocks (image being the motivating
   case; a viewport3d thumbnail is the same shape).
2. Apps cannot read the probed `term::Capabilities` at runtime, so even the
   mosaic fallback guesses (`MosaicMode::auto` fed with synthesized caps).

## Proposed direction (engine's call)
- Either a `FeedBlock::Image(Arc<Bitmap>)` first-class block that routes
  through the real graphics ladder with placement lifecycle owned by the
  Feed widget, or a general widget-hosting block (an `Element` per block —
  heavier, solves the class).
- Independently: a public read-only accessor for the probed capabilities
  (`app::capabilities()` or similar) so canvas-level fallbacks stop
  guessing. This is small and useful beyond Feed.

## App-side workaround to delete when this lands
`abstractcode-tui src/ui/transcript_view.rs::image_block` (mosaic custom
block + hand-rolled contain-fit + synthesized caps).

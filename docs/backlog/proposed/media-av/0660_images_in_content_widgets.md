# 0660 — Images inside content widgets (Feed/Markdown) via protocol placement

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: the study-2 lifecycle fixes (shipped 2026-07-22 — vacated
  rects, placement replace); app-widgets 0144 (markdown images) names
  the mosaic half of this story.
- Feed-block family (convergence cycle 2): this item and app-widgets
  **0102 (`FeedBlock::Rich`)** are SIBLINGS — both extend the
  `FeedBlock` enum (Text/Markdown/Code/Custom today,
  src/widgets/feed.rs:74-94), one with a span-model block, one with an
  image block, and first-app **0280** (custom blocks cannot host
  widgets) is the same enum's third pressure. One design pass should
  settle the block vocabulary — whichever of the three executes first
  owns that pass and the other two review it; the enum grows ONCE,
  never per-item.
- Promotion trigger: a chat/feed app rendering image attachments (the
  a2a chat port names media messages), or 0144's promotion.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none expected (composition of
  shipped seams).

## Context
`Overlays::image` is a SCREEN-space rect — right for a picture viewer,
wrong for "an image inside message 47 of a scrolling feed": the overlay
does not scroll with content, clip to the widget, or die with the item.
The study-2 audit made the underlying lifecycle safe (moves/removals
now repaint honestly); what is missing is the binding from a WIDGET's
solved rect to an image slot.

## Current code reality
- `widgets::Image` (src/widgets/image.rs:20-34) documents the seam
  honestly: draw closures own cells, so the widget is mosaic-only; the
  protocol path lives at app level.
- The tree exposes solved rects (`UiTree::rect_of`, src/ui/tree.rs:420)
  and the driver's image pass reconciles slots per frame
  (src/app/driver_images.rs) — the pieces exist; nothing connects a
  ViewId's rect to an ImageHandle across layout/scroll changes.
- `ImageSession` now replaces kitty placements by id on move (cheap),
  so a scrolling feed re-placing every visible image per scroll tick is
  affordable on the kitty channel; iTerm2/sixel re-emit full payloads
  (documented cost) — a feed on those channels likely wants mosaic.

## Problem
Feed/markdown images need position-follows-widget, clip-to-viewport,
and lifetime-follows-item; hand-wiring that per app re-derives scroll
math and leaks placements on item eviction.

## What we want
1. An **anchored image binding**: `ImageHandle::follow(view_id)` or an
   `Element::image_region(bitmap)` builder — the driver updates the
   slot rect from the solved rect each frame (a set_rect-equivalent,
   already ghost-safe), releases on unmount.
2. **Clip honesty**: a partially visible image CLIPS on kitty (placement
   supports source-rect cropping — `x,y,w,h` keys) and falls back to
   mosaic-through-cells on cursor-paint channels (which cannot clip);
   label the degradation.
3. Channel policy hook: feeds may force mosaic (`MosaicOpts`) where
   re-emission cost beats fidelity.

## Scope / Non-goals
Scope: the binding, clip story, eviction lifetime, feed example.
Non-goals: image loading/caching policy (app-side), animated images
(0665), layout that reserves rows for images (0144's markdown story).

## Expected outcomes
A feed shows pixel images that scroll, clip, and vanish with their
items — no ghosts, no terminal-memory leaks (the session referee
already asserts both).

## Validation
- Driver tests: scroll a feed with a bound image → kitty bytes show
  re-place (no retransmit); evict the item → delete emitted; KittyModel
  stays leak-free. Mosaic path: cells follow the item, restore on evict.

## Progress checklist
- [ ] Rect-follow binding + eviction lifetime
- [ ] Kitty clip via source-rect keys; labeled cursor-paint fallback
- [ ] Feed example + referee tests

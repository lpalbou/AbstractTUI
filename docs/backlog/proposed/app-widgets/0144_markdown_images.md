# 0144 — Markdown images: in-flow mosaic rendering

- Status: proposed
- Track: app-widgets
- Origin: seeded by extensions/0460, cycle-4 handoff
- Depends on: none (JPEG decode exists; PNG exists)

## Problem

`![alt](path)` is currently rendered as text. A markdown reader and a
chat transcript both need inline images.

## What we want to do

An image block in the md vocabulary: decode (PNG + JPEG — widen the
Image widget's PNG-only path), render via the mosaic renderer (cell-safe
in any scroll context), alt text as caption/fallback, width capped to
content width with aspect preserved, lazy decode on first visibility
(a Feed with 100 images must not decode all at mount).

## Open design note (named, not solved here)

Pixel-protocol images (kitty/iTerm2) inside scrollable flowing content:
placement/eviction under partial visibility is unresolved engine-wide;
this item ships MOSAIC-ONLY and defers protocol images in flow to a
follow-up with the damage-contract owner.

## Validation

Golden mosaic snapshots; decode-failure degrades to alt text with a
labeled notice; lazy-decode test (decode count == visible images);
missing-file honesty.

Full analysis: docs/backlog/proposed/extensions/0460 (§seeds).

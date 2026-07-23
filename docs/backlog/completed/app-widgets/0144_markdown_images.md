# Completed: 0144 — Markdown images: in-flow mosaic rendering

- Status: completed (app-widgets wave 3, READER seat)
- Track: app-widgets
- Origin: seeded by extensions/0460, cycle-4 handoff
- Depends on: none (JPEG decode exists; PNG exists)
- Completed: 2026-07-23

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

## Completion report

- Final path: docs/backlog/completed/app-widgets/0144_markdown_images.md
- Date: 2026-07-23
- Vocabulary: `md::ImageBlock { alt, src }` (`DocBlock::Image`) — a
  WHOLE-LINE `![alt](src)`; inline images inside paragraphs stay
  literal text (documented); empty `src` stays literal; empty alt is
  legal (no caption row). Single-line block = trivially correct
  streaming (complete line seals).
- LAZINESS IS TWO-PHASE (src/widgets/markdown_image.rs): typeset reads
  ONLY container headers via the new `gfx::probe_dimensions`
  (src/gfx/probe.rs — PNG IHDR + JPEG SOF marker walk, fuzz-pinned to
  agree with the real decoders on everything they accept) through an
  incremental read ladder (64 KiB → 2 MiB → rest, still decode-free);
  full decode + `mosaic::render` happen at FIRST DRAW of any slice row,
  cached in a bounded thread-local LRU keyed by (path, file signature
  = len^mtime, cols, rows) — the (path, width) key the item asked for
  plus a change-detection signature, surviving ELEMENT REBUILDS (a
  search keystroke re-typesets the reader; it must not re-decode
  visible images). Test-pinned: `typeset_probes_sizes_without_decoding`
  (3 images, 0 decodes), `first_draw_decodes_visible_images_once_and_caches`
  (visible-only + rebuild reuse + scroll-into-view pays exactly one).
- Sizing: half-block geometry (1 px/col, 2 px/row), native width capped
  to content width, aspect preserved; a MAX_IMAGE_ROWS=200 guard keeps
  pathological aspect ratios (1×10000 px) from exploding the typeset
  row count (shrink stays aspect-true).
- MOSAIC-ONLY, as specified: image rows are cells (`Row.image` slice →
  `MosaicGrid` row blit inside `draw_rows`), safe in any scroll
  context. The protocol-in-flow question stays OPEN and is documented
  in the module header (placement/eviction under partial visibility =
  damage-contract territory; `widgets::image`'s protocol seam note is
  cross-referenced).
- Honesty states: probe failure → one labeled notice row
  ("⌧ image unavailable: <src> (<reason>)") + alt caption; decode
  failure AFTER a valid header → the reserved rows stay blank except
  slice 0's labeled "⌧ <alt>: decode failed — ..." (test:
  `undecodable_body_after_valid_header_fails_loudly_at_draw`);
  missing-file honesty pinned. Never silent, never fake pixels.
- Image widget widening: `widgets::Image::from_path` now routes through
  `gfx::decode_image` (magic-sniffed PNG + baseline JPEG) instead of
  the PNG-only decoder; error label updated ("undecodable: ...").
- Tests: gfx/probe.rs inline suite (decoder-agreement pin, zero-dims
  rejection, hostile corpus + truncation ladders + magic-stamped soup),
  markdown_image_tests.rs (laziness, cache, sizing/aspect/row guard,
  missing/undecodable honesty, no-caption-for-empty-alt), image.rs
  (`from_path_decodes_a_real_file_through_the_unified_decoder`).
- Proof vehicle: `examples/reader.rs` embeds a GENERATED png (lazy
  decode observable) and an honestly-missing image; `live_reader` pty
  smoke green.

## Post-completion fix (wave-3 cycle-3 close, CLOSER — 2026-07-23)

Cycle-2 review R-3 (`reviews/wave3/review-cycle2.md`): the decode/probe
cache's file identity was (size, mtime) alone — the known
same-mtime-rewrite class (JsonFileRunStore scan-memo lesson: "mtime
alone is NOT file identity"). A same-length rewrite on a 1s-granularity
filesystem (HFS+, NFS, FAT) or under mtime-preserving tooling
(`rsync -a`, `tar`) served stale pixels forever. `file_signature` now
folds the platform file id into the hash in the metadata read already
paid: unix `dev + ino` (`std::os::unix::fs::MetadataExt` — the
write-tmp-then-rename pattern mints a new inode per rewrite), windows
`creation_time` (std's volume/file-index accessors are unstable;
in-place same-size same-mtime overwrites remain undetected there —
documented degradation, cosmetic blast radius). Test:
`same_size_same_mtime_rename_rewrite_invalidates_the_caches`
(length-equalized red/blue PNGs, rename-replace, mtime pinned back
with `File::set_modified` — only the inode discriminates; pixel +
decode-count asserts).

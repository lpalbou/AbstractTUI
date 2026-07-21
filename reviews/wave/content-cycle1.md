# CONTENT cycle 1 — md::StreamSession (backlog 0110)

## Shipped

`render::md::StreamSession` (src/render/md_stream.rs, child module of
`md` so it reuses the parser's own private line classifiers —
`heading_level`, `list_marker` — zero duplicated boundary logic, no
drift possible).

API:

```rust
let mut s = StreamSession::new(styles);
s.append(delta);                 // any chunking; O(open block) per call
s.closed_blocks() -> &[Block];   // frozen, index-stable, append-only
s.closed_revision() -> u64;      // bumps on growth (0100 consumer seam)
s.open_blocks() -> &[Block];     // parse of the open tail (re-done per append)
s.finish() -> Vec<Block>;        // EOF-closes fences; idempotent
s.open_len() / s.bytes_reparsed_total(); // honest cost meters
```

## Design (the sealing rule)

`md::parse` is line-oriented with exactly two multi-line constructs:
fences and paragraphs. `parse(prefix) ++ parse(suffix) == parse(whole)`
whenever the cut is at a line start that is (a) outside any open fence
and (b) not splitting a paragraph joint. Each append scans the tail's
complete lines and seals the longest safe prefix. The incomplete final
line is classified WORST-CASE: `---` may still become paragraph text
(`---x` soft-joins backwards) so it never seals; committed prefixes
(` ``` `, `>`, `# `, `- `, `1. `) can never become paragraph joiners,
so they seal the block before them immediately.

## Validation (all green; 940 lib tests total, no existing test touched)

- `any_chunking_equals_batch_parse` — 39-doc corpus × 7 chunkings
  (whole / char-by-char / per-line / 4 random-seeded) == `md::parse`.
- `randomized_documents_hold_the_equivalence` — 200 random documents
  from boundary-hostile fragments, random chunkings.
- `open_fence_reports_as_code_before_the_close` — mid-fence honesty
  (0110 §3).
- `rule_shaped_fragment_does_not_seal_the_paragraph` — the `---x`
  hazard that motivates worst-case fragment classification.
- `committed_fragments_seal_the_preceding_paragraph`.
- `closed_blocks_only_append_and_revision_tracks_growth` — the 0100
  freeze contract (typeset closed[i] once, never revisit).
- `appends_behind_closed_content_cost_only_the_open_block` — 1,000
  closed lines, 50 token appends: re-parse bytes bounded by the open
  region alone (byte-meter assertion, not timing).
- `finish_is_idempotent_and_eof_closes_fences`, empty-input edges.

## Drift vs the backlog item

None. One precision worth recording: the item says "the session
maintains … one open tail block" — the open region is *usually* one
block but can transiently parse to more than one between an append and
its seal (never observable as wrong output; `open_blocks()` returns a
slice for this reason).

## Notes for peers

- `md.rs` gained only the module declaration + re-export (my file).
- No public-surface changes elsewhere; no Cargo/lib/prelude edits.

## Next (cycle 2)

`Feed` widget consuming `closed_revision`/`open_blocks` for the
streaming tail item.

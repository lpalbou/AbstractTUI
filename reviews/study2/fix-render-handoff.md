# FIX-RENDER → FIX-INPUT handoff (cycle 3): overview.md ledger rows

FIX-INPUT is the single `docs/backlog/overview.md` writer this cycle;
FIX-RENDER does not touch it. Both items below are DONE — completion
reports written, files moved to `docs/backlog/completed/first-app/`,
whole-tree battery green (1470 tests / 52 suites, clippy zero, fmt
clean, alloc pins green, `cargo semver-checks` clean vs 0.2.1).

## Rows to ADD to the completed table

```
| 0298 | P0 fixed: stale frame band after resize — `apply_resize` now pairs the prev-poison with `Presenter::invalidate()` (cursor+pen), so the post-resize frame re-anchors with absolute CUP instead of relative motion from the pre-reflow parked cursor; every {resize, modal close} interleaving pinned cell-for-cell vs a fresh-paint oracle (tests/adv_resize_modal.rs) — completed 2026-07-22 (fix wave cycle 3) | completed/first-app/ |
| 0290 | UX footgun fixed: every selection copy now ENDS the gesture (release-copy and mid-drag Enter/`c`/Ctrl+C clear the region with the copy; `SelectionAct::Copy` carries the region) — post-copy keys reach the app immediately; api.md key table added; regression `release_copy_frees_enter_and_c_for_the_app` — completed 2026-07-22 (fix wave cycle 3) | completed/first-app/ |
```

## Rows to REMOVE from the proposed/filed table

- The `| 0290 | Selection region lingers after release-copy … |` row
  (was line ~100).
- The `| 0298 | Stale frame band above the live frame … |` row
  (was line ~104).

Also: the prose block around line ~163 ("0290/0298 remain this
cycle's…") can be updated to reflect both landing in cycle 3 —
your call as ledger owner.

## One-line summaries (for any convergence doc)

- **0298 root cause**: the resize path repaired the CELL model (prev
  poison → every cell re-emits) but not the PRESENTER model — the
  virtual cursor still held the old park position, and `move_cursor`
  emits relative motion when row or column matches, so the first
  post-resize run was placed by `CUU` from wherever the emulator's
  reflow left the physical cursor (bottom-anchored growth in the
  field). Fix: `driver.rs::apply_resize` adds
  `self.presenter.invalidate()` — the same rule `boot::player` and
  `external_write` already followed. `docs/design/render.md` §2.4
  documents invalidate-on-resize.
- **0290 fix**: copy = end of gesture, uniformly (release and
  key-copies). Chosen over key-copy-one-shot-alone because a retained
  region still ate the FIRST post-release `c`/Enter (the reported
  composer failure at one-keystroke strength).

# Wave-4 handoff — sync + anchored seat (0282 / 0292 / 0294)

Date: 2026-07-23. Three consumer-filed items (first-app, 0.2.6
adoption wave) shipped, tested, documented, and moved
proposed→completed. This file carries the `docs/backlog/overview.md`
rows for the closer (the overview itself is deliberately untouched —
shared-file discipline; hand these rows over verbatim).

## Rows for the completed table

| 0282 | `FeedState::sync_with(cx, read, spec)` — the sync bridge behind a borrow-based source (items INSIDE a `Signal<Fold>` / a focus-selected nested vec; zero copies, closure-read signals become effect deps, stats-only writes render nothing); `sync` delegates, one shared drain core (fast paths + rebuild policy + one-writer self-heal, whole prior suite pins it) — completed 2026-07-23 (0.2.6 field wave) | completed/first-app/ |
| 0292 | Completion trigger position policy: `trigger_at` + `TriggerPosition::{Anywhere, StartOfInput, StartOfLine}` (first-token semantics, leading whitespace tolerated; refused positions never consult the provider; same char re-registrable under different policies, first passing wins; plain `trigger` = `Anywhere`, byte-identical) — completed 2026-07-23 (0.2.6 field wave) | completed/first-app/ |
| 0294 | Anchored-panel placement bias: `PanelPlacement::{BelowPreferred, AbovePreferred}` + `place_panel_biased` + `AnchoredPanel::open_passive_biased` + `Completion::placement` (mirror rule — short lists off the chrome row below a bottom composer; default `BelowPreferred` everywhere, parity-grid-pinned; owned mode unchanged, can adopt additively) — completed 2026-07-23 (0.2.6 field wave) | completed/first-app/ |

## Rows to REMOVE from the proposed/filed table

The overview currently lists 0292 and 0294 as filed (the "Filed
2026-07-22" rows); 0282 may or may not have a row depending on the
closer's snapshot. All three move to the completed table above.

## Files touched (for the merger's awareness)

- `src/widgets/feed_sync.rs` (drain core extracted, `sync_with`
  added, `sync` delegates), `src/widgets/feed_sync_tests.rs` (child
  module hook), `src/widgets/feed_sync_with_tests.rs` (new).
- `src/app/anchored.rs` (`PanelPlacement`, `place_panel_biased`,
  `open_passive_biased`; `place_panel`/`open_passive` delegate),
  `src/app/anchored_completion.rs` (`TriggerPosition`, `trigger_at`,
  `placement`, policy gate in `find_token`),
  `src/app/anchored_tests.rs` (rig parameterized:
  `completion_rig_with(size, status_row, wire)`; child module hook),
  `src/app/anchored_policy_tests.rs` (new).
- Shared files, append-only edits: `src/prelude.rs` (one wave-append:
  `PanelPlacement`, `TriggerPosition`), `docs/api.md` (three one-line
  notes: feed-sync section, completion section ×2), `CHANGELOG.md`
  (`## [Unreleased]` created with three Added entries — peers append
  under it).

# field-agora — findings from the agora watcher build

Bug reports and footguns discovered while building the second-wave
validator app `agora-tui` (the read-only multi-channel agora hub watcher,
`/Users/albou/projects/gh/agora-tui` — validator app #1 of the 2026-07-23
maintainer decision; scope in `../../planned/live-data/0060_*`). This is
the live-data track's field evidence: every item here should be hit live
during that build, reproduced against the published `abstracttui` 0.2.8
(or the current release), and worked around app-side; each item records
the workaround so the engine fix can delete it.

Band: **0800–0890** (registered in `../../overview.md`). House grammar per
the `../first-app/` items: one file per finding, `NNNN_snake_title.md`,
engine `file:line` cites, a Class (bug / footgun / API gap / capability
gap / UX defect / rendering defect / feature), the app-side workaround,
and what the engine fix would let the app delete. The engine team
round-trips these fast — first-app's 19 items are the precedent.

This track is expected to carry the first NETWORKED field evidence:
`reactive::connection` + `Backoff` (live-data 0040), `bounded_source`
(0020), and `channel_source`/`latest_source` (0010) have never fed on a
real hub for hours. Findings about the transport seam belong here too —
they are the evidence live-data 0050's transport ADR waits on.

Each item carries a severity in its Metadata and in the row here:
**P1** blocked the build / **P2** cost real time, workaround holds /
**P3** paper cut.

Round-trips already observed: 0850 (Disclosure/Card + `Feed::on_item_press`)
shipped in abstracttui 0.2.11 the same day it was filed, alongside
`Scroll::scrollbar_auto_hide` (the cycle-3 design review's first engine
ask, pre-empted); agora-tui adopted 0.2.11 and deleted its click-toggle
gap the same session.

| ID | Title | Class | Severity |
| --- | --- | --- | --- |
| 0800 | use_startup_notices carries unbounded mid-session diagnostics | API gap | P3 |
| 0810 | List rows are plain strings — no badge slot | capability gap | P3 |
| 0820 | Connection has no app-initiated re-dial verb | API gap | P3 |
| 0830 | reconnect countdown needs app-side deadline bookkeeping | API gap | P3 |
| 0860 | RichTextView/MarkdownView have no intrinsic measure — invisible in Scroll | footgun | P3 |
| 0870 | FeedItem headline single-row/nowrap mode (folded cards wrap into body-lookalikes) | capability gap | P3 |
| 0880 | FeedItem body max-measure for wide terminals | capability gap | P3 |
| 0885 | Disclosure title needs a rich-span slot (folded cards lose identity color) | capability gap | P2 |
| 0900 | Completion panel occludes the row above a bottom-docked composer (needs a reserved-rows/offset knob) | API gap | P2 |
| 0905 | Drawer needs vertical insets so docked chrome stays visible | API gap | P3 |
| 0910 | Scroll of widgets: no ensure-visible / child-offset verb (consumers hand-roll height models) | API gap | P2 |

(Ledger repair 2026-07-25: the 0895–0910 rows were missing here — the
band overflowed past 0890 before the overflow rule existed. Number
collision on record: 0900/0905/0910 duplicate ids in
`../field-gateway/` (their own band) — track+number is the working
key; renumbering is the owners' call, the wave11/0990 precedent.
field-gateway continues at 1000–1050; field-core owns 1100–1190.)

Completed 2026-08-21 (the plan's ordered engine lane,
`plan/agora-ui.md` §4 items 1 and 2 — moved to
`../../completed/field-agora/`):

| ID | Title | Class | Severity |
| --- | --- | --- | --- |
| 0895 | Bound `Scroll::offset_y(Signal)` ignored inside Drawer pages — FIXED (`335aea1`), plus the spun-off `extent_signal` warm-start half (`862525c`, documented `12bb12a`). The report's cause was wrong and its symptom exact: not the Drawer, not `PageHost` — `Feed` × `Scroll` across a REMOUNT, which a drawer page is just the commonest way to trigger. `tests/scroll_remount_offset.rs` 3/3, zero `#[ignore]` left. | bug | P1 |
| 0890 | Disclosure capped body under-measures rich feed items — DOES NOT REPRODUCE on 0.4.0; closed by evidence, not by a fix, and pinned so it stays closed (`fc0e2ac`). All three reported shapes plus the report's own two controls are now regressions in `tests/wave_disclosure.rs`. The consumer-side receipt (agora-tui deleting its `panes.rs` workaround) is theirs, not ours. | bug | P2 |

Completed 2026-08-20 (layout-honesty wave, moved to
`../../completed/field-agora/`):

| ID | Title | Class | Severity |
| --- | --- | --- | --- |
| 0840 | layout docs: grow vs intrinsic basis for content-heavy panes — the suggested paragraph landed in `docs/getting-started.md`, extended with the wrapper caveat (`basis` describes one element; a wrapper around a `Scroll` re-derives a content-sized basis). Closed together with first-app 1330, the second field report of the same trap. | UX defect (docs) | P3 |

Completed 2026-07-24 (disclosure wave, moved to
`../../completed/field-agora/`):

| ID | Title | Class | Severity |
| --- | --- | --- | --- |
| 0850 | Disclosure/Card widget for feed items — standalone `Disclosure` widget + `Feed::on_item_press`/`FeedState::item_at_row` (the unreachable click-hit-info) + the documented message-card recipe (fold map + `(rev, folded)` fingerprint); the feed-NATIVE card kind deferred behind first-app 0280's draw-only block boundary | capability gap | P2 |

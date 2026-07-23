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

| ID | Title | Class | Severity |
| --- | --- | --- | --- |
| 0800 | use_startup_notices carries unbounded mid-session diagnostics | API gap | P3 |
| 0810 | List rows are plain strings — no badge slot | capability gap | P3 |
| 0820 | Connection has no app-initiated re-dial verb | API gap | P3 |
| 0830 | reconnect countdown needs app-side deadline bookkeeping | API gap | P3 |

# Proposed: use_startup_notices carries unbounded mid-session engine diagnostics — a consumer surfacing "startup" notices surfaces a live diagnostics firehose

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-agora, agora-tui build)
- Severity: P3 — cost ~30min live (toast spam over the watcher's panes); app-side workaround holds
- Class: API gap

## Context
agora-tui boots degraded on purpose (missing key, unreachable hub, protocol
skew) and the quality bar is "the app still boots and says why", so the app
wired `use_startup_notices(cx)` to a surface-each-notice effect (a `Toast`
per notice — the pattern the getting-started guide suggests: "Read them
reactively with the `use_startup_notices(cx)` hook and render them in a
status line or toast").

First live run (debug build, 120×34 pty): the app ALSO had a layout bug — a
fixed-height row collapsing under overflow pressure. The engine's
zero-collapse diagnostic caught it (excellent diagnostic, verbatim fix in
the message) — but each collapse note was `push_startup_notice`d
mid-session, my effect toasted each, and every toast animation re-solved
layout. The result on screen was a stream of "engine: layout: fixed-size
child … collapsed" toasts sliding over the watcher's panes, minutes into
the session. These are not startup notices; they are a per-frame
diagnostics lane wearing a startup-notice name.

## Current code reality (0.2.8)
- `src/app/notices.rs:25` — the store is a thread-global, grow-only
  `Signal<Vec<String>>`; `publish_notice` (`:48`) only ever pushes.
- `src/app/driver.rs:396` — zero-collapse diagnostics drained per frame
  are forwarded with `app.push_startup_notice(note)`; `:405` does the
  same for image-ladder degradation labels. The driver's own
  `collapse_log` caps at 64; the notices vec has no cap.
- Anything a long-lived app pushes late lands in the same vec with no
  kind, severity, or timestamp — a notice bar cannot tell "input layer
  degraded at boot" from "your layout collapsed on frame 40312".

## Repro
Mount any app whose tree has a collapsible fixed row (a `Cells(1)` child
without `shrink(0.0)` beside a `grow` sibling whose content overflows) in
a debug build, subscribe `use_startup_notices`, and toast each new entry.
Resize the terminal a few times: each new collapse signature lands as a
fresh "startup" notice mid-session and the vec grows for the life of the
process.

## Workaround in the field (delete when fixed)
`src/ui/header.rs::notice_line` in agora-tui: never toast engine notices;
render a single passive line showing `list.last()` plus a count
("N engine notices · latest: …"), in `text_faint`. What an engine fix
would let the app delete: the count-and-latest folding — with a
kind/severity split (startup vs diagnostics), a bounded ring, or a
separate `use_diagnostics(cx)` lane, the notice line could honestly show
boot degradations only and the diagnostics could go to a debug overlay.

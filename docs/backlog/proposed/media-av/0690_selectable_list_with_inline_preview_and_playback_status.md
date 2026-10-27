# 0690 — Selectable list with an inline preview action, and a playback/progress status widget

## Metadata
- Created: 2026-10-28
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: 0640 (external audio-process lifecycle pattern) for the
  playing half; nothing else.
- Root link: AbstractFramework backlog 1055 (voice library track).
- Promotion trigger: the AbstractGateway console-TUI voice picker
  (`console-tui/src/ui/routes.rs:2861`, a Combobox whose Test
  synthesizes but does not play) or any other "browse items and hear /
  see each one" app.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none expected (two composable
  widgets; the engine still owns no audio I/O — 0640).

## Context
A recurring app shape: go through a list of items and preview each
one in place (a voice, a sound, a sample, a theme), then keep the one
you like. The web console now does it for voices: choosing an item
starts its preview at once, a newer choice cancels the older one, and a
play/stop toggle at the right of the row replays from the start or
stops. The terminal has the parts but not the shape:
- `List` (src/widgets/list.rs:6, 250) separates `on_select` (the
  highlight moved) from `on_activate` (Enter/click) — the right split,
  but no per-row trailing action and no debounce for "preview what is
  highlighted".
- `Combobox` (src/app/select_combobox.rs:50, 120) fires `on_change` only
  on commit — a user cannot hear an item while browsing.
- `Progress` (src/widgets/progress.rs:25) draws a fraction; nothing
  shows the idle / loading / playing / failed states of a preview.

## Problem
Each app hand-rolls the same three things — preview on highlight with
cancellation of the previous preview, a play/stop affordance, and a
status line — and gets the edge cases wrong (a stale preview playing
after the selection moved, a spinner that never ends, a preview that
keeps playing after the list closes).

## What we want (generic: no voice, gateway or audio types)
1. **`List` / `Combobox` preview hook**: `on_preview(index)` fired when
   the highlighted row SETTLES (a debounce knob, default ~200 ms) while
   browsing, distinct from `on_select` and `on_activate`; a monotonic
   preview id so the app can drop a stale result (the generation
   pattern), and a cancel callback for the previous preview.
2. **A trailing row action**: an optional per-row (or selected-row)
   trailing button slot (`▶` / `■`) with its own key (Space on the
   focused action) and mouse hit area, not consuming the row's activate.
3. **`PlaybackStatus` widget**: idle / loading (spinner) / playing
   (elapsed vs duration via `Progress` when known) / failed (one line,
   the app's sentence); the app drives it with one signal. Pure view: it
   plays nothing (0640 owns the process lifecycle and the kill-on-drop
   guard so a preview stops when the scope dies).
4. An example: a list of N items whose preview is a mock process
   (`/bin/sleep` or a timer), test-covered without audio hardware:
   browsing quickly starts one preview, a newer highlight cancels the
   older one, stop resets, play replays from 0, leaving the list kills
   the preview.

## Validation
Unit tests for the debounce and the generation drop (a late result for an
old highlight never reaches the status), the trailing action's key and
hit area, the status transitions; the example under the test driver.
Red checks: without the generation id a stale preview shows; without the
cleanup guard the mock process outlives the scope.

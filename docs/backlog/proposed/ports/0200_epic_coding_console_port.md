# 0200 — EPIC: coding-agent console on AbstractTUI (abstractcode port)

## Metadata
- Created: 2026-07-21
- Status: Proposed (epic; blocked on widget + live-data dependencies)
- Track: ports
- Completed: N/A

## ADR status
- Governing ADRs: None in this repo (no ADR system yet — see
  ../app-widgets/0170). The console ships as its own crate; engine-side
  API rulings from 0170 gate what it can rely on.

## Context
The target is a Rust console for a coding agent, functionally matching
what `../abstractcode` ships in Python on prompt_toolkit: a scrollable
rich transcript (streaming model output, code blocks, tool-call cards
with live status), an approval flow, a multiline composer with `/command`
and `@file` completion, a status/cache meter, permission modes, session
tabs, and a detail/timeline panel. The backend boundary is
**`abstractcode serve`** — a long-lived JSONL protocol over
stdin/stdout — so the console is a pure frontend: no agent logic, no
provider code, no tool execution.

The completeness review (reviews/cycle11/completeness-and-code-port.md
§2) evaluated exactly this port and concluded: feasible, with three P0
widget additions and two P1s; everything else on the console's
requirement list is composition over already-shipped, test-pinned engine
surfaces.

## Current code reality
- Backend protocol (`../abstractcode/docs/cli.md:83-110`): commands
  `prompt` / `approve` (decision allow|deny|all, optional deny `reason`) /
  `answer` / `steer` / `cancel` / `status` / `quit`; events `ready`,
  `run_started`, `phase`, `cycle`, `thought`, `tool_call`, `tool_result`
  (bounded `result_preview` + `truncated` flag), `denied`,
  `approval_required`, `ask_user`, `status`, `steer_queued`, `ack`,
  `error`, `llm_call`, `final`. Default permission mode `write`; gated
  calls emit `approval_required` and wait for the controller.
- Reference UI being ported (read, not reused):
  `../abstractcode/abstractcode/fullscreen_ui.py` (~4,368 lines —
  scrollable history, fixed input, status bar, `/`-completion menu whose
  first screen is curated, fullscreen_ui.py:50-120; arrow/history
  semantics at :144-163), `react_shell.py` (~13,300 lines — mostly
  in-process engine logic that `serve` replaces), `session_timeline.py`
  (rewind/fork = transcript cut at user-message boundaries),
  `terminal_markdown.py` (the conservative streaming renderer the engine
  supersedes), `tool_permissions.py` (read-only/write/full-auto modes,
  Shift+Tab cycling), `cache_meter.py`.
- Engine surfaces verified ready by the review (§2a table): cross-thread
  ingestion (`WakeHandle::post` + waker, one frame per burst,
  worker-panic surfacing — test-pinned), `Modal` approval flow with focus
  trap, `Tabs`, Badge/Progress/Sparkline meters, Shift+Tab decoding on
  every terminal (src/input/mod.rs:174), theming incl. `code_token_color`,
  scroll-region byte wins for append-heavy transcripts, and CaptureTerm +
  `Driver::turn` + VtScreen for fully headless development.
- **`../abstractcoder` is a paused charter, not a start**: it contains
  ARCHITECTURE.md, Cargo.toml, `src/model.rs`, `src/protocol/types.rs` —
  no lib.rs/main.rs, not a buildable crate; explicitly put on hold by the
  operator. Its architecture mapping (reader thread per child →
  `WakeHandle::post` → signals → widgets) was reviewed as correct and is
  design input here; its code is three checkpoint files, not a codebase.
  Do not report this epic as "already underway".

## Problem
Without this epic the engine's app-readiness claims stay theoretical, and
the widget items lack the consumer that proves their shapes. Without the
widget items the port has no transcript (Feed), no streaming markdown, no
composer, and hand-rolls follow-tail — the review's verdict is explicit
that those are the only real gaps.

## Dependencies (build order matters)
- ../app-widgets/0100 — Feed/Transcript widget (the transcript itself).
- ../app-widgets/0110 — md::StreamSession (streaming model output into
  the tail item).
- ../app-widgets/0120 — TextArea (composer + history + `/` and `@`
  completion dropdown).
- ../app-widgets/0130 — follow-tail + size query (pinned transcript).
- ../app-widgets/0140 — lexers (proposed): the **diff lexer** is the
  console's strongest want (tool-result patch previews); python/js/toml
  next. Phase-3 quality, not a phase-1 blocker.
- ../app-widgets/0150 — terminal verbs (set_title = session/build status,
  bell/notify on approval-needed and turn-complete).
- Live-data track (band 0010–0090, separately authored):
  0010 (async source → Signal binding — the reader thread →
  `WakeHandle::post` pattern with an ownership rule), 0020 (bounded,
  coalescing ingestion — a flooding `tool_result` stream must batch per
  wake, not post per line), 0030 (the live-feed example + docs page the
  console's feed code starts from). The network-transport items
  (0040/0050) are NOT needed here — the backend is a local subprocess
  pipe.

## Phased plan
- **Phase 0 — dependencies land.** 0100/0110/0120/0130 + live-data
  0010/0020/0030. Nothing app-shaped starts before the transcript stack
  exists (the review costs the workaround as throwaway).
- **Phase 1 — read-only viewer.** New crate (name TBD at kickoff; the
  paused `abstractcoder` charter is prior art, adopt-or-supersede is the
  kickoff's first decision). Spawn `abstractcode serve`, parse the event
  stream, render: transcript Feed (thought/cycle/final as streaming
  markdown items; tool_call/tool_result as plain framed items), status
  bar from `status`/`llm_call` events, follow-tail. No stdin commands
  except `quit`. Headless CaptureTerm tests against recorded JSONL
  fixtures.
- **Phase 2 — interactive core.** Composer (`prompt`), approval Modal
  (`approval_required` → allow/deny/all + deny reason), `ask_user`
  answers, `steer`, `cancel`, error surfacing. The full command loop of
  one session.
- **Phase 3 — console parity.** Permission-mode switch (Shift+Tab +
  footer control), session tabs (N serve children, one Feed each),
  detail/timeline panel (per-turn tool timeline from events), cache/token
  meter (`llm_call` fields), tool-call cards with fold/expand + diff
  tinting (0140), `/`-completion over the console's command subset,
  `@file` mentions.
- **Phase 4 — polish.** notify/bell/title via 0150, copy-message
  (OSC 52), themes pass, keymap help, worked docs.

## Scope / Non-goals
Scope: the console crate, its JSONL client, fixtures, and upstream bug
reports into the widget items. Non-goals: porting `react_shell.py`'s
in-process engine (serve is the boundary — the Python side keeps owning
agent logic); parity with all ~60 slash commands (fullscreen_ui.py:56-120
— phase 3 picks the console-meaningful subset; config-editing commands
stay in the Python CLI); rewind/fork v1 (session_timeline semantics need
serve-side support first); MCP/skills management UI; the web host;
mouse text selection (command-copy first, per the review's P1-6).

## Expected outcomes
A daily-drivable Rust console over `abstractcode serve` proving the
engine's application story end-to-end; upstream widget items validated by
a real consumer; the paused charter superseded by a running program.

## Validation
- Phase gates: each phase ships with CaptureTerm acceptance tests driven
  by recorded serve-session fixtures (including a flooding tool_result
  fixture to prove bounded ingestion under load).
- One live end-to-end session against a real `abstractcode serve` child
  per phase, on macOS and Linux.
- Perf: streaming a long turn (1k+ events) keeps frame cost bounded by
  the tail item (0100's budget), verified in release mode.

## Progress checklist
- [ ] Phase 0: dependencies confirmed landed (numbers above)
- [ ] Kickoff ruling: crate location/name; abstractcoder charter
      adopt-or-supersede
- [ ] Phase 1: read-only viewer + fixtures
- [ ] Phase 2: interactive core
- [ ] Phase 3: parity features
- [ ] Phase 4: polish + docs

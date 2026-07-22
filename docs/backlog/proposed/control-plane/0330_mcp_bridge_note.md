# 0330 — MCP bridge note: agent-protocol access as an external client of 0320

## Metadata
- Created: 2026-07-22
- Status: Proposed (design note — the deliverable is a documented
  mapping + a bridge hosted OUTSIDE this crate)
- Track: control-plane
- Depends on: 0320 (wire protocol), 0310 (verb semantics)
- Completed: N/A

## ADR status
- Governing ADRs: none yet; rides 0320's protocol ADR. No separate ADR:
  this item deliberately adds NOTHING to the core crate's surface.

## Context
The maintainer's ask names MCP (Model Context Protocol) explicitly: an
agent should be able to control a running app "through an API or MCP
server". MCP servers speak JSON-RPC over stdio (or HTTP) and expose
typed *tools*; agents call tools. The right architecture question is
not "how does AbstractTUI speak MCP" but "where does the MCP dependency
live" — and the five-dep posture (`Cargo.toml:16-34`,
`docs/design/00-vision.md` dependency policy) answers it: **not here**.
An MCP server is a protocol TRANSLATOR, and 0320 was designed so that
translation is mechanical.

## Current code reality
- 0320 (proposed) defines JSONL verbs: inject / query / invoke /
  subscribe / event — each already a natural MCP tool shape (a typed
  request with a structured reply).
- The semantic-tree query serializes `AccessSnapshot` rows
  (`src/ui/access.rs:97-104`) — role/label/value/focused/bounds — which
  is exactly the observation surface an agent needs to decide what to
  press; `AccessSnapshot::to_text` (access.rs:118-145) is the
  human/LLM-readable rendering of the same rows.
- `Actions::list()` (`src/app/actions.rs:166-176`) gives per-app
  discoverable commands; with 0310's description field, an MCP bridge
  can surface each registered action as its own named tool
  dynamically.
- The family already runs this pattern in production: thin bridges over
  a JSONL child-process protocol (the `abstractcode serve` lane the
  0200 port epic consumes, `docs/backlog/proposed/ports/README.md:15-18`).

## Problem
Without a written mapping, the first person wanting agent access will
either propose an MCP dependency in-core (posture violation) or invent
a bespoke bridge with its own security assumptions (bypassing 0320's
reviewed boundary).

## Placement (settled, cycle 3)
Converged with the extensions band's 0400 placement ruling
(reviews/study/extensions-on-platform.md P1-2): the 0310 bus is core;
the 0320 server is in-tree behind the default-OFF `control-server`
feature; and **this bridge plus the productized attach client are
OUT-OF-CRATE consumers of the frozen wire protocol** — they hold their
own dependencies (MCP/JSON-RPC here), version against the protocol
ADR, and never gain engine privileges. The attach client may begin as
a feature-gated example for 0360's proof; it productizes into its own
home only on evidence — the same rule as this bridge.

## What we want
1. **A mapping document** (lands in `docs/` with 0320): MCP tool set ↔
   protocol verbs —
   - `screen_read` → query ScreenText / SemanticTree (`to_text` form),
   - `ui_tree` → query SemanticTree (structured rows),
   - `press_key` / `type_text` / `click` / `paste` → inject,
   - `list_actions` / `run_action` → query Actions / invoke,
   - `wait_for_event` → subscribe with a filter + timeout,
   - resize → inject Resize, **headless/serve sessions only**
     (extensions review P2-1: on a real tty nothing resizes the
     physical terminal — the 0310 bus refuses the inject with a
     structured error; the bridge surfaces that error verbatim rather
     than pretending the tool applies everywhere).
2. **A reference bridge, out-of-crate**: a small standalone binary
   (sibling repository or family tooling home — the kickoff ruling
   names it) that connects to one app's control socket and exposes the
   tools above over MCP stdio. It holds the MCP/JSON-RPC dependency;
   this crate never does.
3. **Safety defaults, enforced at the right layer**: the cautious
   read-only posture is a SERVER capability (0320's verb-group mask —
   serve with inject/invoke disabled; extensions review P3-3), not
   bridge politeness: the bridge derives its tool list from the
   `hello`'s enabled verb groups, so a read-only server yields a
   read-only tool list by construction. Bridge-side rules that remain
   bridge-side: connect to explicitly named sockets only (no
   scanning); every tool result carries the app's `hello` identity so
   an agent never confuses two apps.

## Scope / Non-goals
Scope: the mapping doc; the reference bridge (external home); one
worked example (agent reads the semantic tree of the `examples/`
dashboard and toggles its theme via an action).
Non-goals: MCP (or any JSON-RPC) types in `abstracttui`; agent
frameworks, prompting guidance, or tool-approval policy (host
concerns); exposing verbs 0320 does not have (the bridge adds no
power, only translation).

## Feasibility
**Trivial-after-0320, and only after.** The bridge is a few hundred
lines of translation in whatever language the family standardizes its
tooling on precisely because 0320's verbs were shaped as discrete,
id-correlated request/replies. The one design decision with teeth:
whether `run_action` tools are generated per registered action
(dynamic tool list — richer discovery, more MCP-client churn) or one
generic `run_action(name)` tool (stable list, string-typed) — start
generic, revisit on real agent usage. Marked as a note rather than an
engineering item so nobody schedules core-crate work for it.

## Expected outcomes
Agent access becomes a supported, documented consequence of the control
plane instead of a fork risk; the security boundary stays exactly one
(0320's socket), with the bridge adding translation only.

## Validation
- Mapping doc reviewed against 0320's conformance fixture (every mapped
  verb exercised).
- Reference bridge demo: an MCP-speaking client reads the dashboard
  example's semantic tree, invokes a registered action, observes the
  resulting lifecycle/custom event — scripted, reproducible.

## Progress checklist
- [ ] Mapping doc (tools ↔ verbs, safety defaults)
- [ ] Kickoff ruling: bridge home + language
- [ ] Reference bridge against the conformance fixture
- [ ] Worked example against a shipped example app

# 0215 — EPIC: gateway configuration wizard (second validator app)

## Metadata
- Created: 2026-07-23 (maintainer validator decision)
- Status: Planned (maintainer-chosen second validator, alongside the
  0060 watcher)
- Track: ports (app epics band 0200-0219)
- Completed: N/A

## Context

The maintainer chose two second-validator apps to break the
single-consumer evidence bias (nearly all field signal comes from
abstractcode-tui, a chat/composer app): the 0060 read-only a2a watcher
(live-data proving ground) and THIS — an intuitive wizard to configure
AbstractGateway, "similar to gateway/console but improved".

The reference UX is the gateway admin console (Users & Entities,
Runtimes, Providers, Multimodal capability routes, Sandbox): wide data
tables with state badges (configured/covered/not-configured/linked;
enabled/asleep) and per-row actions (Edit/Clear/Override/Configure),
plus multi-step setup flows with validation and a final apply.

## What we want to do

A standalone TUI app (own repo or examples-adjacent — maintainer's
call at kickoff) that walks an operator through configuring an
AbstractGateway instance: connection (base URL + token, probe +
honest state), provider setup, multimodal capability routes
(provider/model per route with default-vs-override semantics — the
fabricated-selection lesson from the console applies: placeholder
rows, resolved "applies now" lines, never a fabricated pair), users
and entity summoning basics, and a review+apply step.

## Engine surfaces this validates (the point)

- app-kits 0500 selects (Select/Combobox for provider/model pickers)
- app-kits 0510 form kit (field rows, validation, masked token input
  — `TextInput::masked` shipped 0.2.1) and 0520 wizard flow
  (multi-step, per-step validation gate, summary/apply) — BOTH still
  proposed: this epic is their promotion trigger and their 0590-class
  validator
- 0530 table upgrades (badge cells + row actions) for the
  routes/users tables
- reactive::connection (0040, shipped 0.2.3) for the gateway
  connection state
- control-plane 0340 Persist (resumable wizard) when it lands —
  optional leg

## Non-goals

Not a monitoring dashboard (the watcher + entity monitor cover the
realtime class); no gateway-side changes — the app consumes the
existing admin HTTP API; no write operations beyond what the console
already exposes.

## Validation

The wizard configures a real local gateway end-to-end (connection →
provider → one multimodal route → apply → verify via GET) driven only
by keyboard; headless test coverage per the engine's CaptureTerm
pattern for every step's form logic.

## Sequencing

0510 form kit → 0520 wizard flow → this epic's build. 0530 table
upgrades can land during or after. Start after the 0.2.7
consumer-polish wave per the maintainer's next_wave decision.

## Project scaffolded + launch prompt ready (2026-07-23)

The setup wave is closed; the build can start from a fresh session.

- **Scaffold**: `/Users/albou/tmp/abstractframework/abstractgateway/console-tui/`
  (in the abstractgateway package, maintainer's placement call) —
  compiles against crates.io `abstracttui = "0.2.8"`, `q` quits,
  headless run exits 0 with a skip notice (re-verified 2026-07-23,
  forced recompile).
- **Builder brief**: `LAUNCH-PROMPT.md` in that directory — mission,
  gateway API grounding (every claim verified at source in the cycle-2
  review), engine guide, design laws, feedback protocol. Its §2.1
  first-hour path was executed live 2026-07-23: gateway started with
  the documented command (free port; 8080 was occupied by the
  operator's gateway — the prompt now documents that case), ping
  returned the documented shape
  (`{"ok":true,"status":"healthy","service":"abstractgateway","time":…}`),
  capability-defaults + `/me` matched their documented shapes, process
  killed clean.
- **Feedback track**: engine findings from this build file into
  `docs/backlog/proposed/field-gateway/` (band **0900–0990**, README
  table is the landing zone) — the field evidence app-kits
  0510/0520/0530 wait on.

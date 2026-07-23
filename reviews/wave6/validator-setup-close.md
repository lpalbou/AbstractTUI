# Wave 6 close — validator-app setup (2026-07-23)

The second-validator setup wave is closed. Two apps are scaffolded,
briefed, and live-proven ready for fresh builder sessions. This file is
the maintainer's handoff: what exists where, the proof, and the two
launcher stubs.

## What exists where

| App | Repo | Brief | Epic | Feedback track (engine repo) |
| --- | --- | --- | --- | --- |
| `abstractgateway-console` (gateway config wizard, validator #2) | `/Users/albou/tmp/abstractframework/abstractgateway/console-tui/` | `LAUNCH-PROMPT.md` in that directory | `docs/backlog/planned/ports/0215_gateway_config_wizard_app.md` | `docs/backlog/proposed/field-gateway/` — band **0900–0990** |
| `agora-tui` (read-only hub watcher, validator #1) | `/Users/albou/projects/gh/agora-tui/` | `LAUNCH-PROMPT.md` in that directory | `docs/backlog/planned/live-data/0060_milestone_multi_room_watcher.md` | `docs/backlog/proposed/field-agora/` — band **0800–0890** |

Both scaffolds: crates.io `abstracttui = "0.2.8"`, forced recompile
green 2026-07-23, headless run exits 0 with a clean skip notice. Both
feedback-track READMEs are empty landing zones with the table header in
place; both bands are registered in `docs/backlog/overview.md`.

## Cycle-3 live proof (first-hour paths executed, 2026-07-23)

**Gateway (LAUNCH-PROMPT §2.1, executed as a cold builder would):**

- Port 8080 was already occupied by the operator's own gateway
  (untouched); started on a free port with the documented command,
  changing only `--port`:
  `ABSTRACTGATEWAY_AUTH_TOKEN=console-dev-secret-0123456789
  /Users/albou/tmp/abstractframework/.venv/bin/python -P -m
  abstractgateway serve --host 127.0.0.1 --port 8090`.
- Boot took ~2 minutes before the port listened (embedding stores);
  the documented ping probe then returned exactly the documented shape:
  `{"ok":true,"status":"healthy","service":"abstractgateway","time":"2026-07-23T17:40:41.150616+00:00"}`.
- Second documented read `GET /api/gateway/config/capability-defaults`:
  `ok:true, authority:"abstractcore.local", writable:true`, 24 route
  rows, row keys `{key, kind, modality, task, label, configured,
  source, provider, model, base_url}` — matches §2.3 (the `config_hint`
  field was confirmed in source as the remote-core-degraded case,
  `abstractgateway/capability_defaults.py:39`). `GET /api/gateway/me`:
  `admin/default, admin:true, auth mode legacy-token, routing
  single-user` — matches §2.1.
- Process killed; port 8090 verified freed; the operator's 8080
  listener verified untouched.

**Agora (read-only subset — no joins, no posts, no acks):**

- `curl -s http://127.0.0.1:8765/healthz` →
  `{"ok":true,"version":"0.12.41","protocol":"agora/0.3","paused":false}`
  — byte-identical to the prompt's documented output. Hub was live; the
  prompt's `agora up` guidance was not needed (and per the wave rules a
  fresh hub was not started).
- `agora whoami --as tui` → seat `tui`, protocol `agora/0.3`, cached
  key works — matches §2.1.
- `agora channels --as tui` → exactly the three documented channels
  (`commons`, `entity-society`, `memory-states`, all public) with the
  `(* = you are a member)` legend and zero member marks for `tui` —
  matches the §2.2 membership-gate claim. The documented one-time
  `agora join --channel <c> --as tui` setup remains the builder's act
  (syntax verified against `agora join --help`, not executed).

**Prompt fixes made this cycle (reality won twice, both gateway-side):**

1. §2.1 "Three parts are load-bearing" → four: added the busy-8080
   reality (probe first, pick a free `--port`, never kill the existing
   listener) — 8080 WAS occupied during the proof.
2. §2.1 gained a "boot patience" note: a minute-plus before the port
   binds, the LanceDB `#FALLBACK` RuntimeWarning is normal, retry the
   probe until 200.
3. §5 headless-testing paragraph now says where tests live (`tests/`
   at the crate root, same layout as abstractcode-tui's model file).
4. The scaffold README's dev-command section mirrors the busy-port +
   slow-boot notes.

The agora prompt needed no fixes — every claim its read-only first
hour makes was verified live as written.

**Engine-side integration done this cycle:** 0215 + 0060 each carry a
dated "Project scaffolded + launch prompt ready" section (scaffold
path + brief + feedback band); 0060's stale `Proposed` title/metadata
aligned with its `planned/` home; overview.md's ports track row now
reads Mixed with `planned/ports/` and names the third epic (its
field-gateway/field-agora rows and 0800/0900 band lines survived the
wave intact); the intro's stale `0.1.0` publish claim updated to
0.2.8.

## Launcher stubs (paste one into a fresh agent session)

**Gateway config wizard:**

> You are building `abstractgateway-console`. Your complete brief is
> `/Users/albou/tmp/abstractframework/abstractgateway/console-tui/LAUNCH-PROMPT.md`
> — read it fully, then execute it. The engine team round-trips
> feedback filed per its §7.

**Agora watcher:**

> You are building `agora-tui`. Your complete brief is
> `/Users/albou/projects/gh/agora-tui/LAUNCH-PROMPT.md` — read it
> fully, then execute it. The engine team round-trips feedback filed
> per its §6.

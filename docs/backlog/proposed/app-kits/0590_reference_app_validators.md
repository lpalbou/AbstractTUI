# 0590 — Reference-app validators: admin console, setup wizard, triage shell

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: every 0500–0580 item it validates (slices land item by
  item); crash-resume slice additionally on control-plane 0340.
- Promotion trigger: the first 0500–0580 item reaching implementation —
  its validator slice starts WITH it (validators grow with the band,
  they do not wait for it).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none (examples + docs only; zero engine surface).

## Context
Roadmap principle 2 is binding here: "Apps validate; they never
design... Gaps found while building them are filed as backlog items —
nothing ships 'just for the app'" (0001 roadmap "Design principles").
Every existing wave proved capabilities with an in-repo runnable
(`examples/dashboard` for density + timers, `examples/feed.rs` for the
live-data pattern, `examples/transcript.rs` for Feed/streaming —
examples/ listing verified). The port epics (0200 console, 0210 chat —
`docs/backlog/proposed/ports/`) are REAL products with networking and
protocols; they validate eras, not widget kits, and they live
out-of-repo. This band needs its own, smaller vehicles: deterministic,
fixture-fed, in-repo examples exercising each kit item against the
brief's app classes — cheap enough to run in CI as CaptureTerm
acceptance, honest enough to expose composition gaps before a product
hits them.

## Current code reality
- **The example convention exists and is the mold**: examples/ carries
  dashboard/ (main.rs 682 lines + data.rs 185 — a deterministic data
  walk, `examples/dashboard/data.rs`), common/ shared helpers, a
  README, `--caps` diagnostics + no-tty clean skip
  (`examples/dashboard/main.rs:44-57`) and an
  `ABSTRACTTUI_START_THEME` determinism knob (main.rs:62-68). New
  validators copy this discipline verbatim.
- **CaptureTerm + Driver make examples testable**: the headless
  harness pumps real frames without a tty (`docs/api.md` testing
  section — `App` + `Driver` + `CaptureTerm` compiled example), so a
  validator's acceptance script is an ordinary test.
- **What no example shows today**: any form (no select, no field
  rows), any stepped flow, any rich table (dashboard's table is
  read-only strings, main.rs:560-584), any tree, any split/rail, any
  banner. The gallery (`examples/gallery.rs`) shows widgets in
  isolation — composition at app scale is exactly what it cannot
  prove.

## Problem
Without in-repo validators, the band's items would each be validated
by unit tests alone — and unit tests cannot catch the composition
failures this study's evidence base is full of (0220/0230/0240 all
appeared only when a real app composed the primitives; the 0250 crash
appeared in the first real picker). Waiting for the out-of-repo ports
to validate app-kit widgets inverts the dependency: products would
block on kit bugs found late.

## What we want
Three deterministic examples, each mapping to brief classes, each
growing a slice per landed item, each with a CaptureTerm acceptance
test beside it:
1. **`examples/admin_console`** (brief class A + D-monitoring): fixture
   data (routes/users/entities as a deterministic table, the
   dashboard/data.rs pattern); composes 0560 header (account chip +
   sign-out) + 0560 pinned admin-context banner + 0550 NavList
   sections + 0530 rich table (state badges, Edit/Rotate/Delete
   actions, multi-select, refresh-stability demo) + 0500 selects in an
   edit panel (incl. one MultiSelect with chip overflow) + 0510 field
   rows with masked API-key + 0540 chips.
   Acceptance: the keyboard-only "rotate a key" journey — nav →
   filter → row → action → masked form → gated submit → banner update.
2. **`examples/setup_wizard`** (brief class B): 0520 wizard over 0510
   forms with 0500 selects (provider → model → base-URL/API-key →
   summary/apply); fixture "apply" writes to a struct printed on exit
   (deterministic, no IO). Acceptance: full run, Back-preserves-data,
   refused-Next focuses the error, summary jump-back, apply-once;
   once control-plane 0340 lands, the crash-resume journey (0520 §7:
   kill -9 mid-wizard → restart → `CrashDetected` → resume modal
   restores steps; stale schema refused with a labeled notice) runs
   here — it is simultaneously 0340's own restore-ordering acceptance
   evidence.
3. **`examples/triage_shell`** (brief classes C + D-notes): 0550
   NavList (channels/DMs with 0540 unread badges) + 0550 FilterTabs
   with live counts + 0560 attention banner ("N need vigilance —
   Review") + the shipped Feed as the thread surface + 0580 SplitPane
   between thread and rail (added cycle 2 — the resizable boundary
   needs a validator) + 0580 PanelRail (Members/Files with badges) +
   0570 Tree (a notes-outline panel, with a 0540 TagInput for note
   tags) — fed by a deterministic message generator (the feed.rs
   producer pattern, bounded ingestion). Acceptance: filter switch
   preserves thread scroll/focus; unread counts drain as items are
   read; divider resizes by drag AND keyboard; rail collapse preserves
   panel state; a tag added via TagInput appears as a chip.
4. **Band completion law — with its mechanism (PLATFORM cycle-2
   F10)**: each 0500–0580 item's checklist gains "consumed by a 0590
   validator" before it may complete. Mechanically: examples are
   BINARIES, so `tests/` cannot import them and no current example has
   a beside-it test (the capture pipeline drives built binaries under
   a pty instead, examples/capture.rs). The chosen shape: each
   validator's UI is a COMPOSE FUNCTION in `examples/common/` (the
   existing shared-helper convention); the example binary mounts it,
   and an integration test in `tests/` mounts the same function under
   `CaptureTerm` + `Driver` for the acceptance journey. The
   consumption-law meta test is a SOURCE SCAN over `examples/common/`
   (the widget-membership lint pattern, src/widgets/mod.rs:171-191) —
   asserting every shipped kit widget name appears in at least one
   validator compose fn.
5. **A docs page** (`docs/app-kits.md`): the composition cookbook —
   one section per validator with its wiring diagram (store struct +
   signals per compose.rs:56-85), cross-linking each widget's rustdoc;
   honest about what stays app-side (routing, data, policy). Includes
   the `Detached`-aware kit convention once control-plane 0350 exists
   (answering PLATFORM's cycle-1 ask, accepted cycle 2): admin/chat
   compositions pause pollers/refresh timers on `Detached` and resume
   on attach — a documented convention over 0300's lifecycle events,
   never kit machinery.
6. **Budget honesty** (roadmap principle 2's last clause): each
   validator records its line count and any workaround it needed —
   "a budget overrun in a dogfood build is a finding about the
   engine"; workarounds file back as band items (the first-app
   README's discipline, `docs/backlog/proposed/first-app/README.md`).

## Scope / Non-goals
Scope: three examples + acceptance tests + docs/app-kits.md + the
per-item completion law.
Non-goals: networking, real gateways/hubs, persistence (fixtures
only — the live-data track and ports own real IO); shipping these as
products; screenshot/marketing polish beyond the standard example
discipline (theme knob, --caps, clean no-tty skip); replacing the
gallery (it keeps the per-widget close-ups).

## Expected outcomes
Every band item is proven in composition before any product consumes
it; the brief's four reference UIs have runnable in-repo skeletons a
product team can lift; composition regressions (the 0220/0230/0240
class) surface in CI acceptance runs, not in maintainer sessions.

## Validation
- Each example: CaptureTerm acceptance journey (keyboard-only, plus
  mouse spot-checks), deterministic data, no-tty clean skip, theme
  restyling spot-check across one dark + one light built-in.
- Meta: the source-scan consumption test over `examples/common/`
  compose fns (mechanism in §4); acceptance journeys run as `tests/`
  integration tests mounting the same compose fns under
  `CaptureTerm` + `Driver` — no pty needed for the journeys.
- Docs: app-kits.md snippets compile as doctests where feasible.

## Progress checklist
- [ ] examples/admin_console + rotate-a-key acceptance journey
- [ ] examples/setup_wizard + wizard acceptance journey
- [ ] examples/triage_shell + filter/rail/tree acceptance journey
- [ ] docs/app-kits.md composition cookbook
- [ ] Consumption-law meta test
- [ ] Budget/workaround findings filed back as band items

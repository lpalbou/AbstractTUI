# 0310 — Automation bus: inject input, query semantic state, invoke actions, subscribe to events

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: control-plane
- Depends on: 0300 (event vocabulary)
- Completed: N/A

## ADR status
- Governing ADRs: None yet. ADR impact: the bus verb set becomes the wire
  protocol's contract (0320) — freeze both together under the first
  protocol ADR. 0170's API-stability pass gates the public Rust shapes.

## Context
"An agent should be able to CONTROL a running app — read its state, drive
its interactions, receive its events." Stripped of transport, that is
four verbs: **inject** (input), **query** (state), **invoke** (named
commands), **subscribe** (events). The engine already implements the
mechanics of all four for its OWN test harness; this item turns them
into one composable, thread-safe surface — the in-process API that 0320
serializes, that integration tests drive directly, and that an embedding
host (an app hosting a TUI pane inside a larger program) consumes
without any server at all.

## Current code reality
Each verb exists today, privately or in fragments:
- **Inject**: `Driver.pending: Vec<Event>` (`src/app/driver.rs:117-119`)
  is a queue of events "dispatched by the next turn so ALL routing stays
  inside turn's phase U" — today fed only by `wait_for_activity`
  (driver.rs:405-413). `Driver::handle_event` (driver.rs:452-506) is the
  ONE correct entry: overlays route first (modal ownership,
  driver.rs:479), then the root tree, then global actions
  (driver.rs:486-494), then default quit. Injecting anywhere else is
  wrong: `UiTree::dispatch` (`src/ui/tree.rs:494`) alone would bypass
  modal routing and the action fallback. Tests inject BYTES instead
  (`CaptureTerm::push_input`, `src/testing/capture.rs:83-85`), which
  only works when the harness owns the Terminal object.
- **Query**: `UiTree::accessibility_tree()` (`src/ui/tree.rs:170-173`,
  re-solves layout first so bounds are truthful) yields `AccessSnapshot`
  — role/label/value/focused/bounds/depth rows
  (`src/ui/access.rs:97-104`), values sampled untracked at snapshot time
  (access.rs:17-19), with a stable text form `to_text()`
  (access.rs:118-145). Plus `UiTree::focused()` (tree.rs:420),
  `rect_of` (tree.rs:412), `App::viewport()` (`src/app/mod.rs:286`),
  `Actions::list()` (`src/app/actions.rs:166-176`),
  `App::startup_notices` (mod.rs:206-208). The rendered screen itself
  lives in the driver's `frame` Surface (driver.rs:114) — no text
  accessor today.
- **Invoke**: `Actions::run(name)` (`src/app/actions.rs:125-148`),
  re-entrancy-safe by taking the callback out while it runs; collisions
  refused at registration (actions.rs:55-79). This is exactly "invoke
  registered app commands by name" — already shipped, minus discovery
  metadata (no description field on `ActionInfo`, actions.rs:22-26).
- **Subscribe**: 0300 adds the lifecycle/custom-event surface; nothing
  exists today beyond startup notices (`src/app/notices.rs`).
- **Thread crossing**: `WakeHandle::post` (`src/reactive/scheduler.rs:107`)
  runs closures in phase U — but posted jobs see only the reactive
  runtime, not `App`/`Driver`. A command that must touch the tree or the
  driver needs a queue THE DRIVER drains, exactly like `pending`. The
  waker (`src/term/waker.rs:46-63`) makes any such queue wake a blocked
  loop from any thread.

## Problem
Four fragments, three access levels (driver-private, App-public,
test-only), no composition. An external controller cannot inject a key
into a running interactive app at all (the pending queue is private and
byte injection requires owning the Terminal). Queries exist but only on
the UI thread with `&mut` access. There is no single object a test, an
embedder, or 0320's server can hold.

## What we want
1. **`ControlBus`** (name per 0170 conventions): a cloneable, `Send`
   handle mintable from `App` (opt-in: `app.control_bus()`), holding an
   internal command queue + the terminal waker. The driver drains bus
   commands at a fixed point in phase U (beside `drain_posted`,
   driver.rs:224), so every command runs under the damage contract with
   full routing semantics.
2. **Verbs, v1 set**:
   - `inject(Event)` — enqueue into the SAME path as terminal events
     (`handle_event`: overlays → tree → actions → quit). Accepts the
     `input::Event` vocabulary (keys with mods, mouse, paste, resize),
     so controllers speak semantics, not escape bytes.
     **Resize guard (extensions review P2-1, verified)**: nothing
     resizes the REAL terminal — `apply_resize` reshapes the frame
     model only (driver.rs:508-533), so an injected Resize on an
     interactive session diverges emitted frames from the physical
     screen until the next genuine SIGWINCH heals it. Rule: Resize
     injection is accepted when the session terminal is not a real tty
     (`Terminal::is_tty`, `src/term/mod.rs:172-174`; VirtualTerm and
     CaptureTerm own size truth) and otherwise refused with a
     structured error naming the divergence — layout-testing at sizes
     is a headless/serve capability by construction.
   - `query(Query) -> Reply` — request/reply over the bus:
     `SemanticTree` (AccessSnapshot), `Focused`, `Viewport`,
     `Actions` (list), `Notices`, `ScreenText` (the flattened frame as
     text — new accessor on the driver's `frame` Surface, honest about
     styling loss; styled dump variant later if a consumer needs it).
     `SemanticTree` MUST compose the ROOT tree plus every visible
     OVERLAY tree in z-order (overlay worlds are separate `UiTree`s in
     the store, `src/app/overlays.rs:61-66`; the root snapshot alone
     would describe content a modal is covering and omit the modal the
     user actually sees). The bus needs no public z API for this: it
     drains inside the driver, which already holds the overlay store —
     iteration order is internal (see reviews/study/platform-on-appkits.md
     cycle-3 addendum on the z-allocator home).
     Replies are owned `Send` values delivered via the caller's channel;
     blocking-with-timeout convenience wrapper for test ergonomics.
   - `invoke(name) -> bool` — `Actions::run` by name. Arg-less and
     value-less BY DESIGN in v1 (the closures take no parameters,
     `src/app/actions.rs:31`); parameterized interaction rides
     `inject` + the semantic tree — clicking/typing at annotated
     targets is universal. The v2 seam is named below.
   - `subscribe(filter) -> Subscription<AppEvent>` — fan-out of 0300's
     stream. **Mechanism (settled, extensions review P1-3)**: a
     mutex-owned ring per subscriber with the ingest module's own
     `OverflowPolicy::DropOldest` semantics
     (`src/reactive/ingest.rs:55-63`) + a wakeup — NOT
     `std::sync::mpsc`, whose `try_send` on a full queue can only
     refuse the NEW value. Drop-newest would starve a consumer of
     exactly the event it is waiting for while preserving stale
     history; drop-oldest preserves liveness. Evictions counted on the
     subscription (the `bounded_source` honesty rule pointed outward).
3. **Action metadata**: add an optional description to registration
   (palette + agent discovery read the same field). Small, additive.
   Names follow the dotted-namespace convention actions.rs already
   documents ("file.save", actions.rs:12-13) — extension-registered
   commands namespace themselves ("graph.zoom_in"), which also gives
   the 0330 bridge stable tool grouping with zero new metadata.
4. **Wake correctness**: commands enqueued while the loop is blocked in
   `wait_for_activity` wake it via the terminal waker; commands landing
   mid-frame run next phase U (never mid-present — epoch rule §2).
5. **Docs**: an `examples/` harness driving a real app headlessly via
   the bus (the RT8-2 doctest at `src/app/mod.rs:85-118` upgraded from
   bytes to bus verbs).

## Cross-track answers (settled cycle 2, converged cycle 3)
- **Action metadata (both tracks now agree — this is the decision of
  record)**: **v1 = name + optional chord + optional description;
  actions stay nullary.** The registry's run closures take no
  arguments today (`src/app/actions.rs:31` `run: Box<dyn FnMut()>`)
  and parameterizing them is a semantic change to a shipped surface,
  not metadata. No typed parameter schemas, capability flags, or
  return contracts enter `ActionInfo` — MCP tool schemas are the
  BRIDGE's presentation concern (0330 derives per-action tools from
  the dotted namespace + description); baking schema vocabulary into
  the engine would freeze agent-protocol shape into core, the exact
  inversion the bus-before-wire rule exists to prevent.
- **`invoke_with(name, payload) -> Result<String>` is the RESERVED v2
  seam** (one name, used by both tracks' documents): designed only
  when a real parameterized consumer exists — the extensions band's
  graph editor (0430) is the expected first case ("select node 42" is
  inexpressible nullary). Until then, parameterized interaction rides
  `inject` + the semantic tree. Wire-compat is pre-paid: 0320's
  `invoke` verb reserves an `args` field and answers non-empty args on
  a nullary action with a structured error — adding `invoke_with`
  later breaks no protocol.
- **Canvas-class content is OPAQUE to the semantic tree, by design**
  (extensions metadata answer §4, adopted): cell-drawn content
  (graph/diagram/canvas surfaces, band 0400) is strokes, not elements
  — the tree ends at the widget node. Extensions expose INTENT as
  registered actions and custom events instead; the bus docs state
  this division ("cell-drawn content is not in the semantic tree;
  expose intent as actions/events") so extension authors do not file
  it as a bus bug. Structured model export is likewise not a bus
  query — see reviews/study/platform-cycle3.md Q(c).
- **Redaction happens at the widget, never in the bus.** Query replies
  export exactly what widgets put in the semantic tree — the bus never
  filters. Consequence pinned with the app-kits track
  (reviews/study/platform-on-appkits.md F2): secret-bearing widgets
  (masked inputs) must mask `access_value` at the source
  (`src/widgets/input.rs:210` exports the raw value today), because
  this bus, the 0320 wire, and the 0330 bridge all republish the tree.

## Scope / Non-goals
Scope: the bus type, driver drain point, the six queries, event egress
(DropOldest ring), action descriptions, Resize-inject guard, example +
docs (incl. the canvas-opacity division).
Non-goals: any wire format (0320); parameterized/value-returning
invoke (deliberate v1 limit — `invoke_with` is the reserved v2 seam,
see Cross-track answers); recording/replay of sessions (a harness can
build it on inject+subscribe); a widget-level automation DSL ("click
the button labeled X" — controllers resolve targets from the semantic
tree themselves; a convenience resolver can come later if both ports
want it); interception/blocking of real user input.

## Feasibility
**v1-able.** No new mechanics: the drain point mirrors `pending`, the
queries wrap existing accessors, invoke wraps `Actions::run`, egress
rides 0300 + the ingest module's ring/overflow semantics pointed
outward (`OverflowPolicy::DropOldest`, `src/reactive/ingest.rs:55-63`
— settled cycle 3, replacing the earlier mpsc sketch which could not
express drop-oldest). The two design decisions to settle in review:
(a) does `inject` accept `ui::UiEvent` too (post-conversion vocabulary,
`src/app/events.rs:77-126` documents the lossy seam) or only
`input::Event` — recommend `input::Event` only, one entrance, the
conversion stays engine-owned; (b) `ScreenText` needs a
`Surface -> String` accessor — trivial (the VT model already proves the
shape, `VtScreen::to_text`, used by `examples/capture.rs:399-405`), but
decide whether it lives on `render::Surface` or stays driver-scoped.
Idle cost: an unused bus (no commands, no subscribers) is one empty
`Mutex<Vec>` checked per turn alongside the existing drains — zero
wakeups, zero allocations; not minted = zero everything.

## Expected outcomes
Integration tests drive real apps semantically (no escape-byte
scripting); 0320 becomes a serialization layer instead of a design
problem; embedding hosts get a supported control path; both port epics
(0200/0210) can be driven end-to-end by their own harnesses.

## Validation
- Unit: command ordering (bus commands drain before input dispatch or
  after — pick and pin), reply delivery, subscriber drop-counting,
  dead-bus inertness after `App` drop (the stale-handle discipline,
  `src/reactive/source.rs:33-36`).
- CaptureTerm acceptance: inject a key from a std::thread while the
  loop is parked → waker fires → event routes through a MODAL overlay
  first (proving handle_event parity); query SemanticTree with no
  overlays matches the root `accessibility_tree_text()` byte-for-byte,
  and WITH a modal open additionally contains the modal tree's entries
  (the composed-snapshot rule above); Resize inject refused on an
  `is_tty` terminal, accepted on CaptureTerm; invoke fires a
  registered action; subscribe sees Ready → Resized in order.
- Idle pins: a minted-but-quiet bus keeps `tests/adv_app.rs:54` and
  `tests/alloc_budget.rs:140` green.

## Progress checklist
- [ ] ControlBus type + driver drain point (phase U)
- [ ] inject via handle_event parity + is_tty Resize guard
- [ ] query set (composed tree/focused/viewport/actions/notices/screen
      text)
- [ ] invoke + action descriptions (dotted-namespace convention doc'd)
- [ ] subscribe egress (DropOldest ring, counted drops)
- [ ] headless-drive example + docs (canvas-opacity division included)
- [ ] idle-cost pins extended

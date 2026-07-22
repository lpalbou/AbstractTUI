# 0300 — App lifecycle events: named transitions + custom app events, subscribable

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: control-plane
- Consumed by: 0310 (bus event egress), 0320 (`subscribe`/`event` wire
  verbs), 0340 (Shutdown/SuspendPending flush triggers), 0350
  (Detached/Attached vocabulary) — this item is the track's foundation
  and sequences FIRST.
- Completed: N/A

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170/0050).
  ADR impact: none for this item alone; the event *vocabulary* becomes a
  wire contract once 0320 serializes it, so freeze names with 0320's ADR.

## Context
Every capability in this track — the automation bus (0310), the control
server (0320), persistence (0340), attach/detach (0350) — needs to answer
"what is the app doing right now, and tell me when that changes". Today
the engine HAS all the transition points but no vocabulary and no
subscription surface: an app cannot ask to be told "you are about to
suspend" or "the session is ending", and an external observer cannot ask
at all. The class-level justification is broad: dashboards pause polling
while suspended, editors flush drafts on shutdown, chat clients mark
presence on detach, kits (band 0500) gate wizard timers on ready — and
items 0310–0360 are direct consumers.

## Current code reality
Every lifecycle edge already exists as code, none as an event:
- **Boot/ready**: `Driver::new` enters the terminal session and emits the
  probe (`src/app/driver.rs:136-199`); the first `turn` renders the first
  frame from mount-time damage (driver.rs:134-135 doc). "Ready" =
  first-frame-presented is observable inside `render_frame`
  (driver.rs:325-400) but surfaces nowhere.
- **Suspend/resume**: `Terminal::suspend` (`src/term/mod.rs:176-190`,
  unix impl `src/term/unix.rs:610-634`) does leave → SIGTSTP → re-enter
  and documents caller obligations ("damage-all, re-query size(),
  re-apply verbs") — obligations an app can only meet if it HEARS the
  transition. Today nothing in `src/app/` even calls `suspend`; the verb
  is reachable only by apps holding the terminal themselves.
- **Resize**: `Driver::apply_resize` (driver.rs:508-533) poisons `prev`
  and republishes the viewport signal (`src/app/viewport.rs:50-55`) —
  the one transition that already has a reactive surface, and the
  pattern to copy.
- **Shutdown**: quit is a flag (`Quitter`, `src/app/mod.rs:74-80`;
  checked at driver.rs:288-297), teardown is `Driver::finish`
  (driver.rs:434-450, releases image slots then `term.leave()`) and
  `App::shutdown` (mod.rs:390-395, disposes the root scope). No
  "about to quit" moment exists for user code.
- **Capability upgrade**: `apply_caps_upgrade` (driver.rs:538-554) — the
  probe completing mid-session already changes presentation strategy;
  apps that gate features on caps (`App::startup_notices` one-liner,
  mod.rs:405-431) cannot re-read them today.
- **Prior art for the subscription shape**: the startup-notice store
  (`src/app/notices.rs:24-51`) and viewport signal
  (`src/app/viewport.rs:17-55`) — thread-local immortal-root signals
  with an app-internal publisher; mount-time readers subscribe, late
  publishes propagate. Exactly the mechanism this item generalizes.
- **Terminal focus in/out**: parsed (`Event::FocusGained/FocusLost`) and
  deliberately DROPPED at the routing seam
  (`src/app/events.rs:120-124`) — noted there as "wiring them to
  hover/focus policy is a widgets decision". A lifecycle surface is the
  natural non-widget home for them.

## Problem
The transitions are real, load-bearing, and mute. Consequences: apps
cannot flush state before quit (0340 has no safe hook), cannot pause
work while suspended/unfocused (battery/correctness), and the control
plane (0310/0320) has no event stream to publish. Custom app events
(one component telling another "job finished") are re-invented per app
via shared signals with no naming discipline.

## What we want
1. **A small, closed engine vocabulary** — `AppEvent` (name up to 0170's
   conventions) covering: `Boot` (session entered), `Ready` (first frame
   presented), `Resized(Size)`, `CapsChanged`, `FocusGained`/`FocusLost`
   (terminal-level, from events.rs's currently-dropped pair),
   `SuspendPending`/`Resumed` (around the suspend verb),
   `QuitRequested` (cancelable? see open question), `Shutdown` (last
   turn, before `Driver::finish`). Detach/attach names reserved for 0350
   (`Detached`/`Attached`) so the vocabulary ships once.
2. **One subscription surface, two idioms**: a reactive signal holding
   the latest transition (the viewport/notices pattern — components read
   it in `dyn_view`) AND a callback registry on `App` for non-rendering
   consumers (`app.on_lifecycle(fn(&AppEvent))`), which is what
   0310/0320 tap. Emission happens ONLY in phase U or the turn edges
   around it (never inside phases L..S — damage contract §1).
3. **Custom app events**: a namespaced channel (`app.emit("job.done",
   payload)` / subscribe by name) with the same phase discipline.
   Payload type: start with `String`/small POD to stay POD-honest; a
   generic typed channel is a possible extension, not v1.
4. **The suspend wiring**: an `App`-level suspend request (bindable to
   Ctrl+Z by apps) that emits `SuspendPending`, calls
   `Terminal::suspend`, and on return performs the residual
   obligations itself, then emits `Resumed`. Turns the trait's "caller
   must" contract into engine code with one obvious hook.
   **What "residual" means, exactly (extensions review P2-5,
   verified)**: the unix impl already re-enters WITH THE SAME OPTIONS
   internally (`leave → deliver_stop → enter(&opts)`,
   `src/term/unix.rs:610-634`), so everything `EnterOptions` carries
   (alt screen, cursor hide, mouse mode, bracketed paste, focus
   events, kitty flags) is restored by `enter()` itself. What
   genuinely remains driver/app-side after `suspend()` returns:
   (a) damage-all + poison `prev` (the alt screen came back blank);
   (b) size re-query (the window may have resized while stopped);
   (c) re-applying LATCHED session verbs outside `EnterOptions` —
   cursor style (unix.rs:636-645), title (unix.rs:647-654), pixel
   mouse (unix.rs:656-667). The driver owns (a)/(b); (c) needs the
   driver to remember which verbs the app set (a small latch mirror)
   or a `Resumed` contract line telling apps to re-apply their own —
   decide at implementation, but the list above is closed: do not
   double-apply what `enter()` already did.
5. **Docs**: a `docs/` page naming every event, its exact emission point
   in the phase sequence, and ordering guarantees.

## Scope / Non-goals
Scope: the vocabulary, both subscription idioms, custom events, suspend
wiring, emission-point tests, docs.
Non-goals: window-manager-grade lifecycle (no per-widget
mount/unmount events — scopes already own that,
`src/ui/tree.rs:318` "lifecycle is single-sourced in scopes"); no
async event queues; no cancelable-event framework beyond (at most)
`QuitRequested`; no cross-process delivery (that is 0320's job).

## Feasibility
**v1-able.** Every emission point exists and is single-threaded; the
subscription mechanics are proven in-repo (notices/viewport). The two
real design decisions: (a) whether `QuitRequested` is observable-only
or cancelable — cancelable interacts with the default-quit path
(`src/app/driver.rs:497-499`) and with `App::quit_requested` semantics;
recommend observable-only in v1 (apps that want to intercept quit
already can: consume the Ctrl+C event, driver.rs:495-497); (b) whether
`Ready` fires on first render or first *emitted* frame
(`Turn::emitted`, driver.rs:76-77) — recommend first presented frame,
it is the user-visible moment. Zero idle cost holds: events fire on
transitions only; an idle app emits none.

## Expected outcomes
0310/0320 get their event stream for free; 0340 gets its flush hook
(`Shutdown`, `SuspendPending`); apps stop hand-rolling "am I visible"
state; the suspend verb becomes actually usable from `App`-level code.

## Validation
- Unit: emission order per transition (Boot before Ready; Resized
  carries the post-resize size; SuspendPending → Resumed bracket).
- CaptureTerm acceptance: scripted resize/focus bytes produce the
  events in phase U of the following turn; a subscriber writing
  signals from the callback re-renders in the SAME frame (posted-job
  semantics, damage contract §2); idle turns emit zero events and zero
  bytes (extend `tests/adv_app.rs` pins).
- Suspend path (unix, live-pty ignored test): the existing
  `src/term/unix.rs:233-241` self-stop test shape, extended to assert
  the event bracket and the damage-all on resume.

## Progress checklist
- [ ] `AppEvent` vocabulary + emission points in Driver/App
- [ ] Reactive latest-transition signal (viewport pattern)
- [ ] Callback registry for non-rendering consumers
- [ ] Custom named events (emit/subscribe)
- [ ] Suspend request wiring + resume obligations moved engine-side
- [ ] FocusGained/FocusLost routed to the lifecycle surface
- [ ] Docs page + ordering tests + idle pins extended

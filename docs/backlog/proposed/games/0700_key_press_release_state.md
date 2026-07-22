# 0700 — Key press/release state: held keys as a first-class input fact

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: games (band 0700–0790)
- Completed: N/A
- Depends on: nothing HARD for the service itself — the wire data
  already arrives on env-claimed terminals (see Current code reality);
  this is a routing/service gap, not a decode gap. FIDELITY
  prerequisite: **first-app/0293** (kitty enter-flags never follow the
  probe) — on probe-proven terminals (iTerm2 ≥ 3.5, VS Code/Cursor,
  Warp) the flags are never pushed, so releases never reach the wire
  and this service would run repeat-approximated exactly where the
  protocol is available. Dependency chain (convergence cycle 2):
  **0293 (enable flags post-probe) → 0700 (expose state) → media-av
  0610 (consume)**.
- Cross-band consumers: MEDIA's push-to-talk item (media-av/0610 —
  hold-to-record is press/release verbatim; it consumes this service
  and adds no second key-state machinery); any hold-to-act UI
  (hold-to-confirm delete, scrub-while-held).
- Promotion trigger: the first real-time game example/dogfood, or
  MEDIA's voice capture surface reaching its push-to-talk phase —
  whichever moves first.

## ADR status
- Governing ADRs: ADR-0001 (additive only — this item deliberately adds
  a service + an opt-in rather than changing `ui::KeyEvent`'s shape; if
  a `kind` field on `ui::KeyEvent` is preferred instead, that is 0.3
  breaking-budget territory, planned/0002). ADR impact: none for the
  service design below.

## Context
Real-time cell games need "is Up held right now?" (move-while-held,
chorded diagonals Up+Right, stop-on-release). Push-to-talk needs "fire
on press, stop on release" for one key. Both are the same primitive: key
state over time, not key events. The engine is one seam away from having
it — the terminal layer already pays for the data.

## Current code reality
- The engine REQUESTS release visibility only on ENV-CLAIMED kitty
  terminals: enter-time flags are `DISAMBIGUATE | REPORT_EVENT_TYPES`
  (`KittyFlags::standard()`, src/term/options.rs:54-73), wired by
  `Driver::new` when env detection claims the protocol
  (src/app/driver.rs:163-171; the claim covers kitty/WezTerm/Ghostty/
  foot only, src/term/caps.rs:235). `EnterOptions::default()` itself
  carries `KittyFlags(0)` (options.rs:114). [Citation corrected in
  convergence cycle 2: the prior "wired as the full-screen default at
  options.rs:179" pointed at a TEST literal, and probe-proven
  terminals never get the push at all — see first-app/0293.]
- The parser decodes all three kinds: `KeyEventKind::{Press, Repeat,
  Release}` (src/input/mod.rs:227-239), with release tests at
  src/input/kitty.rs:144-151. `KeyEvent::is_down()` (press OR repeat)
  already exists input-side (src/input/mod.rs:349-352).
- The routing seam then DISCARDS it: `convert_event` returns `None` for
  every Release (src/app/events.rs:80-82) — a documented drop ("key
  RELEASE events (kitty; press/repeat dispatch)", events.rs:70) — and
  the ui vocabulary's `KeyEvent` carries no kind (events.rs:83-87), so
  press and repeat are indistinguishable past the seam.
- No key-state tracking exists anywhere in src/ui/ or src/app/ (grep
  for keys_down/key_state/held: zero hits, verified 2026-07-22).
- Legacy terminals structurally lack releases — "Legacy terminals only
  ever produce Press" (src/input/mod.rs:227-229); auto-repeat arrives
  as more presses at the OS repeat cadence.

## Problem
Held-key behavior is impossible at the app layer even on terminals that
report it, because the information is decoded and then dropped one seam
later. The workaround space is bad: apps cannot re-parse (the reader
owns the wire), and repeat-cadence inference from routed presses cannot
distinguish "held" from "tapping fast", nor see chords.

## What we want
1. **A key-state service** (suggested home `app::keys`), maintained by
   the driver from the PRE-conversion input stream (it sees releases
   even though routing drops them):
   - `keys_down() -> snapshot` of currently-held `ui::Key`s (converted
     vocabulary, locks stripped — same rules as events.rs:17-32);
   - edge callbacks or a generation-counted signal so a Dyn/effect can
     react to hold-start/hold-end without polling.
2. **Honest degradation on legacy wires** (the engine's standing
   principle): without release events, "held" is approximated by
   repeat-timeout — key enters the down-set on press, leaves when no
   repeat arrives within a tunable window (~2× typical OS repeat
   period). The service EXPOSES which fidelity is active (kitty-true vs
   repeat-approximated), the way `Capabilities` exposes everything else
   (src/term/caps.rs) — a game may choose tap-to-move on legacy wires.
3. **Optional release routing**: an opt-in (builder or shortcut flag)
   for widgets that want the release EVENT itself (push-to-talk's
   stop-recording), so the events.rs drop stays the default and no
   existing dispatch semantics change. Shortcut matching must keep
   ignoring releases by default (the existing rule, pinned at
   src/input/mod.rs:504-513).
4. Focus/terminal-focus hygiene: the down-set clears on FocusLost
   (Event::FocusGained/FocusLost already arrive, src/app/events.rs:
   120-124) — a key released while the terminal was unfocused must not
   stick down forever.

## Scope / Non-goals
Scope: the service, the legacy approximation, the opt-in release
routing, capability honesty, focus-clear.
Non-goals: changing `ui::KeyEvent`'s public shape (0.3 budget);
gamepad/joystick anything; key-mapping/rebinding vocabulary (app
policy); synthesizing releases the wire never sent beyond the labeled
timeout approximation.

## Expected outcomes
Move-while-held with chords works on kitty-class terminals and degrades
honestly elsewhere; MEDIA's push-to-talk consumes the same primitive;
no existing app changes behavior (additive service + default-off
routing opt-in).

## Validation
- Parser-level: already covered (kitty.rs release tests); add
  driver-level tests that the down-set tracks press/release across a
  scripted event sequence, including chords (Up+Right both down).
- Legacy approximation: scripted repeat cadence → down-set holds; gap
  past the window → key leaves the set (virtual clock).
- FocusLost clears the set; a release arriving with no matching press
  is a no-op (never panics).
- Fidelity flag matches the terminal's kitty capability in both modes.

## Progress checklist
- [ ] Design pass with the input/driver owner (where the pre-conversion
      tap lives)
- [ ] `app::keys` down-set + edges + fidelity flag
- [ ] Legacy repeat-timeout approximation (virtual-clock tested)
- [ ] Opt-in release routing for widgets
- [ ] FocusLost hygiene + tests
- [ ] docs: input section + capability note

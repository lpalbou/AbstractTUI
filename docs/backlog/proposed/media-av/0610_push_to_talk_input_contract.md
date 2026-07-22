# 0610 — Push-to-talk input contract (hold-to-record over key state)

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: games/0700 (key press/release state service) — this item
  deliberately adds NO second key-state machinery; it is 0700's first
  named consumer plus the voice-specific degradation policy.
- Dependency chain (convergence cycle 2): **first-app/0293 (kitty
  enter-flags never follow the probe) → 0700 → this item.** 0293 is
  the wire-level prerequisite: on probe-proven terminals (iTerm2 ≥ 3.5,
  VS Code/Cursor, Warp) the engine never pushes the kitty flags after
  the probe, so releases never arrive and hold-to-talk would degrade to
  latch mode exactly where the protocol exists. 0293 lands → 0700's
  fidelity flag reads kitty-true there → this item's Hold mode works,
  all with zero changes here.
- Promotion trigger: a voice-assistant/dictation app reaching its mic
  phase (the AbstractAssistant port class), together with 0700.

## ADR status
- Governing ADRs: ADR-0001 (additive only). ADR impact: none — app-level
  recipe + a small helper over 0700's service.

## Context
Every voice app has a capture gesture. The two shapes in production
clients today (read 2026-07-22, gateway study): open-mic VAD (the
assistant's `VoiceRecognizer`: webrtcvad segments speech automatically —
all app-side, no engine need) and push-to-talk (hold a key to record,
release to submit). PTT is the engine-relevant one because it needs
*release* events, which the terminal wire only carries under the kitty
keyboard protocol.

## Current code reality
- The engine requests release visibility on ENV-CLAIMED kitty terminals
  only (`KittyFlags::standard()` = `DISAMBIGUATE | REPORT_EVENT_TYPES`,
  src/term/options.rs:54-73; pushed by `Driver::new` when env detection
  claims the protocol, src/app/driver.rs:163-171 — kitty/WezTerm/
  Ghostty/foot per src/term/caps.rs:235). Probe-proven terminals never
  get the push today — first-app/0293. The parser decodes
  `KeyEventKind::{Press, Repeat, Release}` (src/input/mod.rs:227-239).
- The routing seam DROPS releases (`convert_event` returns `None`,
  src/app/events.rs:80-82) — games/0700 documents this exhaustively and
  specifies the `app::keys` down-set service, legacy repeat-timeout
  approximation, fidelity flag, and FocusLost hygiene.
- `Capabilities::kitty_keyboard` (src/term/caps.rs:39-40) is the honest
  fidelity source.

## Problem
Without a contract, every voice app re-derives the same three decisions:
which gesture on which wire, what happens on legacy terminals, and how
recording state stays truthful when focus is lost mid-hold (a stuck
"recording" state is a privacy bug, not just a UI bug).

## What we want
1. **A `PushToTalk` helper** (app-layer, over 0700's down-set): binds one
   `KeyChord`; emits `CaptureEdge::{Start, Stop(reason)}` where reason is
   `Released | FocusLost | Cancelled`. Stop-on-FocusLost is mandatory
   (mic must never keep running when the user cannot see the app).
2. **Honest degradation = LATCH mode**: when 0700 reports
   repeat-approximated fidelity (legacy wire), the same chord becomes
   toggle-to-talk (press starts, press stops) and the helper exposes
   `mode() -> Hold | Latch` so the UI can label the gesture truthfully
   ("hold Space" vs "press Space to start/stop"). Never fake a hold from
   repeat cadence for capture — a dropped repeat would stop recording
   mid-sentence.
3. **A visible state signal** (`Signal<CaptureState>`) so the meter
   (0620) and any recording indicator derive from one truth.

## Scope / Non-goals
Scope: the helper, the latch fallback, focus hygiene, docs section.
Non-goals: audio capture itself (app-side, see 0640), VAD (app-side),
global hotkeys (terminals cannot see unfocused keys — say so in docs).

## Expected outcomes
A voice app writes `PushToTalk::bind(cx, chord)` and gets correct
hold-to-talk on kitty-class terminals, a labeled latch everywhere else,
and a mic that always stops when focus leaves.

## Validation
- Driver-level scripted tests: kitty press/release bytes → Start/Stop;
  FocusLost mid-hold → Stop(FocusLost); legacy wire → latch toggling.
- Fidelity label matches `Capabilities::kitty_keyboard` in both modes.

## Progress checklist
- [ ] 0700 lands (dependency)
- [ ] PushToTalk helper + latch fallback
- [ ] FocusLost stop + tests
- [ ] docs: voice input section

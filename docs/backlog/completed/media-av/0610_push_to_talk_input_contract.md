# 0610 — Push-to-talk input contract (hold-to-record over key state)

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed)
- Track: media-av (band 0600–0690)
- Completed: 2026-07-23 (wave 3, INPUTAV) — `app::PushToTalk`:
  `bind(cx, chord)` + `on_start`/`on_stop(StopReason)` builders,
  `state() -> Signal<CaptureState>` (Idle/Held/Latched — the one truth
  for meters/badges/feeds), `mode() -> PttMode::{Hold, Latch}` +
  `gesture_label()` (both tracked: the label flips live at the 0293
  upgrade), `cancel()`. Consumes 0700's edge/state surface exactly as
  specified (no second key-state machinery). Mic-privacy rule shipped
  in BOTH modes: FocusLost stops capture (`StopReason::FocusLost`), and
  capture never auto-restarts when focus returns mid-hold (repeats
  re-prove the hold, a fresh press is required). Machine detail found
  by test: the effect acts on press RISING edges only — a mid-turn
  fidelity flip re-running the effect over the same sealed press edge
  used to double-toggle a fresh latch. Driver-level acceptance in
  tests/wave_inputav.rs (kitty press/release bytes → Held → Released;
  focus-out escape mid-hold → FocusLost; legacy wire → labeled latch);
  unit matrix in src/app/push_to_talk.rs. Latch caveat stated, not
  papered over: legacy auto-repeat arrives as more presses, so HOLDING
  the chord on a Degraded wire toggles repeatedly — the truthful label
  ("press … to start/stop") is the mitigation, never a synthetic
  release.
- Depends on: games/0700 (key press/release state service) — this item
  deliberately adds NO second key-state machinery; it is 0700's first
  named consumer plus the voice-specific degradation policy.
- Dependency chain (convergence cycle 2): **first-app/0293 (kitty
  enter-flags never follow the probe) → 0700 → this item.** 0293
  **SHIPPED in 0.2.2** (`Driver::apply_caps_upgrade` pushes the flags
  post-probe) and 0700 shipped this wave, so Hold mode works on
  probe-proven terminals (iTerm2 ≥ 3.5, VS Code/Cursor, Warp) exactly
  as the chain predicted — with zero changes here.
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
- [x] 0700 lands (dependency — same wave)
- [x] PushToTalk helper + latch fallback
- [x] FocusLost stop + tests (both modes; never-auto-restart pinned)
- [x] docs: voice input section (docs/api.md "app::PushToTalk",
      CHANGELOG; examples/voice_mock.rs is the living recipe)

## Post-completion addition (wave-3 cycle-3 close, CLOSER — 2026-07-23)

Cycle-2 review I-2 (`reviews/wave3/review-cycle2.md`): `Terminal::
suspend` bypassed key-state hygiene — a key released while the process
was STOPPED stayed in the down-set forever (no repeat corrects it;
Ctrl+Z keeps focus, so no FocusLost covers it), and a Held capture
would resume "recording" with the chord up. Shipped the suspend seam:
`Driver::suspend(app, term)` orchestrates keys-drain →
`Terminal::suspend` → resume re-sync (`driver_suspend.rs`);
`keys::on_suspend()` drains the down-set into synthesized releases and
flags the frame (`KeyState::suspend_cleared`, sealed per turn like
focus); PushToTalk stops with the NEW `StopReason::Suspended` in every
mode (Latch included — the flag fires with an empty down-set) BEFORE
the stop signal, and never auto-restarts on resume. The keys module
doc names suspend beside focus loss. Tests:
`suspend_drains_the_down_set_and_labels_the_frame` (keys),
`suspend_stops_a_hold_with_the_truthful_reason` +
`suspend_stops_a_latch_on_a_degraded_wire_too` (PTT),
`driver_suspend_drains_holds_stops_ptt_and_represents` (driver-level,
through `CaptureTerm`'s new in-memory suspend round trip).

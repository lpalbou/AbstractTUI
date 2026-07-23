# 0650 — `examples/voice-mock.rs`: the whole voice surface, no audio, no network

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed)
- Track: media-av (band 0600–0690)
- Completed: 2026-07-23 (wave 3, INPUTAV) — shipped as
  `examples/voice_mock.rs` (snake_case, matching the example set):
  push-to-talk on Space through 0610/0700 (HOLD on kitty-class
  terminals, labeled LATCH on legacy wires; the footer prints the
  truthful `gesture_label()` + the live `KeyFidelity` + whether the
  kitty keyboard is active via `use_caps`), a 30 ms timer-driven
  deterministic sine+hash-noise fake mic through `bounded_source`
  (`DropOldest`, window = the scope's ring) into the dB `Meter`, an
  8-band spectrum `Meter::bands`, and a rolling `AudioScope`; a fake
  transcription appends a word every ~360 ms into a `Feed` streaming
  item while "talking" and finishes the line with the stop reason. The
  synth interval exists ONLY while capturing (cancelled on stop) and
  the meters decay to their fixpoint — the quiet app is byte-for-byte
  idle, live. FocusLost stops capture (exercised in the smoke via the
  `CSI O` escape). Headless guard: exits 0 with a one-line notice when
  no tty. Joined the live pty smoke matrix (`live_voice_mock`:
  space/space/focus-out/q → exit 0, zero unknown sequences, terminal
  restored). SCOPE DELTA vs the draft, stated honestly: 0630 (speaking
  highlight over markdown) and 0640's `--mock-recorder` KillOnDrop flag
  are NOT included — both items remain Proposed and keep their own
  validation vehicles; this example validates the 0700 → 0610 → 0620
  chain it shipped with.
- Depends on: 0620 (meter/scope — SHIPPED same wave), 0610 (PTT —
  SHIPPED same wave, over games/0700), 0630 + 0640 (NOT consumed — see
  the scope delta above).
- Promotion trigger: lands as the VALIDATION VEHICLE of the voice items —
  the apps-as-validators principle (backlog overview) applied to media-av.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none (example only).

## Context
The voice items must be provable without microphones, speakers, TTS
engines, or a gateway — CI has none of those, and the maintainer should
see the whole surface by running one example. The production shapes to
imitate are known (grounding study 2026-07-22): mic levels arrive as
~30 ms f32 frames; playback meters arrive as N-band f32 frames; spoken
text advances word-by-word against a duration. All of that is
synthesizable from the frame clock.

## Current code reality
- `reactive::interval` (completed live-data 0070) + `animate` drive
  timed synthesis; `latest_source`/`bounded_source` carry the frames
  cross-thread exactly as a real recorder thread would.
- The dashboard example already fakes live series
  (examples/dashboard/data.rs) — the precedent for honest mock data.
- `tests/live_smoke.rs` runs every example under a real pty; the mock
  joins that matrix for free once it exists.

## Problem
Without a mock demo, the voice widgets would ship validated only by
unit tests — no end-to-end proof that meter ballistics, highlight
advance, and PTT state compose in a real frame loop, and nothing for a
maintainer to SEE.

## What we want
One example, three panels, zero external anything:
1. **Fake synthesizer**: a paragraph of text + a per-word schedule
   (chars/sec pacing); "speaking" advances a `Signal<Range<usize>>`
   through it, driving the 0630 highlight over rendered markdown.
2. **Fake mic**: a timer-driven level generator (noise + envelope on
   keypress-to-talk) feeding the 0620 `Meter` (single channel) and a
   synthesized 8-band frame feeding the band mode; `AudioScope` shows
   the rolling fake waveform.
3. **PTT surface**: hold/latch gesture (0610) toggles the "recording"
   state; the meter animates only while capturing; FocusLost stops it
   (assertable in the pty smoke by writing the focus-out escape).
4. Keys: space = talk (hold or latch per fidelity), `s` = speak the
   paragraph, `q` = quit. Footer names the input fidelity honestly
   ("hold-to-talk (kitty)" vs "toggle-to-talk (legacy wire)").
5. A `--mock-recorder` flag exercises the 0640 KillOnDrop pattern with
   `/bin/cat` as the stand-in child (unix; skipped honestly elsewhere).

## Scope / Non-goals
Scope: the example + its live-smoke case.
Non-goals: real audio, gateway calls, STT/TTS integration (that is an
APP, not an example — the boundary statement in 0640 applies).

## Expected outcomes
`cargo run --example voice-mock` demonstrates every voice primitive in
one screen; the smoke matrix proves it exits clean under a real pty.

## Validation
- live_smoke case: scripted `s`, space, focus-out escape, `q` → exit 0,
  zero unknown sequences, no panic text.
- The example doubles as the acceptance harness for 0620/0630 damage
  containment (assert via CaptureTerm variant in tests if the smoke
  proves flaky-prone).

## Progress checklist
- [x] Fake synthesizer + word schedule (fake transcription into a Feed
      streaming item; the 0630 markdown-highlight panel stays with 0630)
- [x] Fake mic levels + bands (deterministic sine + hash noise — no
      rand, no wall entropy; the dashboard precedent)
- [x] PTT wiring with fidelity label (hold/latch + Full/Degraded footer)
- [ ] KillOnDrop flag — NOT SHIPPED HERE: 0640's pattern keeps its own
      item and validation; adding a child-process flag to this example
      would blur the "zero external anything" claim it exists to prove.
- [x] smoke case (`live_voice_mock` in tests/live_smoke.rs, incl. the
      focus-out escape)

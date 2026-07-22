# 0650 — `examples/voice-mock.rs`: the whole voice surface, no audio, no network

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: 0620 (meter/scope), 0630 (speaking highlight), 0610 (PTT;
  degrades to latch in the demo when 0700 hasn't landed), 0640 (pattern).
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
- [ ] Fake synthesizer + word schedule
- [ ] Fake mic levels + bands
- [ ] PTT wiring with fidelity label
- [ ] KillOnDrop flag + smoke case

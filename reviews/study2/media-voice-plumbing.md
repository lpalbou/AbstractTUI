# MEDIA study 2 — voice/AV plumbing: what the engine owes a voice app

Date: 2026-07-22. Engineer: MEDIA. Read-only grounding pass over the
gateway's audio surface (`~/tmp/abstractframework/abstractgateway`) and
the assistant's client-side audio code
(`~/tmp/abstractframework/abstractassistant`, `abstractvoice`), then the
engine-agnostic design. Items filed: `docs/backlog/proposed/media-av/`
0610–0650 (band charter in its README).

## 1. The shapes voice apps actually handle (grounding facts)

**Gateway TTS, artifact lane** — `POST /runs/{id}/voice/tts`
(routes/gateway.py:8981): request `{text, provider?, voice?, profile?,
model?, format wav|mp3, speed?, quality_preset?, instructions?,
request_id?, timeout_s?}` (VoiceTTSRequest, gateway.py:8794-8812);
response names an audio ARTIFACT the client downloads and plays.
All-or-nothing default fill (a partial provider/voice pair passes
through untouched — the 2026-07-17 cross-provider voice-leak rule).
Server watchdog: `ABSTRACTGATEWAY_VOICE_TTS_TIMEOUT_S`, default 300 s.

**Gateway TTS, stream lane** — `POST /runs/{id}/voice/tts/stream`
(gateway.py:9176): **JSON-Lines events**, wav-only segments, each event
a dict with `request_id`; idle-gap watchdog emits a terminal error
event instead of a silent forever-stream (gateway.py:9271-9280).
Measured first-chunk latency ~3 s (AGENTS 2026-07-15 note); synthesis
is serial per segment.

**Gateway STT** — `POST /runs/{id}/audio/transcribe` (gateway.py:9595):
upload an audio artifact, get text; `{language?, provider?, model?,
prompt?, response_format?, temperature?}`. Word-level timestamps exist
engine-side only for whisper-class backends
(`abstractvoice/adapters/stt_faster_whisper.py`) and are not currently
surfaced through the route.

**Client-side playback/mic patterns** (the assistant, the closest thing
to a production voice TUI client):
- Playback: async-dispatch `speak()` on a worker thread; a
  stream-control triple (generation id + pause/stop `threading.Event`s)
  minted per utterance; artifact fallback when streaming is
  unconsumable; players are child processes (`afplay`/`paplay`/
  `ffplay`, `gateway_voice_manager.py:_spawn_player`) paused with
  SIGSTOP/SIGCONT, or an in-process sounddevice player.
- Meters: per ~33 ms audio chunk, RMS → one f32 level 0..1 AND
  log-spaced FFT band levels 80 Hz–6 kHz
  (`_emit_audio_meter_from_chunk`) — the callback receives `f32 |
  Vec<f32>`.
- Mic: `abstractvoice.recognition.VoiceRecognizer` — sounddevice input
  loop + webrtcvad segmentation (aggressiveness 0-3,
  `min_speech_duration`), `audio_level_callback(level 0..1)` with EMA
  smoothing (recognition.py:278-302), pause/resume/stop controls.

## 2. The boundary ruling this study proposes

**The engine never does audio I/O, HTTP, codecs, VAD, or synthesis.**
Everything in §1 is app-side (or a future `abstracttui-voice` sibling
crate per the extensions-track modularity item). What EVERY one of those
apps needs from the UI engine — and would otherwise re-derive — is
exactly four primitives plus one proof vehicle:

| Item | One-liner | Engine delta |
| --- | --- | --- |
| 0610 push-to-talk contract | Hold-to-record over games/0700's key-state service; LATCH (toggle) fallback + truthful gesture label on legacy wires; mic always stops on FocusLost | small helper over 0700; no new key machinery |
| 0620 `Meter` + `AudioScope` | Level bars with real ballistics (instant attack, frame-clocked decay, peak-hold, dB mapping, theme zones) + rolling waveform; data-driven via `latest_source`/`bounded_source` | two widgets over the chart substrate (chart.rs braille/eighth-block) |
| 0630 speaking highlight | Karaoke emphasis driven by `Signal<Range<usize>>` over rich/markdown text; third consumer of the 0148/0160 text↔cells mapping; selection-style ink swap + follow-scroll | the range→cells query (shared substrate) + a decorator |
| 0640 external-process pattern | Spawn recorder/player, stream via `bounded_source`, kill-on-drop via `Scope::on_cleanup` — **verified docs/example item, NOT engine code** (see §3) | zero |
| 0650 `examples/voice-mock.rs` | Timer-driven fake synthesizer + fake mic: levels, bands, word timings, PTT — no audio, no network; the validation vehicle for all of the above | example + live-smoke case |

Timing honesty baked into 0630: gateway TTS provides NO word alignment
(wav bytes only — verified), so TTS karaoke paces by estimation
(chars/sec against known duration); STT-sourced highlights can use real
whisper word timestamps. Both are app policy; the engine renders ranges.

## 3. The item-4 verification (mission asked: "verify and say so")

Claim to verify: live-data already covers the external-process pattern.
**Verdict: data plumbing yes, process lifetime no — docs item, not code.**
- Covered: `bounded_source` (src/reactive/ingest.rs:370 — bounded
  window, overflow policy, fold-panic firewall) carries audio chunks;
  `latest_source` (src/reactive/source.rs:210) carries levels; senders
  outliving their scope turn inert with counted dead sends — a reader
  thread may keep reading a dying process harmlessly.
- Covered: teardown hook exists — `Scope::on_cleanup`
  (src/reactive/scope.rs:117) runs at disposal; LIFO, children first.
- NOT covered (the gap): nothing kills the CHILD PROCESS on scope
  death; an orphaned recorder holds the microphone open — a
  privacy-grade leak, and the trap every first voice app will hit. The
  fix is ~12 lines of app code (a `KillOnDrop(Child)` guard registered
  on `cx.on_cleanup`); an engine type would not make it shorter and
  would drag platform process semantics into a UI crate. So 0640 is a
  documentation + example item, deliberately.

## 4. What stays out (rejected candidates, with reasons)

- **Audio decoding/resampling widgets**: byte→f32 conversion is DSP,
  not UI; the boundary is `Vec<f32>` into a source.
- **A VAD indicator widget**: it is a `Meter` with a threshold line —
  0620's zone tokens cover it; a dedicated widget would be the
  one-app-shaped API the general-needs-first principle forbids.
- **JSONL/SSE stream client**: transport is the live-data track's 0050
  decision (explicitly not-now); voice must not smuggle it in.
- **Global push-to-talk hotkeys**: terminals cannot see unfocused
  keyboards; promising it would be a capability lie. Documented in 0610.

## 5. Hardest open questions

1. **Latch-mode truthfulness on half-modern terminals**: terminals that
   support kitty keyboard WITHOUT event-type reporting (flags accepted
   but releases never sent) would make 0610 claim hold-to-talk while
   releases never arrive — the fidelity flag must derive from the
   PROBED flag response (`CSI ? flags u`), not from `kitty_keyboard`
   alone; needs a probe-side check during 0700's design pass.
2. **Meter decay clock vs idle frames**: decay animation requires
   frames while levels fall, but the engine's zero-idle-cost principle
   bills animations as frame requests — a meter left at -60 dB must
   reach a FIXPOINT and stop requesting frames (0620 must pin
   "silent meter = idle loop" in its acceptance tests, or voice apps
   ship a permanent 60 fps drain).

# media-av track (band 0600–0690) — OWNER: MEDIA

Media capabilities the engine owes applications, in two families:

- **Voice/AV plumbing (0610–0650)**: the UI-side primitives every
  TTS/STT app needs — push-to-talk input, level meters and scopes,
  progressive speech highlight, the external-process audio pattern, and
  a no-audio mock demo. The engine NEVER does audio I/O, HTTP, or
  synthesis: apps feed data through the existing live-data sources; the
  engine renders and routes input. Grounding study:
  `reviews/study2/media-voice-plumbing.md` (gateway/assistant shapes
  read 2026-07-22).
- **Image-path follow-ups (0660–0688)**: real findings from the study-2
  image-truth audit (`reviews/study2/media-images-truth.md`) that were
  out of fix-cycle scope.

| ID | Title | Family |
| --- | --- | --- |
| 0630 | Speaking-highlight primitive (offset-driven, shares 0148's text↔cells mapping) | voice |
| 0640 | External audio-process lifecycle pattern (docs + example; engine code NOT needed — verified) | voice |
| 0660 | Images inside `Feed`/content widgets via protocol placement | image |
| 0665 | Animated image sessions (kitty `a=f` frames; mosaic timer fallback) | image |
| 0670 | Cell-pixel-size refresh on resize (font zoom re-renders sixel/3D scale) | image |
| 0675 | Scroll-shift × image re-place (restore the scroll byte-win with live images) | image |
| 0680 | Sixel bottom-row/off-screen honesty (clamp + DECSET 8452 probe) | image |
| 0688 | Detection/transport robustness (strict probe reply parse; >1 MiB single-frame protocols under tmux) | image |

Completed 2026-07-22 (moved to `../../completed/media-av/`):

| ID | Title | Family |
| --- | --- | --- |
| 0685 | Probed-capabilities signal (`use_caps`) — discharged by first-app 0295 (one accessor, both consumers); the images example's channel label is truthful now | image |

Completed 2026-07-23, wave 3 INPUTAV (moved to `../../completed/media-av/`):

| ID | Title | Family |
| --- | --- | --- |
| 0610 | Push-to-talk input contract — `app::PushToTalk` over games/0700 (Hold on kitty-true fidelity, labeled Latch elsewhere, FocusLost always stops) | voice |
| 0620 | `widgets::Meter` + `widgets::AudioScope` — ballistics (instant attack, frame-clocked decay, peak hold), dB mapping, token zones; the idle fixpoint law test-pinned | voice |
| 0650 | `examples/voice_mock.rs` — the no-audio, no-network voice demo (0700/0610/0620 validation vehicle; 0630/0640 keep their own vehicles) | voice |

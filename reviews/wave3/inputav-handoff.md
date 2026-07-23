# Wave 3 — INPUTAV handoff (ledger rows for the overview writer)

Owner: INPUTAV. Audience: CONTENT2 (single overview.md writer for this
wave). Per the shared-file discipline I have NOT touched
`docs/backlog/overview.md`; the rows below are ready to fold verbatim.
Items are already moved: 0700 → `docs/backlog/completed/games/`,
0610/0620/0650 → `docs/backlog/completed/media-av/` (each carries its
full completion note + checklist in the file).

## Ledger rows

| ID | Title | Status | Completed | Shipped surface |
| --- | --- | --- | --- | --- |
| games/0700 | Key press/release state (held keys) | Completed | 2026-07-23 | `app::keys`: `use_key_state`/`key_state` → `KeyState` (`is_down`/`keys_down`/`pressed`/`pressed_chord`/`released`/`focus_cleared`), `KeyFidelity::{Full,Degraded}`, `hold_gesture_label`; driver pre-conversion tap, per-turn edge sealing, fidelity re-published at the 0293 probe upgrade |
| media-av/0610 | Push-to-talk input contract | Completed | 2026-07-23 | `app::PushToTalk` (`bind`/`on_start`/`on_stop(StopReason)`/`state()`/`mode()`/`gesture_label()`/`cancel()`); Hold on Full fidelity, labeled Latch on Degraded, FocusLost stops capture in every mode |
| media-av/0620 | Meter + AudioScope widgets | Completed | 2026-07-23 | `widgets::Meter` (ballistics: instant attack, frame-clocked decay 20 dB/s default, peak hold ~1.5 s; dB mapping; mono h/v + band bars; ok/warn/error token zones) + `widgets::AudioScope` (braille strip over a `Signal<Vec<f32>>` window); THE IDLE LAW pinned (fixpoint drops the frame task; zero frames + zero allocs on unchanged input) |
| media-av/0650 | voice mock example | Completed | 2026-07-23 | `examples/voice_mock.rs` (PTT on Space + truthful fidelity footer, fake mic → dB meter + 8-band spectrum + scope, fake transcription into Feed) + `live_voice_mock` smoke case |

## Count deltas (if the overview tracks them)

- games: proposed 4 → 3; completed 0 → 1 (`docs/backlog/completed/games/` is new).
- media-av: proposed 11 → 8; completed 1 → 4.

## Chain note (already recorded in the item files)

first-app/0293 (kitty flags follow the probe, shipped 0.2.2) →
games/0700 → media-av/0610 — the convergence-cycle-2 chain landed
end-to-end this wave; 0700's fidelity flips Degraded→Full live at the
probe-upgrade moment and 0610's gesture label follows it.

## Scope deltas the ledger should not mis-credit

- 0700's "legacy repeat-timeout approximation" was DROPPED by ruling
  (0610's never-fake-releases capture rule generalized); "opt-in
  release routing for widgets" is deferred with reasons in the item.
- 0650 does NOT consume 0630 (speaking highlight) or 0640
  (`--mock-recorder`); both remain Proposed with their own validation.

## Shared-file appends made this wave (for merge awareness)

- `src/widgets/mod.rs`: `pub mod audio_scope; pub mod meter;` +
  re-exports + two entries in the color-lint `SOURCES` list (count
  26 → 28 — if you also add widget files, the array length literal is
  the collision point).
- `src/app/mod.rs`: `pub mod keys; pub mod push_to_talk;` + re-exports.
- `src/prelude.rs`: appended key-state/PTT/meter exports at the end.
- `docs/api.md`: three sections appended at the END of the file.
- `CHANGELOG.md`: `## [Unreleased]` section added above 0.2.2 (append
  into it if you create entries).
- `tests/alloc_budget.rs`: one new idle-pin test appended (REDTEAM's
  binary; the existing pins are untouched).
- `tests/live_smoke.rs`: one `live_voice_mock` case appended.
- `src/reactive/animate.rs` + `mod.rs`: `register_frame_task` widened
  to `pub(crate)` (meter ballistics consumer); the PUBLIC frame-task
  surface remains games/0710's decision — deliberately not preempted.
- `src/app/events.rs`: `convert_key` widened to `pub(crate)` (shared
  identity vocabulary with the key-state tap).
- `src/app/driver.rs`: three insertions — fidelity publish in
  `Driver::new` + `apply_caps_upgrade`, `keys::begin_turn()` in phase
  U, pre-conversion tap at the top of `handle_event`.

## Cross-lane note (one peer line touched, minimal, gate-unblocking)

`src/widgets/image.rs:422` (fresh edit this wave, `from_path_decodes_a_
real_file_through_the_unified_decoder`) used `Rgba::rgb(` in test code,
failing the directory-wide color lint
(`no_color_literals_or_arithmetic_in_widgets`) for the WHOLE tree. Fixed
minimally with the file's own documented precedent (`stripes()`:
token-rule-safe constants only) — the test's routing purpose is
unchanged. Whoever owns that test: keep bitmap probes on
`Rgba::WHITE/BLACK/TRANSPARENT` or move pixel-variety probes out of
`src/widgets/`.

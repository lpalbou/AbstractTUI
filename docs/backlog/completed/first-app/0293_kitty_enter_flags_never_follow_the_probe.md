# Completed: kitty enter-flags never follow the probe — Shift+Enter dead on iTerm2/VS Code/Warp

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed — capability-truth defect, first-app finding, composer wave)
- Completed: 2026-07-22 (first-app fix wave, cycle 3)

## ADR status
- Governing ADRs: None. ADR impact: none — capability lifecycle behavior.

## Context
`abstractcode-tui`'s composer teaches the chat convention (Enter sends,
Shift+Enter newline). Shift+Enter is only REPORTABLE under the kitty
keyboard protocol, and the engine already parses `CSI 13;2u` correctly —
but the protocol is never switched on for the terminals most macOS users
actually run.

## Current code reality
- `Driver::new` decides the enter-time flags push ONCE from env-detected
  caps (src/app/driver.rs:163-171: `kitty_keyboard: if caps.kitty_keyboard
  { KittyFlags::standard() } else { KittyFlags(0) }` — the conditional at
  driver.rs:165-169; verified convergence cycle 2).
- Env detection claims the protocol only for kitty/WezTerm/Ghostty/foot
  (src/term/caps.rs:235).
- The active probe (`CSI ? u`) later PROVES the protocol on any reply —
  `on_reply` sets `caps.kitty_keyboard = true` (src/term/probe.rs:133-138)
  — but nothing re-applies the enter flags after the probe. The push at
  enter time already happened with `KittyFlags(0)`.
- Consequence: iTerm2 ≥ 3.5, VS Code / Cursor terminals, and Warp all
  support the protocol and answer the probe, yet never receive
  `CSI > flags u` — Shift+Enter stays a plain `\r` for them under any
  abstracttui app.
- The inverse defect on the same lines: WezTerm is env-CLAIMED
  (caps.rs:235) but ships `enable_kitty_keyboard = false` by default, so
  the claim (and any capability-derived help text) over-promises exactly
  where the user didn't configure it.

## Proposed direction (engine's call)
1. On a probe transition of `kitty_keyboard` false→true, emit the
   standard flags push at that moment and register the exit restore
   (the same pair `Driver::new`/exit already manage).
2. Gate the CLAIM on evidence for terminals whose support is config-off
   by default: env may seed the probe attempt, but `kitty_keyboard`
   should read true only after the probe reply (WezTerm case). Terminals
   the env asserts AND that answer the probe are unaffected.

## App-side state
`abstractcode-tui` ships an app-side universal fallback (Ctrl+J inserts a
newline) and capability-neutral help text; when this lands, Shift+Enter
starts working on the majority macOS terminals with zero app changes.

## Downstream consumers (convergence cycle 2, 2026-07-22)
This item is the PREREQUISITE of the key-state chain: **0293 (enable
flags post-probe) → games/0700 (key press/release state service) →
media-av/0610 (push-to-talk)**. Without the post-probe flags push,
`REPORT_EVENT_TYPES` (the release-visibility bit riding the same
`KittyFlags::standard()` push as the Shift+Enter disambiguation,
src/term/options.rs:54-73) never reaches probe-proven terminals — so
0700's down-set would run repeat-approximated, and 0610 would fall back
to latch mode, exactly on the terminals that DO speak the protocol.
Fixing this item upgrades both downstream items on iTerm2 ≥ 3.5,
VS Code/Cursor, and Warp with zero changes on their side. The exit
restore for a post-probe push must pop the SAME entry the enter path
pops (`leave_bytes` emits `CSI < u` only when enter pushed,
options.rs:149-151 — a probe-time push needs its own pop bookkeeping).

## Completion report

- Completed: 2026-07-22 (first-app fix wave, cycle 3). Both proposed
  directions shipped.
- **Direction 1 — the post-probe flags push.** New Terminal verb
  `Terminal::set_kitty_keyboard(flags)` (defaulted honest refusal;
  implemented by `UnixTerminal`, `WindowsTerminal`, and
  `testing::CaptureTerm`): emits the exact delta (`CSI > flags u` push /
  `CSI < u` pop / pop-then-push on change, byte builders shared with
  enter/leave via `KittyFlags::push_bytes`/`POP_BYTES` — the
  `MouseMode::arm_bytes` no-drift rule) and updates the ENTERED SESSION
  OPTIONS, so the accounting is structural: `leave` derives its pop from
  the same options (exactly one pop per live push), and job-control
  `suspend` — which internally leaves and re-enters with those options —
  pops before teardown and re-pushes after re-enter with zero extra
  state. The panic-hook emergency restore gets the pop PREPENDED
  (`prepend_emergency_leave`) so it runs while the alternate screen is
  still active — kitty flag stacks are per screen buffer; a pop after
  `?1049l` would hit the main screen's stack.
  `Driver::apply_caps_upgrade` (both completion paths: sentinel and tmux
  grace expiry) pushes `KittyFlags::standard()` when the probe proved
  `kitty_keyboard` on a session whose DERIVED enter carried no flags,
  and flushes immediately (a kitty-only upgrade may not render a frame
  that turn). Explicit `RunConfig::enter` postures are the embedder's
  own and are never upgraded (`Driver::kitty_auto`). A refused push
  (scripted terminal without session tracking) degrades to a labeled
  startup notice, never silence.
- **Direction 2 — the WezTerm over-claim.** `Capabilities::detect_env`
  no longer asserts `kitty_keyboard` for WezTerm (ships
  `enable_kitty_keyboard = false` by default); the claim is now
  evidence-gated — the probe's `CSI ? u` raises it, and direction 1 then
  pushes the flags. kitty/ghostty/foot (protocol on by default) keep the
  env claim and the enter-time push, unaffected.
- Tests: `tests/wave_probe_caps.rs::
  probe_proof_pushes_kitty_flags_then_shift_enter_works_and_finish_pops`
  (CaptureTerm byte assertions: no push at enter, `CSI > 3 u` after the
  probe reply folds, `kitty_push_depth` 1 → 0 across finish; Shift+Enter
  `CSI 13;2u` reaches the app as Enter+SHIFT once flags are live),
  `explicit_enter_posture_is_never_upgraded`, and the pty-level
  `pty_runtime_kitty_push_pops_on_suspend_repushes_on_resume_and_pops_on_leave`
  (src/term/unix_tests.rs: runtime push → suspend pops inside teardown /
  resume re-pushes inside re-enter → leave pops exactly once;
  idempotent re-set emits zero bytes). WezTerm gate pinned in
  `term::caps::tests::iterm_and_wezterm_prefer_iterm_images`.
- Downstream: unblocks the key-state chain (games/0700 gets
  `REPORT_EVENT_TYPES` on probe-proven terminals; media-av/0610's
  push-to-talk inherits it) with zero changes on their side.
  `abstractcode-tui`'s Ctrl+J app-side fallback can stay (harmless) —
  the engine now also ships Ctrl+J-inserts-newline in `TextArea`
  (0295's nice-to-have), so the app workaround is deletable.

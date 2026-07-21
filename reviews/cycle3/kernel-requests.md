# KERNEL cycle 3 — requests + contracts to other owners

## To REDTEAM

1. **`Terminal` grew four defaulted methods this cycle** — trait still
   STABLE, `CaptureTerm` compiles unchanged, but two deserve real impls
   on your side:
   - `is_tty() -> bool` (default `false`): please add a scripted value
     (`set_is_tty(bool)`-style) so splash-gate tests can exercise both
     branches. Default-false means existing capture tests auto-skip
     splash logic, which is the right conservative default.
   - `suspend()`: default `Err(Unsupported)` — fine for CaptureTerm, no
     action unless you want to script it.
   The verb methods (`set_cursor_style`/`set_title`/`clipboard_copy`/
   `bell`/`notify`) are byte-writers with defaults; CaptureTerm records
   their bytes through `write()` already.
2. **VtScreen sequence gaps** (one-liners, snapshot quality only — my
   tests assert raw bytes meanwhile): DECSCUSR (`CSI Ps SP q`) currently
   lands in `unknown_seq_count`; OSC 52 and OSC 9 fall to the OSC
   `note_unknown` arm; XTWINOPS 22/23 (title stack) likewise. If you
   model them (even as counters: `cursor_style()`, `clipboard_writes()`,
   `title_stack_depth()`), my leave-balance tests can move from raw-byte
   greps to model assertions, and your enter/leave balance suite can pin
   "every style set has its reset" the way it pins DECSET/DECRST.
3. **Suspend's kill line is untestable in-process** — `kill(0, SIGTSTP)`
   targets the process group, i.e. cargo and your harness too. It is
   `cfg(test)`-seamed to a no-op (`UnixTerminal::deliver_stop`), so the
   pty test pins the full restore→stop-point→re-enter byte order but the
   signal itself never fires under test. If the rig ever grows a
   subprocess harness (spawn a child TUI, TSTP it for real, CONT it),
   this is a named candidate — until then the 2-line kill is correct by
   inspection and flagged as such in my report.
4. **ConPTY deferred-wrap thread: CLOSED as "impossible on this host",
   not as "done".** Three cycles, no Windows machine. Status + flip
   procedure are now in `docs/design/term-input.md` §5. The standing ask
   converts to: the day ANY Windows host (CI or manual) exists, the
   verification run is the first thing scheduled on it — please keep the
   line item on your ledger so it cannot silently evaporate; I will
   re-file it every cycle the host does not exist.
5. RT2-5 tripwire re-confirmed after cycle 3: still no `\x1b[6n` (DSR 6)
   emission anywhere in term/input — the new verbs emit DECSCUSR/OSC
   0/52/9/BEL and XTWINOPS 22/23 only.

## To REACT

1. **Suspend wiring**: bind Ctrl+Z (a plain `Key(Char('z'), CTRL)` in
   raw mode — the tty driver no longer does it) to `Terminal::suspend()`.
   On return: damage-all, re-query `size()`, re-apply cursor style/title
   if your app set them. The verb blocks until SIGCONT; run it from the
   loop thread (it tears the terminal down — nothing else may write
   mid-suspend).
2. **Splash gate**: `term.is_tty()` is now the honest surface (DESIGN
   request 6 delivered); `have_tty()` remains for the pre-construction
   check. Both ask the render handle, not stdout.
3. Stale mapping reminder from your cycle-2 request 1: `app::driver::
   present_caps_from` maps `undercurl`/`underline_color` to `false` —
   both fields have existed on `term::Capabilities` since cycle 2, and
   `caps.present_caps()` fills them; the two-line update on your side is
   still pending as of this filing.

## To DESIGN

1. `is_tty()` landed on the trait (your request 6); `bell()`/`notify()`
   exist for boot/error chrome — `notify` gates on
   `Capabilities::osc9_notify`, fall back to `bell()`.
2. Title for the splash: `set_title` pushes the title stack once and
   leave restores the user's title automatically — safe to brand the
   window during boot without cleanup code on your side.

## To GFX3D

1. `term::tmux_wrap(payload)` exists (ESC-doubled `ESC Ptmux; … ESC \`)
   and `Capabilities::needs_tmux_passthrough` tells you when it would be
   required — but graphics remain force-disabled under tmux because
   `allow-passthrough` (off by default since tmux 3.3a) is undetectable
   from env. Do NOT wrap-and-hope: a verified-passthrough probe (active
   round trip through the wrapper) is the cycle-4 unlock if you want
   images under tmux; say so in your requests if it matters and I will
   prioritize it.

## To the integrator

Nothing needed in base or Cargo.toml. New public surface this cycle:
`term::{CursorStyle, tmux_wrap}` + five trait verbs + `is_tty`/`suspend`
+ four `Capabilities` fields (`osc52_copy`, `osc9_notify`,
`needs_tmux_passthrough`, `tmux_version`). Prelude candidates when the
dust settles: `CursorStyle` only (the verbs ride the trait).

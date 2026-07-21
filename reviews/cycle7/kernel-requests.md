# KERNEL cycle 7 — RT5-1 verdict + requests

## RT5-1 VERDICT: REAL platform bug (Darwin), NOT a harness artifact —
## fixed, acceptance green

Decisive evidence (`src/term/rt5_live_tests.rs`, self-spawned pty child
with setsid + TIOCSCTTY — a PERFECT controlling terminal by construction,
eliminating the harness-ctty hypothesis):

- `alias_open=ok` — /dev/tty opens (ctty exists), yet
  `alias_poll revents=0x20 nval=true` with a byte already queued:
  **Darwin's poll(2) rejects the /dev/tty ALIAS device itself.** Your
  harness was fine; your finding was right.
- Second trap found while fixing: `ttyname(/dev/tty fd)` on Darwin
  answers the literal string `/dev/tty` — CIRCULAR, so "resolve the
  alias" is not a fix. Resolution must start from a REAL tty fd:
  `stdin_resolves_to=/dev/ttys015`, `real_poll pollin=true`,
  `real_read=Z` — the resolved device polls and delivers.

The fix (unix.rs, acquisition rewritten):

1. stdin+stdout both ttys → use directly (the real-terminal launch path;
   never touches the alias).
2. Any single std fd is a tty (redirection cases) → `ttyname_r` that fd,
   open the real device fresh — `echo x | app` keeps interactivity.
3. `/dev/tty` alias only as a last resort (all three std fds redirected).

Plus the structural guarantee shipped REGARDLESS of root cause:
**keyboard-dead is impossible** — if poll ever reports POLLNVAL on the
terminal fd, the read loop falls back once to stdin-as-tty with a
labeled degradation (`UnixTerminal::degraded() -> Option<&'static str>`),
else fails LOUDLY. The fallback was live-exercised: before the
acquisition rewrite, the engine child delivered keystrokes THROUGH the
fallback (`ENGINE: got=x`, `degraded=…POLLNVAL…`); after it, the primary
path is healthy (`degraded=none`) — both states observed in real runs.

## To REDTEAM

1. **Acceptance green**: `RT5_1=1 cargo test --test live_smoke --
   live_ctty_input_reaches_app --ignored --exact --nocapture` →
   `hello-ctty: exit=0 bytes=6667`, clean assert. The gate can be removed
   and the test promoted into the regular live suite — your file, your
   call. Kernel-side permanent twins run in the NORMAL suite:
   `term::rt5_live_tests::{devtty_alias_vs_real_device_poll,
   engine_keystrokes_flow_on_ctty_path}` (skip gracefully without a pty).
2. Your characterization child (`rt5_1_child_poll_devtty`) polls the
   alias in COOKED mode — fine for the POLLNVAL verdict (it fires either
   way), but if you extend it: raw-mode the tty first or queued bytes sit
   in the line discipline and muddy readable-vs-pollable (my probe does
   `cfmakeraw` for that reason; same lesson as the tmux rig's stty raw).
3. Your `ctty=true` harness path needed NO fix — setsid+TIOCSCTTY in
   `src/testing/pty.rs` is exactly right and the finding stands as filed.
   One nit: `pre_exec` does `ioctl(0, TIOCSCTTY)` — correct since stdin
   IS the slave; my rig targets the slave fd explicitly for the same
   effect.
4. **MouseEvent is `#[non_exhaustive]` as of this cycle** (your ui-typed
   MouseEvent literals in adv_pointer/adv_reactive/adv_widgets are
   REACT's type — unaffected). **KeyEvent's flip is HELD one more
   cycle**: `tests/adv_splash.rs:138` now uses FRU
   (`KeyEvent { kind: …, ..KeyEvent::plain(…) }`) — good against field
   adds, but `#[non_exhaustive]` FORBIDS downstream FRU too, so flipping
   now would break you again. Swap to
   `KeyEvent::key(KeyCode::Enter).with_kind(KeyEventKind::Release)` (or
   `.plain(..).with_kind(..)`) and I flip next cycle. Pattern-match `..`
   sites (adv_input.rs) are already non_exhaustive-proof.

## To REACT

1. **The undercurl saga: verified CLOSED this cycle** — driver.rs now
   maps real caps and even carries a regression test
   (`undercurl capability must reach the presenter`). Thanks; withdrawn
   from my ledger.
2. `UnixTerminal::degraded()` exists (concrete type, not on the trait):
   if you surface one diagnostics line at startup, this is the string to
   show — it is `Some(..)` only when the terminal fd was un-pollable and
   the reader fell back to stdin.
3. Acquisition-order change heads-up (no action expected): interactive
   apps under redirection now resolve the real device instead of the
   /dev/tty alias; `echo x | app` still gets full interactivity — plus
   keystrokes now actually arrive on macOS default paths (RT5-1).

## To the integrator

- Unsafe additions stay within the sanctioned boundaries: unix.rs FFI
  (ttyname_r/open/isatty) + the pty-child ctty setup in the two live-test
  rigs (setsid/TIOCSCTTY with SAFETY comments, mirroring testing/pty.rs).
- New public surface: `UnixTerminal::degraded()`; `MouseEvent` is now
  `#[non_exhaustive]`. KeyEvent flip queued for cycle 8 (one downstream
  FRU site).

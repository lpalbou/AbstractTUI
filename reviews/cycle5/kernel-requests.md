# KERNEL cycle 5 — requests + verification results

## Live tmux verification — VERIFIED (the cycle-4 risk #1 is closed)

No tmux existed on this host; installed via Homebrew (tmux 3.7b —
disclosed system-tooling change, no Cargo dependency involved). The rig
(`src/term/tmux_live_tests.rs`, ignored-by-default, skips without a tmux
binary or inside a tmux session) makes the TEST the outer terminal: a pty
pair whose slave becomes the controlling tty of a real
`tmux -f <cfg> -S <socket> new-session`, with an inner `sh` emitting the
same wrapped kitty query shape `ActiveProbe` sends. Results (tmux 3.7b):

1. **allow-passthrough on**: the wrapped query emerged on the outer tty
   UNWRAPPED (`ESC _Gi=9999…`, wrapper stripped, doubled ESCs undone; no
   `ESC Ptmux;` leak). 797 outer bytes captured.
2. **defaults (passthrough off)**: swallowed entirely — no kitty APC and
   no wrapper on the outer tty (829 bytes of ordinary tmux redraw).
3. **Reply routing**: an APC reply written INTO the outer tty routed to
   the pane process byte-exact (`\x1b_Gi=9999;OK\x1b\\` captured inside
   the pane) — the full chain the cycle-4 probe design rides is now
   live-proven, not folklore.

Two harness traps cost most of the debugging time and are recorded in
the rig for the next reader: the pane tty starts CANONICAL (the reply
sat invisible in the line discipline until the inner shell ran
`stty raw`), and `cat`'s stdio buffer died unflushed on kill-server
SIGHUP (0-byte captures; `dd` writes per-block and fixed it). Before
those fixes the reply half honestly read NOT ROUTED — if you ever see
that result on a future tmux, check the harness before blaming tmux.

Still scripted-only (named in §3.5): the FIFO attribution of direct-vs-
wrapped XTVERSION replies (first = tmux, second = outer). The live rig
proves transport, not reply ORDER under load.

## To REDTEAM

1. **RT4-2 closed for kernel files**: the 4 named `input/parser.rs`
   clippy warnings are fixed (two `needless_range_loop` →
   `extend_from_slice`, one `wrong_self_convention` — `to_ground` renamed
   `reset_to_ground` — one `needless_return`), plus 2 in `term/unix.rs`
   the scoped scan surfaced (`manual_c_str_literals` → `c"/dev/tty"`).
   Kernel clippy is ZERO on native, `--target x86_64-pc-windows-msvc`,
   and `--tests`.
2. **RT4-3**: the Windows-clippy command is documented in term-input.md
   §5; windows-target kernel warnings: none found, none suppressed.
3. The live tmux rig is yours to adopt/absorb if you want it under the
   rig's process-spawning umbrella — it deliberately lives in my tree
   today (kernel-owned premise), guarded and ignored-by-default so suite
   runs stay hermetic. tmux 3.7b is now on this host for any future
   passthrough regression runs.

## To REACT

1. **poll_many adoption — second ping** (cycle-4 filing, verified still
   open): `app/driver.rs` drains with three `poll_event` call sites
   (lines ~215/360/376 at this reading). The drain loop at 215 is
   exactly `poll_many`'s shape — one call, batch out, same `Ok(0)`
   semantics as `None`. Not urgent (the queue already amortizes
   syscalls), but the per-call empty-confirmation read it pays per event
   is the thing poll_many removes. Your court; third ping only if a
   bench ever shows it.
2. **`Terminal::notify` signature changed** (cycle-3 API, no consumers
   found outside kernel): now `notify(message, channel)` with
   `NotifyChannel` from `caps.notify_channel()` — kitty gets OSC 99
   (it never adopted OSC 9), iTerm2/WezTerm/ghostty get OSC 9, everything
   else gets the bell. One channel per call: ghostty speaks both dialects
   and would double-notify if we sprayed.
3. Cycle-3/4 reminder, still open: `app/driver.rs::present_caps_from`
   hardcodes `undercurl: false`; `caps.present_caps()` fills it.

## To the integrator

- New public surface: `term::NotifyChannel`, `Capabilities::
  {osc99_notify, notify_channel()}`; `Terminal::notify` gained the
  channel parameter (breaking for any out-of-tree caller; none in-tree
  outside kernel).
- System note: Homebrew-installed tmux 3.7b for live verification (test
  tooling; no manifest change).

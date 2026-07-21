# term + input — design notes (cycle 1)

Owner: KERNEL. Scope: `src/term/**` (platform terminal I/O, capability
detection) and `src/input/**` (byte stream -> events). This document records
the research conclusions, the decisions taken, and the alternatives rejected.

## 1. Research: how the reference stacks do it

### 1.1 Raw mode + tty acquisition

- **crossterm** uses stdin when `isatty(STDIN_FILENO)` holds, otherwise opens
  `/dev/tty` read+write; raw mode = `tcgetattr` + `cfmakeraw`-equivalent +
  `tcsetattr(TCSANOW)`, saving the original termios in a global so a second
  `enable_raw_mode` is a no-op and restore is exact.
  (github.com/crossterm-rs/crossterm: `src/terminal/sys/unix.rs`,
  `src/terminal/sys/file_descriptor.rs`, PR #957 "fall back to /dev/tty".)
- **termwiz** `UnixTerminal::new()` opens `/dev/tty` outright; `new_from_stdio`
  duplicates stdin/stdout fds and fails unless both are ttys. Restore is tied
  to the instance (saved termios), not a global.
  (github.com/wez/wezterm: `termwiz/src/terminal/unix.rs`.)
- **notcurses** keeps the tty fd inside the context and restores on stop; it
  treats the signal layer as process-global (see 1.2).

**Decision (rewritten cycle 7 after RT5-1, live-proven).** Acquisition
order in `UnixTerminal::new()`:

1. stdin+stdout both ttys → use them directly (how every real terminal
   launches apps).
2. ANY std fd is a tty (pipes on the others: `echo x | app`) → resolve
   that fd's REAL device via `ttyname_r` and open it fresh — interactive
   apps keep their terminal under redirection.
3. The `/dev/tty` alias only as a last resort (all three std fds
   redirected).

The alias is avoided because **Darwin cannot `poll(2)` the `/dev/tty`
alias device**: POLLNVAL even with input queued and a perfect controlling
terminal, and `ttyname` of the alias circularly answers `/dev/tty`
(both live-pinned in `term::rt5_live_tests` under a setsid+TIOCSCTTY
child — RT5-1's root cause; crossterm's select()-based unix source is the
same bug worked around downstream). Raw mode is done manually (all
`cfmakeraw` flag edits are spelled out, see `unix.rs`) with `VMIN=0,
VTIME=0` — reads are gated by `poll(2)`, never by termios timing, so a
spurious poll wake cannot block us in `read(2)`. The saved termios lives
in the instance for symmetric restore AND in a process-global slot for
the panic/emergency path (see 1.9).

**Keyboard-dead is structurally impossible**: if the read loop's poll
ever reports POLLNVAL on the terminal fd — any platform, any reason — it
falls back once to stdin-as-tty with a labeled degradation
(`UnixTerminal::degraded()`), and with no candidate it returns a loud
error. Silence is the one forbidden outcome.

### 1.2 Resize detection without signal-handler races

- **crossterm** registers SIGWINCH through signal-hook (mio backend) or a
  nonblocking socketpair self-pipe (`tty` backend) and polls the pipe next to
  the tty fd. (crossterm `src/event/source/unix/tty.rs`, PR #735.)
- **notcurses** installs a `sigwinch_handler` that only sets an atomic and
  writes one byte to a readiness pipe; crucially, their docs state that POSIX
  signals are *unreliable* (delivery may coalesce or be missed), so the
  authoritative geometry is always re-read with `ioctl(TIOCGWINSZ)` at render
  time; the signal is merely a wakeup. (notcurses `src/lib/in.c`,
  `src/lib/unixsig.c`, discussion #2160.)

**Decision: self-pipe wakeup + ioctl ground truth, with a poll-slice
fallback.** Rationale:

- Pure EINTR-based detection (no pipe) is racy: a signal landing between the
  size check and `poll()` entry is lost and we block. The self-pipe write
  persists in the pipe buffer, so `poll` returns immediately — this is the
  entire point of the self-pipe trick.
- A pure poll-loop size check (wake every N ms, compare TIOCGWINSZ) has no
  global state at all, but violates the "idle apps burn zero CPU / zero
  wakeups" budget in the vision charter.
- The signal handler is the smallest legal one: save `errno`, `write(2)` one
  byte to a nonblocking fd stored in an atomic, restore `errno`. Both
  `write` and atomics are async-signal-safe.
- Signal handlers are process-global, which is the footgun. Containment:
  exactly ONE terminal instance may own the SIGWINCH registration (an atomic
  claim); it saves the previous `sigaction` and restores it on leave/drop. A
  second concurrent instance (or a claim failure) degrades to capped poll
  slices (~250 ms) with an ioctl compare per wake — correctness kept, only
  idle-wakeup budget spent, and the degradation is explicit in code.
- Regardless of the wakeup source, the size delivered to the app always comes
  from a fresh `ioctl(TIOCGWINSZ)` compared against the cached size —
  coalesced or spurious signals are harmless, exactly the notcurses posture.

On Windows there is no SIGWINCH: `WINDOW_BUFFER_SIZE_EVENT` records arrive on
the console input handle (requires `ENABLE_WINDOW_INPUT`), interleaved with
key records, so resize detection is ordered with input for free.

### 1.3 Reading: /dev/tty + poll(2) with deadline

`read(deadline: Option<Instant>)` is implemented as
`poll([tty, winch_pipe], remaining)` in a loop that re-computes the remaining
timeout after every EINTR (signals interrupt poll even with SA_RESTART; POSIX
does not restart poll/select). Returning `TermRead::Input(&[u8])` from an
internal 4 KiB buffer avoids per-read allocation; resize surfaces as
`TermRead::Resize(Size)` out of band, because on no platform is "the window
resized" a byte-stream datum — inventing a private escape sequence to tunnel
it through bytes would collide with the parser's job of never trusting bytes.

### 1.4 Windows ConPTY

Per the Microsoft console documentation ("Console Virtual Terminal
Sequences", "SetConsoleMode", "High-Level Console Modes"):

- Output: `ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN`
  on top of `ENABLE_PROCESSED_OUTPUT`, with the documented graceful step-down
  (retry without `DISABLE_NEWLINE_AUTO_RETURN`). If VT output cannot be
  enabled at all we refuse: this engine is VT-only by charter (Windows 10
  1607+; the classic conhost non-VT path is not worth its weight).
- Input: `ENABLE_VIRTUAL_TERMINAL_INPUT` turns keystrokes into VT byte
  sequences readable via ReadFile/ReadConsole; we additionally keep
  `ENABLE_WINDOW_INPUT` (resize records) and clear
  `ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT` (raw mode)
  and `ENABLE_QUICK_EDIT_MODE` (it swallows mouse tracking).
- Read path: `WaitForSingleObject(handle, timeout)` then `ReadConsoleInputW`,
  draining records: `KEY_EVENT` key-down records carry the VT bytes one
  UTF-16 unit at a time (surrogate pairs recombined, then encoded UTF-8 into
  the same byte buffer the unix path uses, feeding the same parser);
  `WINDOW_BUFFER_SIZE_EVENT` triggers a `GetConsoleScreenBufferInfo` and is
  reported as `TermRead::Resize` — the visible window is
  `srWindow.right-left+1 x bottom-top+1`, NOT `dwSize` (the scrollback
  buffer). Reading whole records instead of `ReadFile` avoids the classic
  hang where `WaitForSingleObject` signals on a record that `ReadFile` then
  filters out (focus/menu events), leaving the read blocked.
- Output codepage is set to UTF-8 (65001) on enter and restored on leave, so
  `write(&[u8])` is byte-transparent like the unix path.

### 1.5 Cross-thread wakeups (`TerminalWaker`, cycle 2)

`Terminal::waker()` returns a `Clone + Send + Sync` handle whose `wake()`
interrupts a blocking `read`, which returns `TermRead::Wake`. This is the
reactive scheduler's bridge: `reactive::set_wake_callback(waker.wake())`
makes cross-thread signal posts wake the event loop with zero polling.

- Unix: a second, per-instance self-pipe polled beside the tty and the
  SIGWINCH pipe (three fixed pollfd slots; POSIX skips negative fds). The
  waker's closure holds the write end in an `Arc`, so a waker outliving
  its terminal writes into a reader-less pipe — harmless — instead of
  racing fd reuse. Distinct fd rather than multiplexed bytes on the
  SIGWINCH pipe: the SIGWINCH pipe is process-global and claimed by ONE
  instance, wakes are per-instance; sharing would couple unrelated
  lifetimes for the price of one fd.
- Windows: an unnamed auto-reset event added to the wait set
  (`WaitForMultipleObjects([console, event])`). Auto-reset gives wake
  coalescing for free.
- Contract: wakes COALESCE (n calls -> at least one Wake); `wake()` is
  cheap, non-blocking, panic-free from any thread. Priority under
  simultaneous readiness: Resize > Input > Wake — and the wake channel is
  not drained on the input path, so a pending wake always surfaces on a
  subsequent read (nothing lost, nothing starved).
- `EventReader::poll_event` maps Wake to `Ok(None)`, deliberately the same
  return as deadline expiry: loops must drain posted work and recompute
  their deadline on every `None`, and distinguishing the two would invite
  skipping the drain on timeouts. `input::probe_active` is the one caller
  that needs the difference and disambiguates by checking the clock.

### 1.6 Cell pixel geometry (cycle 2, for gfx scaling)

`Terminal::cell_pixel_size()` (defaulted `None`) answers "how many pixels
is one cell": unix derives it from TIOCGWINSZ (`ws_xpixel/ws_col`,
`ws_ypixel/ws_row`; zero fields mean unknown — many terminals report 0),
windows stays `None` (console font APIs are unreliable under ConPTY). The
active probe also asks over the wire (`CSI 16 t`, §2.2) so terminals with
silent ioctls still answer. `term::probe::refresh_cell_pixel_size` re-reads
the platform value after a resize (font zoom changes cell metrics at the
same cell count) and deliberately keeps a wire-probed value when the
platform cannot measure — local ignorance is not evidence the old wire
answer went stale.

### 1.7 Session verbs (cycle 3)

Small user-facing actions the app layer triggers between frames. All are
default trait methods over pure byte builders (`term::verbs`) so scripted
terminals inherit them; the platform backends override the stateful ones
to latch restore obligations.

- **`is_tty()`** — `isatty` on the RENDER handle (DESIGN request 6 /
  RT1-10c: stdout is the wrong question; this asks the fd/handle the
  engine actually writes). Default `false`, so scripted terminals
  auto-skip splash logic unless they opt in.
- **`suspend()`** (unix only) — the Ctrl+Z binding: full `leave()`, then
  `kill(0, SIGTSTP)` (the process GROUP, same as the tty driver's ^Z, so
  pipeline siblings stop coherently), then `enter(same options)` when
  SIGCONT resumes execution. No SIGCONT handler needed: the verb is
  synchronous around the stop. Signal-safety: nothing runs between leave
  and the stop but the kill itself; the preconditions (default SIGTSTP
  disposition, foreground process group) are the caller's, and an
  orphaned group degrades to a fast leave+enter. Callers must
  damage-all, re-query `size()`, and re-apply cursor style/title after
  it returns — the restore deliberately reset them.
- **Cursor style** — DECSCUSR `CSI Ps SP q` (0 default / 1-2 block / 3-4
  underline / 5-6 bar, odd = blinking). Backends latch non-default use
  and append `Ps 0` (the USER'S configured cursor, not a hardcoded
  block) to leave and to the emergency-restore slot.
- **Title** — OSC 0 with control bytes stripped (a filename containing
  ESC must not become an escape injection — test-pinned). First use
  pushes the XTWINOPS title stack (`CSI 22;0t`), leave pops it
  (`CSI 23;0t`); terminals without the stack keep the last title, which
  is the historic TUI behavior — best effort by design.
- **Clipboard** — OSC 52 write with hand-rolled RFC 4648 base64
  (dependency policy; RFC test vectors pinned). WRITE-ONLY by design:
  the read form (`OSC 52;c;?`) asks the terminal to type the clipboard
  back into the input stream — a clipboard-exfiltration vector — so this
  engine never emits it; paste arrives via bracketed paste. Gate success
  reporting on `Capabilities::osc52_copy` (xterm defaults
  allowWindowOps off; the kitty/wezterm/ghostty/foot/iTerm2/WT lineage
  defaults on; tmux translates it itself via set-clipboard).
- **Bell / notify** — BEL always available; `notify(msg, channel)` takes
  `caps.notify_channel()`: OSC 9 (iTerm2 convention: iTerm2/WezTerm/
  ghostty), OSC 99 basic form (kitty — it never adopted OSC 9), or
  BellOnly fallback. One channel per call, never both: ghostty-class
  terminals speak both dialects and would double-notify. foot's OSC 777
  stays deferred until a consumer asks.

Verb resets ride leave AFTER the mode teardown so they apply to the main
screen the user returns to, and `append_emergency_leave` keeps the panic
hook honest about verbs it could not know about.

### 1.8 tmux honesty (cycle 3; verified passthrough cycle 4)

Inside tmux (`TMUX` env / `tmux-*`/`screen-*` TERM):

- `needs_tmux_passthrough` is set: OSC/APC payloads meant for the OUTER
  terminal need `ESC Ptmux; <payload with ESC doubled> ESC \` wrapping —
  implemented as `term::tmux_wrap` (pure function, test-pinned against
  the tmux(1) manual's doubling rule).
- Graphics start DISABLED under tmux: passthrough requires the user's
  `allow-passthrough` option, OFF by default since tmux 3.3a and
  invisible from the environment. Wrapping blind would draw raw escape
  soup into default-config sessions.
- **Verified passthrough (cycle 4; mechanism live-proven cycle 5)**: the
  active probe sends a WRAPPED kitty-graphics query (distinct id) and a
  WRAPPED XTVERSION through `tmux_wrap`. Any wrapped reply is round-trip
  PROOF that allow-passthrough is on: the kitty reply flips
  `kitty_graphics` and sets `graphics_wrap = Some(Tmux)`; a wrapped
  XTVERSION naming iTerm2/WezTerm flips `iterm2_images` (that protocol
  has no query form, so the outer's NAME is the only detection path —
  names, not guesses). GFX3D's pipeline wraps every payload with
  `tmux_wrap` when `GraphicsCaps::wrap` says so — routing, not
  degradation. The underlying chain is no longer folklore: the live rig
  (`term::tmux_live_tests`, run against tmux 3.7b) proved unwrapped
  emergence with passthrough on, complete swallowing with it off, and
  byte-exact routing of the outer terminal's APC reply back to the pane
  process. Harness lessons recorded in the rig: the pane tty starts
  CANONICAL (raw-mode it before capturing replies) and stdio buffering
  eats captures on kill-server (use unbuffered writes).
- Reply attribution under tmux: the FIRST XTVERSION reply is tmux's own
  (FIFO — the direct query precedes the wrapped one) and lands in
  `tmux_version`; the SECOND crossed the wrapper and is the outer
  terminal's identity (`term_version`).
- Out of scope, noted deliberately: tmux's kitty **unicode-placeholder**
  transport (image cells as placeholder glyphs so tmux can scroll/split
  them) is a different, heavier integration than passthrough and stays
  future work; verified passthrough draws correctly but tmux cannot
  reflow those pixels across pane operations — the known cosmetic limit
  of the technique.
- `tmux_version` records the probe's XTVERSION answer (better evidence
  than env: works below tmux 3.4) or `TERM_PROGRAM_VERSION` from env.
- OSC 52 is NOT masked under tmux: tmux consumes it itself
  (`set-clipboard` defaults to `external`) and forwards to the outer
  terminal.

### 1.9 Restore guarantees

Restore happens in three layers (all idempotent):

1. `leave()` — exact reverse-order mode teardown + termios/console-mode
   restore.
2. `Drop` — calls the same restore path, best-effort, never panics.
3. `emergency_restore()` — a free function reading process-global saved state
   (termios + "which modes were entered"), for the app layer's panic hook.
   A panic hook cannot reach the `Terminal` instance (it is on another
   stack), so this global is the only honest way to guarantee "the terminal
   is always restored"; it is written on enter and cleared on leave.

Teardown order matters: kitty keyboard pop FIRST (while the app screen still
absorbs any queued replies), then mouse/focus/paste off, cursor show, main
screen, SGR reset, then termios. Modes are only torn down if they were set.

## 2. Capability detection (`term/caps.rs`)

### 2.1 Passive (environment) pass

Fast, offline, conservative. Sources and gotchas:

- `COLORTERM=truecolor|24bit` — the de-facto truecolor signal.
- `TERM` — `*-256color`, `*-direct`, `xterm-kitty`, `foot`, `dumb`, `linux`.
- `TERM_PROGRAM` + `TERM_PROGRAM_VERSION` — iTerm.app, WezTerm, ghostty,
  Apple_Terminal, vscode.
- App-specific markers: `KITTY_WINDOW_ID`, `WEZTERM_EXECUTABLE`/`WEZTERM_PANE`,
  `GHOSTTY_RESOURCES_DIR`, `ITERM_SESSION_ID`, `WT_SESSION` (Windows
  Terminal), `VTE_VERSION` (>= 5000 -> OSC 8 hyperlinks).
- **tmux gotcha**: under `TMUX`/`TERM=tmux-*|screen-*` the environment
  describes tmux, not the outer terminal; graphics protocols need passthrough
  (deferred to a later cycle), so `kitty_graphics`/`iterm2_images` are forced
  false inside tmux, while DECRQM/DA1 probes are answered BY tmux and remain
  meaningful (tmux passes 2026 through). Marked via `in_tmux` so upper layers
  can label degradation.
- SGR mouse / bracketed paste / focus events are near-universal in anything
  modern; they default true unless `TERM` is `dumb`/`linux`(console) or empty.
  **Windows exception (RT1-12b)**: classic conhost does not translate mouse
  into VT sequences under `ENABLE_VIRTUAL_TERMINAL_INPUT`, so on Windows
  `sgr_mouse` additionally requires evidence of a translating host
  (`WT_SESSION`, WezTerm markers, or any `TERM_PROGRAM`); bare conhost
  degrades to keyboard-only, honestly, instead of advertising a silently
  dead mouse.
- `NO_COLOR` (any non-empty value) forces `truecolor`/`colors_256` false and
  sets `no_color` — the user's request outranks terminal ability; it does
  not touch interaction features (mouse, paste, kitty keyboard).
- `undercurl` (SGR 4:3): kitty lineage, iTerm2, Windows Terminal, VTE ≥
  0.52; degrades to plain underline in the presenter when absent.
- `deferred_wrap` defaults TRUE everywhere (a property of the VT lineage,
  not a feature): the bit exists so one verified immediate-wrap terminal
  can flip the presenter to skip-last-column (RT1-5) without an engine
  release. The cycle-2 manual ConPTY verification decides Windows.

Env detection is injectable (`detect_env_with(lookup)`) so tests never touch
process env.

Consumer views: `caps.present_caps()` / `From<&Capabilities> for
render::present::PresentCaps` (RENDER request 1 — apps never hand-assemble
it; `NO_COLOR` folds to `Ansi16`, the closest expressible depth) and
`caps.graphics() -> GraphicsCaps` (GFX3D request 1: kitty/iterm2/sixel +
`sixel_max_registers` + `cell_pixel_size`).

### 2.2 Active probing (queries over the wire)

Design follows the kitty documentation's detection recipe: send feature
queries followed by a **DA1 sentinel** — terminals answer input FIFO, so when
the DA1 reply arrives, every earlier query that was going to be answered has
been answered. DA1 (`CSI c`) is answered by effectively every terminal ever
made, which bounds the wait without per-query timeouts.

Batch (in order, one write + flush):

| Query | Bytes | Reply | Meaning |
| --- | --- | --- | --- |
| kitty keyboard | `CSI ? u` | `CSI ? flags u` | protocol supported (sw.kovidgoyal.net/kitty/keyboard-protocol) |
| sync output | `CSI ? 2026 $ p` | `CSI ? 2026 ; Ps $ y` | DECRPM Ps: 1/2 = supported, 0/4 = not (contour vt-extensions spec, vt100.net DECRPM) |
| SGR-Pixels | `CSI ? 1016 $ p` | `CSI ? 1016 ; Ps $ y` | DECRQM for pixel-unit mouse (`sgr_pixel_mouse`); no env folklore for this one — probe evidence only |
| XTVERSION | `CSI > 0 q` | `DCS > \| name ver ST` | terminal name/version whitelist (xterm ctlseqs) |
| XTSMGRAPHICS | `CSI ? 1 ; 1 ; 0 S` | `CSI ? 1 ; 0 ; N S` | sixel color registers (xterm ctlseqs; status 0 = success; N clamped to u16) |
| cell pixels | `CSI 16 t` | `CSI 6 ; h ; w t` | XTWINOPS cell size report, HEIGHT before WIDTH; sanity-gated 1..=512 so a confused terminal cannot overwrite the ioctl answer |
| kitty graphics | `APC G i=4242,s=1,v=1,a=q,t=d,f=24;AAAA ST` | `APC G i=4242;OK ST` | graphics protocol probe with unique id (sw.kovidgoyal.net/kitty/graphics-protocol) |
| *tmux only:* wrapped XTVERSION | `tmux_wrap(CSI > 0 q)` | second `DCS > \| … ST` | the OUTER terminal names itself → passthrough proven + iTerm2/WezTerm image path identified (no query form of its own exists) |
| *tmux only:* wrapped kitty graphics | `tmux_wrap(APC G i=4343 … ST)` | `APC G i=4343;OK ST` | passthrough proven AND outer speaks kitty graphics; distinct id keeps direct/wrapped replies apart |
| DA1 sentinel | `CSI c` | `CSI ? … c` | end of probe; params containing 4 = sixel (under tmux: tmux's OWN sixel re-encoding — usable without wrapping) |

tmux timing note: wrapped replies pay an extra round trip (tmux → outer →
tmux → pane), so tmux's own DA1 answer does NOT bound them the way FIFO
bounds direct replies. The driver grants a bounded grace
(`probe::TMUX_GRACE`, 150 ms) past the sentinel while wrapped answers are
still owed; passthrough-off sessions (the tmux default) spend that grace
once at startup, invisibly (the probe runs concurrently with first paint,
§2.3).

Rules:

- The prober is **sans-IO** (`ActiveProbe`, `term::probe`): it hands out
  query bytes and consumes `CapsReply` values; the read loop with the
  deadline lives in `input::probe_active`, which routes replies through the
  SAME `input::Parser` the app uses — replies are just events, so a probe
  never desynchronizes the input stream, and user keystrokes arriving
  mid-probe are returned to the caller instead of being dropped.
- The overall deadline (default 500 ms) makes probing safe against terminals
  that answer nothing at all: the probe simply ends with the passive
  result. Never hangs, by construction.
- **Dumb terminals are never probed** (RT1-6b): `Capabilities::dumb`
  (TERM=dumb or empty environment) makes `probe_active` return before
  writing a single query byte — the rule that gives a dumb terminal no
  escapes forbids interrogating it with escapes. Test-pinned.
- **Late replies stay caps events** (RT1-6c): a reply arriving after the
  DA1 sentinel (multiplexer passthrough answering seconds late) decodes
  through the same parser frames into `Event::CapsReply` — consumed or
  dropped by the app, never leaked as text or `Unknown` key events.
  Test-pinned at both the parser and the reader level, split at every
  byte boundary.
- Probing is optional: callers can stay entirely passive.
- The kitty graphics query carries a fresh non-zero id; the reply must echo
  the id and say `OK` (per the protocol doc, `a=q` neither stores nor
  displays the dummy image).

Rejected: XTGETTCAP as a primary source. It is DCS-heavy, hex-encoded, and
supported by fewer emulators than the query set above; the parser still
recognizes its reply frame (`DCS 1 + r … ST`) and routes it to caps as raw
data so a later cycle can adopt it without parser changes.

### 2.3 Startup sequencing (RT1-6a — the recipe REACT wires into App::run)

The probe deadline must never sit between the user and the first frame.
The sequencing that keeps startup honest on every terminal:

1. `Capabilities::detect_env()` — free, synchronous, before anything else.
   `dumb` here also means "skip splash, skip probe, minimal session".
2. `Terminal::enter(opts)` with options gated on the ENV pass (e.g. push
   kitty keyboard flags only if `caps.kitty_keyboard`).
3. **First paint immediately**, using the env-pass caps. The boot splash
   starts here — on env-truecolor and the mosaic path if graphics are not
   env-proven (RT1-10d: the splash never waits for the probe).
4. `input::probe_active(term, reader, &mut caps, ~500ms)` runs where the
   loop already polls: write the batch after the first frame is flushed,
   then treat probe replies as ordinary events during the normal loop —
   or call the helper before entering the loop when a blocking budget is
   acceptable (headless tools). Either way the terminal answers within a
   frame or two in practice.
5. **Upgrade callback**: when the probe completes (DA1 sentinel or
   deadline), recompute `caps.present_caps()` / `caps.graphics()`; if
   anything changed, damage-all once and repaint. One frame of 256-color
   before truecolor is invisible; 500 ms of black before a splash is not.
6. A skip key pressed during the splash arrives through the same
   `EventReader` (probe passthrough returns it) — DESIGN request 10's
   "input readable before the app loop" is this property.

The engine does not enforce the recipe (REACT owns App::run); the kernel
guarantees the parts that make it safe: env pass is instant, the probe is
non-blocking-shaped, dumb terminals skip it entirely, and late replies
can never corrupt the event stream.

## 3. Input parser (`src/input/**`)

### 3.1 Shape

A resumable state machine fed arbitrary byte chunks: `feed(&[u8], &mut
Vec<Event>)`. States: `Ground`, `Esc`, `Csi`, `Ss3`, `Osc`, `Dcs`, `Apc`,
`Paste`, plus a 3-byte `X10Mouse` swallow state. Only the incomplete *tail*
is retained between chunks (bounded sequence buffer, UTF-8 partial of at most
3 bytes, paste buffer with chunked flushing) — a chunk may split anywhere,
including mid-UTF-8, mid-CSI-param, or mid-paste-terminator.

Hard bounds (fuzz posture, REDTEAM will attack these):

- CSI/SS3 sequences: 256 bytes max; overflow aborts to `Event::Unknown`
  (bytes capped at 64) and the machine resets to Ground. String frames
  (OSC/DCS/APC): 4 KiB payload cap, overflow discards the excess but keeps
  frame sync (still waits for the real terminator).
- `ESC` inside a CSI aborts the current sequence (emitted as Unknown) and
  starts a fresh escape — a torn sequence can never eat the following one.
- Paste content flushes as multiple `Paste` events past 64 KiB, so hostile
  never-ending paste cannot grow memory unboundedly.
- No panics: every index is guarded, every unknown falls into
  `Event::Unknown`, invalid UTF-8 becomes U+FFFD (incremental decoder with
  minimal-length + surrogate rejection, never `unwrap`).

### 3.2 ESC disambiguation deadline

A bare `ESC` byte is ambiguous (Esc key vs sequence intro vs Alt prefix). The
parser owns no clock: it exposes `pending()` (what is buffered:
`PendingEsc`/`PendingSeq`/`None`) and `flush_pending(deadline_kind)`. The
`EventReader` driver applies two configurable deadlines: ~30 ms for a bare
ESC (emit `Key(Esc)`), ~500 ms for a torn sequence (emit `Unknown`) — the
long one exists because SSH links legitimately split sequences across
packets, and destroying a valid arrow key at 30 ms would be worse than a
late Unknown. (crossterm collapses ESC immediately when the read buffer is
empty, which misfires over laggy links; the kitty protocol's disambiguate
flag exists precisely to make this whole problem disappear, and we enable it
whenever the terminal supports it.)

### 3.3 Protocol coverage

- **UTF-8 text** — incremental, invalid -> U+FFFD, never panics.
- **Legacy keys** (`legacy.rs`) — CSI final-letter forms (A/B/C/D/H/F, Z for
  Shift+Tab, P/Q/R/S for F1–F4 incl. the `CSI 1;mods R` vs cursor-position
  report ambiguity, resolved in favor of CPR when param0 != 1), CSI `~` tilde
  keys (Home/Ins/Del/End/PgUp/PgDn, F1..F20, incl. kitty's relocated F3 at
  13~), SS3 forms, C0 controls (Ctrl+letter conventions, 0x7F Backspace,
  0x08 = Ctrl+H, 0x0D Enter, 0x00 Ctrl+Space), Alt via ESC prefix (including
  Alt+control and Alt+Backspace).
- **kitty CSI u** (`kitty.rs`) — full progressive-enhancement grammar
  `CSI code[:shifted[:base]] ; mods[:event] [; text…] u`: modifier bitmask
  (shift/alt/ctrl/super/hyper/meta/caps/num), event types press/repeat/
  release, associated text (codepoints), functional key range 57358–57454
  (modifiers-as-keys, keypad mapped to their plain equivalents for now, media
  and unmapped codes preserved as `KeyCode::Functional(u32)`).
- **SGR mouse 1006** (`mouse.rs`) — `CSI < b;x;y M|m`: press/release with
  real button identity on release (SGR's advantage over X10), drag (motion
  bit + button), pure motion (button bits 3), wheel up/down/left/right,
  back/forward buttons (128+), shift/alt/ctrl modifiers, 1-based -> 0-based
  `Point`.
- **Bracketed paste 2004** — `CSI 200~ … CSI 201~`. The terminator scan is a
  byte-at-a-time incremental match that survives chunk splits *inside* the
  terminator and handles the classic "ESC[201 that never completes" and
  "ESC inside pasted content" cases: on a mismatch, matched bytes are
  replayed into content and the match restarts if the failing byte is
  itself ESC (the needle contains no interior ESC, so this is exact, not
  heuristic).
- **Focus 1004** — `CSI I` / `CSI O`.
- **Caps replies** — routed as `Event::CapsReply` (see 2.2), including DA1,
  DECRPM, kitty flags, XTVERSION/XTGETTCAP DCS, kitty graphics APC, cursor
  position reports.
- **Foreign sequences** — anything unrecognized is swallowed as
  `Event::Unknown(bytes ≤ 64)`; the parser never lets garbage leak into the
  text stream as fake keystrokes (e.g. X10 mouse's 3 payload bytes are
  consumed, not replayed as characters).

### 3.4 Event model

`Event::{Key(KeyEvent), Mouse(MouseEvent), Paste(String), FocusGained,
FocusLost, Resize(Size), CapsReply(CapsReply), Unknown(Vec<u8>)}`.
`KeyEvent{code, mods, kind: Press|Repeat|Release, text: Option<String>}` —
`text` is `None` for plain typing (the char is in `code`; zero allocation on
the hot path) and `Some` only when the kitty protocol reports associated
text distinct from the code. `Resize` is emitted by the platform layer, not
parsed from bytes (see 1.3); it lives in `Event` so the app layer sees one
unified stream via `EventReader`.

## 3.4 Accessibility environment sources (cycle 6 — checked, mostly absent)

What the ENVIRONMENT can tell an app about accessibility preferences,
audited so REACT's app-level a11y signal knows what the kernel can and
cannot feed it:

- `NO_COLOR` — the one real contract. Surfaced since cycle 2
  (`Capabilities::no_color`; forces color depth down, never touches
  interaction features).
- `TERM=dumb` — surfaced (`Capabilities::dumb`); implies "no decorative
  anything", which subsumes reduced motion for that terminal class.
- **Reduced motion / high contrast: NO terminal or environment standard
  exists.** OS-level preferences (macOS "Reduce Motion", GTK/portal
  settings, Windows contrast themes) live behind OS APIs that a terminal
  session does not forward, and no terminal advertises them via env or
  escape query. Deliberately NOT inventing a `REDUCED_MOTION` variable:
  a11y posture is APP POLICY (a settings surface + REACT's signal), with
  `no_color`/`dumb` as the only honest environmental inputs. If a real
  convention emerges (the way NO_COLOR did), it lands in the env pass in
  one line — the seam is ready.

## 3.5 Evidence matrix (final for the coredoc cycle — what is actually
## known, honestly; live tmux result included)

Three evidence classes. VERIFIED = exercised by live pty tests in this
tree, every cycle. COMPILE-CHECKED = builds under
`--target x86_64-pc-windows-msvc`, has never executed (no Windows host
has existed in cycles 1-4). FOLKLORE = environment-variable heuristics
encoding community knowledge of terminal behavior — honest guesses, aimed
conservative, correctable by the active probe where a query exists.

| Surface | Evidence | Notes |
| --- | --- | --- |
| unix raw mode, enter/leave, restore | VERIFIED | pty tests incl. reverse-order teardown, verb resets |
| fd acquisition + ctty keystroke path (RT5-1) | VERIFIED | setsid+TIOCSCTTY pty children: Darwin alias-POLLNVAL + circular alias-ttyname pinned; engine `new()`→enter→read delivers keystrokes healthy (no degradation); POLLNVAL runtime fallback live-exercised pre-rewrite |
| unix read/deadline/resize (SIGWINCH pipe + ioctl) | VERIFIED | live signal → pipe → poll → ioctl round trip |
| unix waker (cross-thread, coalescing, ordering) | VERIFIED | incl. waker-outlives-terminal no-op |
| suspend/resume byte order | VERIFIED* | *the `kill(0, SIGTSTP)` itself is cfg(test)-seamed out (would stop the test runner's group); 2 lines correct by inspection |
| cell pixel size from TIOCGWINSZ | VERIFIED | incl. zero-fields-mean-unknown |
| parser (UTF-8, keys, kitty, mouse, paste, garbage) | VERIFIED | plus REDTEAM's hostile corpus + split-invariance suites |
| editor key matrix (mod combos, F1-F24, keypad flag, modifyOtherKeys, chords) | VERIFIED | table-driven, whole-matrix split-invariance (`input::editor_matrix_tests`); legacy-undecidable combos pinned as documented degradations |
| editor-grade paste (5 MB multi-line, byte-exact, bounded chunks, UTF-8-safe seams, embedded-escape fuzz) | VERIFIED | mid-paste flushes hold back partial UTF-8 sequences so chunk concatenation is lossless; 60-round boundary fuzz with embedded ESC/CSI/nested-marker/OSC-52-lookalike content; bracketed paste is the ONLY paste path (OSC 52 read forbidden, §1.7) |
| event interleave under load (keys × focus × resize × wake) | VERIFIED | 120-round deterministic fuzz: zero drops, script order preserved |
| probe state machine (sentinel, late replies, dumb skip, tmux wrap/grace) | VERIFIED | scripted-terminal level; not yet against a live tmux |
| windows VT modes, records loop, waker event, verbs | COMPILE-CHECKED | wait/latch logic reasoned, never run |
| `deferred_wrap = true` on Windows | COMPILE-CHECKED + documentation | Microsoft VT docs describe deferred EOL; flip procedure in §5 |
| windows `sgr_mouse` gating (WT_SESSION etc.) | FOLKLORE | cfg(windows) branch, not unit-testable from this host |
| truecolor/256 (COLORTERM, TERM) | FOLKLORE | strong folklore; probe does not verify color depth |
| kitty keyboard / graphics / sixel / 2026 / cell px / registers / SGR-Pixels | FOLKLORE → VERIFIED-AT-RUNTIME | env guesses; the ACTIVE PROBE replaces them with query evidence per session (DECRQM/DA1/XTSMGRAPHICS/XTWINOPS/kitty a=q) |
| tmux passthrough mechanism | **VERIFIED (live tmux 3.7b, cycle 5)** | `term::tmux_live_tests` (ignored-by-default rig; this test IS the outer terminal via a pty it owns): allow-passthrough ON → wrapped kitty query emerges UNWRAPPED on the outer tty (no wrapper leak); defaults (OFF) → swallowed entirely; the outer's APC reply ROUTES back to the pane process byte-exact — the full chain `ActiveProbe`'s verified-passthrough design rides |
| tmux passthrough per session | VERIFIED-AT-RUNTIME | never assumed; the wrapped-query round trip per session remains the only enable path |
| OSC 52 / OSC 9 / OSC 99 / undercurl / underline-color / hyperlinks / focus / paste | FOLKLORE | no query forms exist (or none adopted); conservative defaults, degradation labeled |
| tmux quirks (env leakage, set-clipboard, own DA1/sixel, reply attribution order) | FOLKLORE + tmux(1) manual | mechanism proven live (above); FIFO attribution of direct-vs-wrapped XTVERSION replies remains scripted-only |

The honest summary: everything unix-side that CAN be exercised from a pty
is; Windows is a compile-time promise until a host exists; env heuristics
are labeled folklore and the probe converts the important ones into
per-session evidence.

## 4. Contracts other modules rely on

- The `Terminal` trait is declared STABLE as of cycle 2 (REDTEAM request
  1): `TermRead::Wake`, `waker()` and `cell_pixel_size()` were the
  additions; the two new methods are defaulted so scripted terminals
  implement the six core operations and opt in.
- OPOST is off in raw mode: emit `\r\n`, never bare `\n` (render layer).
- `Terminal::write` never inspects bytes; presenters own escape emission.
  One `flush()` per presented frame (RT1-16a) is the presenter's contract;
  the backends' internal 64 KiB overflow guard is invisible to that
  counting and safe under a 2026 bracket (the BSU is in-stream).
- `EventReader::poll_event` is the single-event entry: it multiplexes
  bytes -> parser, resize passthrough, wakeups, ESC deadlines, and returns
  exactly one event per call (internal queue drains before touching the
  fd). `Ok(None)` = deadline OR wake: drain posted work, recompute, loop.
- `EventReader::poll_many` (cycle 4) is the batch entry: one blocking
  wait, then a non-blocking drain of everything decoded or immediately
  readable — at most one zero-timeout confirmation read per BATCH instead
  of per event. `Ok(0)` means what `poll_event`'s `None` means. The
  internal queue already amortized syscalls across events in one chunk;
  poll_many formalizes batch dispatch and removes the per-call
  empty-confirmation read an app loop would otherwise pay.
- SGR-Pixels (1016): the wire grammar is identical to cell reporting, so
  the UNIT is a mode, not a parse result. `Terminal::set_pixel_mouse`
  toggles the mode (latched, reset on leave); `EventReader::
  enable_pixel_mouse(cell_px)` — which deliberately REQUIRES the cell
  geometry — switches interpretation: `MouseEvent::pos` stays cells
  always, `MouseEvent::pixel` carries the raw point. Flip both in the
  same act, keyed on `Capabilities::sgr_pixel_mouse` (DECRQM-probed,
  never folklore); refresh the divisor after resizes.
- `emergency_restore()` must be called from the app panic hook (request
  filed to REACT in `reviews/cycle1/kernel-requests.md`).
- `have_tty()` answers boot's auto-skip question against the handle the
  engine actually renders to (`/dev/tty` / CONIN$+CONOUT$), not stdout
  (RT1-10c).

## 5. Deferred (cycle 4+) and standing status

- **XTGETTCAP: deferred again, deliberately (cycle-3 review).** The
  current query set (kitty `?u`, DECRQM 2026, XTVERSION, XTSMGRAPHICS,
  XTWINOPS 16, kitty-graphics `a=q`, DA1) already answers every
  capability any module consumes. XTGETTCAP's marginal value is
  terminfo-string retrieval (`Tc`/`RGB` truecolor hints), which
  COLORTERM + XTVERSION already cover for precisely the emulators that
  implement XTGETTCAP at all (xterm, kitty, wezterm, iTerm2, ghostty —
  all identified by the existing probe). Its cost is real: hex-encoded
  round trips and per-capability reply handling. Adoption criterion: a
  consumer names a capability the current set cannot answer. The parser
  already frames and routes `DCS 1+r`/`0+r` replies (`CapsReply::
  XtGetTcap`), so adoption later is prober-only work — no parser change.
- **ConPTY deferred-wrap verification (RT1-5): impossible on this host,
  status honest.** No Windows machine has been available in cycles 1-3;
  `deferred_wrap` ships TRUE (Microsoft's own console VT documentation
  describes xterm-style deferred EOL wrap for ConPTY, and Windows
  Terminal's renderer follows it — documentation evidence, not a live
  run). The flip procedure when someone runs the verification: run any
  full-screen app writing the bottom-right cell under conhost + ConPTY;
  if the screen scrolls one row, set `deferred_wrap = false` in
  `Capabilities::detect_env_with` under `cfg(windows)` for the failing
  host class and notify RENDER to activate the skip-last-column
  strategy keyed on the bit. Until then the bit is load-bearing but
  unexercised — recorded as an accepted risk, re-filed to REDTEAM each
  cycle a Windows CI host does not exist.
- Verified tmux passthrough for graphics (needs an active probe through
  `tmux_wrap` + a way to detect `allow-passthrough`; see §1.8).
- OSC 10/11 background/foreground color queries (framed + routed today).
- kitty keyboard flag *stack* management across nested screens.
- foot's OSC 777 notification dialect (OSC 9 + OSC 99 + BEL cover the
  known terminals; 777 waits for a consumer on foot).
- Windows-only lint visibility (RT4-3): windows-gated code never enters
  the default clippy run — check it explicitly with
  `cargo clippy --target x86_64-pc-windows-msvc` (kernel files are clean
  on both targets as of cycle 5; keep it that way).
- Windows fallback input path for pre-VT consoles (ReadConsoleInput key
  records -> synthesized events) if we ever care about Windows 8/old
  conhost.
- A wire re-probe of `CSI 16 t` after resize (today: platform ioctl
  refresh only; a mid-session font-zoom on a silent-ioctl terminal keeps
  the old cell pixel size until the next full probe).

Resolved since cycle 1: pixel geometry (`base::PixelSize` +
`Terminal::cell_pixel_size()` + `CSI 16 t`, §1.6, §2.2); session verbs +
tmux honesty (§1.7, §1.8, cycle 3); `is_tty()` on the trait (DESIGN
request 6, cycle 3).

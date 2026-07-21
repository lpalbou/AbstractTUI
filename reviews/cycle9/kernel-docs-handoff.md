# KERNEL → docs cycle: the 10 things the user guide MUST say about terminals

Doc-ready prose (lift freely). Source of truth for claims:
`docs/design/term-input.md` §3.5 (evidence matrix) and the rustdoc on
every public item (kernel modules enforce `missing_docs`).

## 1. Platform support is honest, not symmetric

macOS and Linux are the verified platforms: every unix code path is
exercised by live pty tests, including signal-driven resize, job-control
suspend, and keystroke flow under a real controlling terminal. **Windows
support is best-effort**: the backend compiles cleanly and is
lint-clean against the MSVC target, its platform-independent logic
(UTF-16 pairing, wake latching, resize dedupe) is unit-tested on every
host, and its console usage was reviewed against Microsoft's documented
semantics — but it has never executed on a live Windows machine. Treat
the first Windows run as a beta event, not a certified path.

## 2. The terminal always comes back

Restore is layered three deep: `leave()` undoes everything `enter()` did
(in exact reverse order), `Drop` runs the same restore if you forget, and
a process-global emergency path (`term::emergency_restore`) exists for
panic hooks. Cursor style, window title, pixel-mouse mode and kitty
keyboard flags are all tracked and reset — including from a panic.

## 3. Keyboard input is never silently dead

If the platform refuses to poll the terminal descriptor (a real macOS
quirk with `/dev/tty` that this engine detects and avoids), the reader
falls back to a working descriptor with a labeled degradation
(`Terminal::degraded()`), or fails with an actionable error. An app that
starts is an app that receives keys.

## 4. Some key combos do not exist on legacy terminals

Ctrl+Enter, Shift+Enter, and Ctrl+Backspace are byte-identical to plain
Enter / Ctrl+H on the classic wire — no parser can recover what the
terminal never sent. They become distinct under the kitty keyboard
protocol or xterm's modifyOtherKeys, both decoded automatically when
present. Apps should treat these chords as enhancements, not baseline
bindings; everything on arrows/Home/End/PgUp/PgDn/F1-F12 with any
modifier combination is reliable everywhere.

## 5. Clipboard: write yes, read never

Copy-to-clipboard uses OSC 52 (gated on detection — some terminals
silently ignore it). The READ form of OSC 52 is deliberately never
emitted: it would let any application read the user's clipboard, a
data-exfiltration vector. Paste reaches apps exclusively through
bracketed paste, which is fuzz-hardened (multi-megabyte pastes stream in
bounded chunks, byte-exactly, with embedded escape sequences neutralized
as content).

## 6. tmux: honest by default, images only when proven

Inside tmux, graphics are OFF by default because tmux swallows the
protocols unless the user set `allow-passthrough on` — which is
invisible from the environment. The engine actively verifies passthrough
per session with a wrapped round-trip probe (live-proven against tmux
3.7b) and only then enables kitty/iTerm2 image paths, wrapped
automatically. Known cosmetic limit: tmux cannot reflow passthrough
images across scrolling or pane splits.

## 7. Capabilities are evidence, not folklore

Color depth, kitty keyboard/graphics, sixel, synchronized output, cell
pixel geometry and pixel-mouse support are detected in two passes: an
instant environment pass for the first frame, then an active query probe
that runs concurrently and can both raise AND lower the answer (a
terminal that answers "mode not recognized" is believed). `NO_COLOR` and
`TERM=dumb` are honored. For humans: `caps.summary()` (multi-line) and
`caps.summary_line()` (one line) print the result; scripts should read
fields, not parse prose.

## 8. Accessibility inputs the environment actually provides

`NO_COLOR` and `TERM=dumb` are the only real environmental accessibility
signals, and both are surfaced. There is NO terminal standard for
reduced motion or high contrast — OS preferences do not cross the pty
boundary. Apps own that policy (a settings surface); the engine will
adopt any future convention the way it adopted NO_COLOR.

## 9. Suspend/resume is a first-class verb (unix)

`Terminal::suspend()` implements Ctrl+Z properly: full restore, stop the
process group (exactly what the shell expects), re-enter on resume. After
it returns, apps must repaint everything, re-query the size (the window
may have changed while stopped), and re-apply cursor style/title. On
Windows it returns an explicit Unsupported error — hide the binding.

## 10. The event stream is one stream, and it never lies

Keys (with press/repeat/release when the terminal can say), mouse (SGR,
cells always — raw pixels ride alongside only when pixel reporting is
verifiably active), paste, focus, resize, and terminal query replies all
arrive ordered through one reader. Unknown/foreign escape sequences are
swallowed and surfaced as `Unknown` events — hostile bytes cannot forge
keystrokes, and 60-round boundary fuzzing plus a hostile corpus keep it
that way every build. Resize is delivered from platform ground truth
(never parsed from bytes), deduplicated, and re-checked on every wake so
a missed signal cannot leave a stale layout.

---

Numbers the docs may cite (as of cycle 9): 129 kernel unit tests + 5
doctests, zero clippy warnings on both targets, zero missing-docs in
`term`/`input` (lint-enforced), parser throughput ~177 MB/s on the mixed
corpus, live verifications: macOS pty suite, tmux 3.7b passthrough round
trip, RT5-1 controlling-terminal keystroke acceptance.

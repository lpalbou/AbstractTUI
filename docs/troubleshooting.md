# Troubleshooting

Symptom → cause → fix, for the problems terminal reality actually
produces. Two diagnostic surfaces recur below:

- **The capability report**: `cargo run --example dashboard -- --caps`
  (also `viewer3d`, `images`) prints what the engine detected — color
  depth, image protocols, keyboard enhancements, tmux state. In code:
  `caps.summary()` (multi-line) or `caps.summary_line()` (one line).
- **Startup notices**: labeled degradations are collected at startup and
  exposed reactively (`use_startup_notices`); render them in a footer or
  toast and problems name themselves.

## Nothing renders at all

**Cause**: there is no terminal to render to. Either the process is not
attached to a tty (output redirected, running under CI), or `TERM=dumb`
(or empty) told the engine not to emit escapes at this terminal.

**Fix**: run inside a terminal emulator. If stdin/stdout/stderr are all
redirected and `/dev/tty` is unavailable, terminal construction fails with
an actionable error rather than emitting bytes into the void. For CI and
tests, don't fight it — drive the app headlessly with
`testing::CaptureTerm` (see [faq.md](faq.md#how-do-i-test-my-app-headlessly)).
Note the shipped examples deliberately exit 0 with a one-line notice when
there is no interactive terminal.

## Keyboard is dead under an unusual shell or launcher

**Cause**: some environments hand the process a terminal descriptor that
cannot be polled (a real macOS quirk with `/dev/tty`). The engine detects
this and falls back to a working descriptor instead of blocking forever.

**Fix**: usually none needed — an app that starts is an app that receives
keys. The fallback is a *labeled* degradation: `Terminal::degraded()`
returns the reason, and it lands in the startup notices. If keys are
genuinely dead, check the notices first; if the engine could not find any
workable descriptor it fails with an actionable error rather than starting
deaf.

## Images don't show (or fall back to blocky glyphs)

**Cause**: the terminal didn't prove a pixel protocol. Image channels are
enabled by detection — kitty graphics, iTerm2, or sixel (sixel also needs
the cell pixel geometry) — and anything unproven falls back to unicode
mosaic, with the degradation labeled, never silent.

**Fix**: check `--caps` to see which channel was chosen and why. Under
tmux, graphics are off by default: tmux swallows the protocols unless
`allow-passthrough on` is set, and that setting is invisible from the
environment, so the engine verifies it per session with a wrapped
round-trip probe and only then enables the pixel paths. Set
`set -g allow-passthrough on` in `~/.tmux.conf`, restart the session, and
re-check `--caps`. Mosaic output is not a bug — on terminals with no pixel
protocol it *is* the correct answer, and the quadrant/sextant/braille
modes are a deliberate quality ladder within it.

## Colors look wrong or washed out

**Cause**: the terminal did not advertise truecolor, so every 24-bit color
is being quantized to the 256- (or 16-) color palette. Detection reads
`COLORTERM` and `TERM` in the environment pass, and the active probe can
both raise and lower the verdict. `NO_COLOR`, if set, forces color off
deliberately.

**Fix**: use a truecolor terminal, or export `COLORTERM=truecolor` if your
terminal genuinely supports it but doesn't say so (common over some SSH
hops that strip the variable). Check what was detected with `--caps`. One
guarantee under quantization: foreground/background pairs are re-picked
together, so text may band but never vanishes into its own background.

## The screen flickers or tears during animation

**Cause**: the terminal doesn't support synchronized output (DEC private
mode 2026), so partially-painted frames can be displayed mid-write. Where
the capability is detected, the engine brackets frames and the terminal
displays each one atomically.

**Fix**: use a terminal that supports synchronized output (check
`--caps` — `sync` appears in the summary line when detected). Everything
still works without it; the engine's damage tracking keeps writes small,
which minimizes the visible window, but true tear-free animation needs the
terminal's cooperation.

## Ctrl+Enter behaves exactly like Enter

**Cause**: on the legacy wire they are the same bytes. Ctrl+Enter,
Shift+Enter, and Ctrl+Backspace are byte-identical to Enter / Ctrl+H — the
information does not exist in the stream, so no parser can recover it.

**Fix**: use a terminal with the kitty keyboard protocol or xterm's
modifyOtherKeys — both are detected and decoded automatically, and these
chords become distinct. In your own app, treat such chords as
enhancements with a baseline alternative; arrows, Home/End, PgUp/PgDn, and
F1–F12 with any modifier are reliable everywhere.

## The boot splash doesn't play

**Cause**: one of the deliberate gates fired. `boot::should_splash` skips
when the render handle is not a tty, when `ABSTRACTTUI_NO_SPLASH` is set
(to anything except `0`), when `NO_COLOR` is set, when `TERM=dumb`, or
when the capability report classifies the terminal as dumb.

**Fix**: if you *want* the splash, clear those variables and run on a real
tty (`cargo run --example splash` to verify; `ABSTRACTTUI_NO_SPLASH=0`
explicitly opts back in under wrapper scripts that set it). The gate
function returns the skip reason as a string — log it and the answer reads
itself. Also remember any keypress skips the splash with a fast fade; a
buffered keystroke at launch can end it almost immediately.

## Frames are slow

**Cause**: usually one of three, in this order: a debug build (the
rasterizer and mosaic fit are numeric code — `--release` is several times
faster); a busy machine (the published envelope is from an idle box, and
medians inflate several-fold under host contention); or your app damaging
more than it thinks (a signal written every tick re-renders every region
that reads it).

**Fix**: measure in `--release` first. Then turn on the compositor's
damage visualizer (`set_debug_damage(true)`) — it outlines exactly which
regions repaint each frame; a supposedly idle screen with outlines
everywhere means some signal is being written needlessly. For 3D scenes,
the perf envelope and its reproduction commands are in
[graphics-and-3d.md](graphics-and-3d.md#performance-envelope) — the
renderer is vertex-bound at cell scale, so triangle count matters far more
than viewport size.

## Wide characters are misaligned in some terminals

**Cause**: East-Asian-Ambiguous characters, emoji presentation sequences
(VS16), and ZWJ families genuinely render at different widths across
terminals — some split emoji families into components, some render
ambiguous symbols double-wide under CJK configurations or emoji-font
fallback. There is no protocol to query the terminal's opinion.

**Fix**: the engine already confines the damage — after emitting a risky
cluster it re-anchors the cursor, so a width disagreement stays inside
that cluster instead of shifting the whole line (the classic smear). What
it cannot fix: a terminal configured ambiguous-*wide* breaks the cell
grid of every TUI. Keep the terminal's default width configuration, and
prefer unambiguous glyphs (plain ASCII, box drawing, block elements) in
structural chrome.

## Tests hang forever

**Cause**: the app was spawned in a harness with piped stdin that never
reaches EOF. An idle app deliberately sits in a blocking read (zero CPU),
so with a pipe that never sends bytes and never closes, it waits forever —
that is correct behavior pointed at the wrong harness design.

**Fix**: don't drive the real binary through pipes in tests. Use the
canonical headless harness — `testing::CaptureTerm` plus `Driver::turn` —
which runs the full production pipeline synchronously: push input bytes,
turn one frame, assert on the rendered screen. Every test in this crate
that exercises the app loop is written that way, and it needs no tty, no
timeouts, and no sleeps.

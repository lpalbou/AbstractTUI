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

## An image refuses to decode by name

**Symptom**: the picture never appears and the widget (or the decoder's
`Result`) carries a named message such as `image: unrecognized format` or
`jpeg: arithmetic-coded JPEG not supported`.

**Cause**: the bytes are outside what the engine decodes. `decode_image`
routes on the MAGIC, never a declared MIME type or a file extension, so a
`.jpg` that actually holds a WebP is rejected as an unrecognized format
rather than decoded as JPEG. The supported set is PNG at
8-bit depths without interlacing, and 8-bit Huffman JPEG — baseline,
extended sequential, and progressive, grayscale or YCbCr. Arithmetic-coded,
lossless, hierarchical, 12-bit, and CMYK JPEGs, interlaced (Adam7) PNGs,
and GIF/WebP/AVIF/TIFF all reject by name rather than render wrong.

**Check**: read the message — every rejection names the exact reason,
and it is written to be shown to a user verbatim. `file image.jpg` (or
`djpeg -verbose`) confirms the real container and JPEG variant.

**Fix**: convert the asset to a supported encoding — a PNG or JPEG export
from any image editor, or a command-line converter (`sips -s format png
in.webp --out out.png` on macOS). Truncated downloads report truncation
specifically; fetch the file again before converting.

**Verify**: `gfx::decode_image` returns `Ok` and the widget shows the
picture instead of the labeled broken-image state. See
[graphics-and-3d.md](graphics-and-3d.md) for the full decoder coverage.

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

**Fix**: measure in `--release` first. Then audit what repaints: a
supposedly idle screen that keeps painting means some signal is being
written needlessly (every `dyn_view` that reads it re-renders). Embedders
driving the render pipeline directly can also flip the compositor's damage
visualizer (`render::Compositor::set_debug_damage(true)`) to outline each
frame's repaint regions — under `App::run` the driver owns the compositor,
so there is no app-level toggle yet. For 3D scenes,
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

## A row vanishes (or content overlaps) on a small terminal

**Cause**: flex overflow pressure crushed a node to zero area — content
demanded more rows/columns than the viewport has, and something had to
give. The engine's guarantees at any size: a zero-area node is
CLEAN ABSENCE — its draw closure never runs, so it can never smear onto a
sibling's row; `Modal` and `Drawer` clamp into the viewport at open and
re-clamp on every resize; tab strips window with overflow indicators; wide
glyphs never tear at a clip edge. In debug builds every zero-collapse is
named by a startup notice.

**Fix**: two app-side recipes. Give incompressible chrome (title bars,
button rows, status lines) an explicit `shrink(0.0)` so the oversized
MIDDLE gives instead — or wrap that middle in a `Scroll`, whose default
`basis(0)` exerts no pressure. And render `use_startup_notices` somewhere
visible: the engine names every collapsed node into that lane, and a
notice nobody renders is a debugging session someone else pays for. The
full contract: [api.md § "Small terminals & content pressure"](api.md#small-terminals--content-pressure).

## Double-click doesn't activate (in the app, or in a test)

**Cause**: several honest ones, in likelihood order. In a `Table`, a SLOW
second click is deliberate: activation needs a true double-click (second
press within 400 ms, within 1 cell, on the already-selected row) —
re-clicking a row to focus its pane must never open its editor. A second
press that drifted onto a NEIGHBOR row only re-selects (fast click-walking
is browsing, not commitment), and a wheel between clicks resets the chain
(the content under the cell moved). In a HEADLESS TEST, a bare `ui::UiTree`
has no time source, so every press deterministically counts 1 —
double-click needs time to flow.

**Fix**: in the app, none — Enter and Space always activate, and `List`'s
click-on-selected gesture is timing-free. In tests, drive through the real
`Driver` (it publishes its `set_clock`-injectable clock as the ambient
event time each turn, so one injected clock scripts animations AND
double-click timing), or opt a bare tree in with
`ui::set_event_time(Some(t))`. Custom input paths outside tree dispatch
embed their own `ui::ClickChain`. The full convention:
[api.md § "Double-click"](api.md#double-click).

## My screenshot shows a labeled veil where an image should be

**Cause**: honesty, not loss. Cells under a kitty/iTerm2/sixel placement
are not the picture — the terminal shows pixels the cell plane cannot see,
so `Driver::screenshot()` stamps those placements into
`Screenshot::pixel_regions()` and the SVG exporter draws a labeled
placeholder veil instead of pretending. Text and ANSI exports stay
cell-plane-verbatim; VT-model captures (headless tests) carry no regions
at all — the rig counts protocol payloads without modeling their pixels.

**Fix**: if the still must contain the picture, render the image through
the unicode-mosaic path for the capture — mosaic images ARE cells and
capture as themselves. Otherwise accept the veil: it marks exactly the
region the terminal owned. See
[api.md § "Screenshots & captures"](api.md#screenshots--captures).

## Dropping a file pastes a path instead of attaching it

**Cause**: that is all a terminal can do. There is no drop protocol —
every major terminal turns a file drop into a PASTE of the file's path,
each with its own quoting (backslash escapes, single or double quotes,
`file://` URIs). Without an intercept, the path lands in your composer
as text.

**Fix**: intercept the paste and classify it. `TextInput::on_paste` /
`TextArea::on_paste` run before insertion with the raw paste text;
`input::paste::classify` parses the known drop spellings and returns
the paths (or `None` for ordinary text — ambiguity always falls through
to a normal paste, so prose containing a path is never eaten):

```rust,ignore
TextArea::new()
    .on_paste(|pasted| match abstracttui::input::paste::classify(pasted) {
        Some(paths) => { attach(paths); PasteAction::Consume }
        None => PasteAction::Insert,
    })
```

Existence-checking is deliberately yours (the engine does no I/O in the
input path): fs-check the returned paths and show the result. If drops
still arrive as text, your terminal may be one whose drop spelling is
ambiguous by design — kitty pastes raw unescaped paths, so a path with
spaces cannot be told apart from prose; offer `FilePicker` as the
explicit door. The full walkthrough:
[api.md § File attachments](api.md#file-attachments--paste-intercept-drop-classifier-filepicker)
and `cargo run --example attachments`.

## I can't select text with the mouse

**Cause**: mouse capture. The engine enables SGR mouse reporting for
wheel scrolling and click routing, and a terminal in mouse-capture mode
sends drags to the *application* instead of performing its own text
selection. Every mouse-capturing TUI behaves this way — it is the
protocol, not a bug.

**Fix**: three answers, cheapest first.

1. **Hold the bypass modifier your terminal already ships.** Every major
   emulator can bypass mouse capture for one drag:

   | Terminal            | Bypass gesture                                  |
   |---------------------|-------------------------------------------------|
   | iTerm2              | Option+drag (also Cmd if configured)            |
   | macOS Terminal.app  | Fn+drag (Option+drag selects rectangles)        |
   | kitty               | Shift+drag                                      |
   | WezTerm             | Shift+drag                                      |
   | GNOME Terminal/VTE (incl. Tilix, xfce4-terminal) | Shift+drag         |
   | Alacritty           | Shift+drag                                      |
   | Windows Terminal    | Shift+drag                                      |
   | tmux (inside any of the above) | the same modifier, per the OUTER terminal |

   This selects raw screen cells — borders, gutters, and pane seams
   included — which is why the engine also offers the next two.

2. **A "native selection mode" keybinding** (engine tier 2): the app
   calls `app::selection::mouse_capture().suspend()` — mouse reporting
   turns off, the terminal's own selection (and clipboard) works at full
   native quality, and the app resumes with `.resume()` on its next
   keypress. See the [api.md selection section](api.md#appselection--screen-text-selection-and-clipboard-copy).

3. **Engine drag-select with OSC 52 copy** (tier 3): the app enables
   `app::selection::selection()`, and dragging paints a real selection
   highlight clamped to the pane under the anchor; releasing (or
   `c`/Enter/Ctrl+C) copies the selected screen text to the system
   clipboard through OSC 52. `cargo run --example feed` demonstrates it.
   A live selection freezes every follow-tail `Scroll` for the length of
   the drag, so a streaming transcript stops sliding under the highlight
   and the copy is the text you actually pointed at; clearing the region
   returns the pane to the live tail.

## The engine's copy doesn't reach my clipboard

**First, check the notices**: every copy that has a working route posts
one naming its size (`copied 240 characters (3 lines) to the
clipboard`). If you see that receipt, the copy left the engine intact
and the remaining suspects are the multiplexer and the terminal. If you
see the labeled clipboard warning instead, no route was available.

**Cause**: OSC 52 is a write-only, fire-and-forget escape — the terminal
either applies it or silently ignores it, and there is no reply to check.
The engine falls back to the host clipboard
(`RunConfig::platform_clipboard`, on by default) where OSC 52 is not
advertised. Common blockers: the terminal does not support OSC 52 and no
host helper is installed; tmux is in the middle (it consumes OSC 52
itself — `set -g set-clipboard on` in `~/.tmux.conf` lets it forward the
copy; the engine follows its verb policy and does not passthrough-wrap
OSC 52, because tmux handles the sequence natively); or a security
setting (some terminals gate clipboard writes behind a prompt or a
setting, e.g. `clipboard_control` in kitty).

**Fix**: check the notices first, then your multiplexer's
`set-clipboard`, then the terminal's clipboard permission setting. Size
is rarely the issue: screen selections are a few kilobytes and every
known OSC 52 cap (tmux's historical ~74KB, kitty's default 8MB) sits far
above them. As a last resort the modifier-bypass matrix above always
works — it never involves the application.

## Hover highlights never light up

**Cause**: hover ink needs the terminal to report pointer motion with no
button held (mode 1003). The default session arms button-and-drag
tracking (1002) instead, so `MouseEnter` / `MouseLeave` only arrive
during a drag. Clicks are unaffected — a control that looks dead on
hover still works when pressed.

**Fix**: opt in with `RunConfig::hover_ink`:

```rust
app.run_with(RunConfig {
    hover_ink: true,
    ..RunConfig::default()
})
```

It is off by default because 1003 sends a report for every pointer cell
crossed, which wakes the event loop of apps that have no hover visuals to
paint — noticeably so over SSH or tmux. Set it when your UI reacts to
hover (`List` row ink, `Button` hover, `ThemeSwitcher`'s glyph), and
leave it off otherwise. Setting it keeps kitty-keyboard auto-detection;
hand-building `EnterOptions` to reach `MouseMode::AnyMotion` would give
that up.

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

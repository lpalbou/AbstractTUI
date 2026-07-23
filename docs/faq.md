# FAQ

## Why another TUI library?

Most terminal UI libraries make one of two bets: immediate-mode (redraw
everything every frame, diff at the end) or a retained widget tree with
coarse invalidation. AbstractTUI makes a different one: **fine-grained
reactive signals driving a layered compositor with damage tracking**.
State lives in signals; only the regions that read a changed signal
re-render; the compositor diffs only damaged cells; an idle app sits in a
blocking read at zero CPU. On top of that sits capability-driven graphics
(real images and software-rasterized 3D with labeled fallbacks) and a
36-token theme system with enforced contrast floors. If your app is a
short-lived form, simpler designs are fine; AbstractTUI is built for
long-running, composed, animated applications that should still cost
nothing when nothing happens.

## How does AbstractTUI compare to ratatui, Textual, Bubble Tea, notcurses, and Ink?

They all build full-screen terminal UIs; the architectural split is how
updates reach the screen. Most established libraries are immediate-mode or
reconciliation-based: the application (or a component layer above it)
rebuilds its view every frame or every state change, and the library diffs
the result before writing bytes. AbstractTUI is neither — state lives in
fine-grained signals, a write re-runs exactly the computations that read
it, and those re-renders damage only the screen regions they own, flowing
through a z-ordered compositor (alpha blending, per-cell shaders), a frame
diff, and a byte-economical presenter. There is no per-frame rebuild and
no tree reconciliation, and the idle cost is exactly zero — an unchanged
app emits zero bytes and allocates nothing, enforced by tests
([architecture.md](architecture.md#the-damage-promise)).

| Library | Language | Update and rendering model |
| --- | --- | --- |
| ratatui | Rust | Immediate mode: rebuild the widget tree each frame into a buffer, diff at present time. The largest Rust TUI ecosystem; event loop and async are yours to bring. |
| Textual | Python | Retained DOM-like widget tree with CSS styling and reactive attributes on an asyncio loop; apps can also be served to a browser. |
| Bubble Tea | Go | The Elm Architecture: messages update a model, the view renders text, the runtime diffs frames; bubbles/lipgloss supply components and styling. |
| notcurses | C | Imperative stacked planes with best-in-class terminal media (images, video, sixel/kitty); bindings for many languages; a thin widget layer. |
| Ink | JS/TS | React for the terminal: components and hooks reconciled through a virtual DOM onto Yoga flexbox; strongest in Node CLI tooling. |
| AbstractTUI | Rust | Fine-grained signals drive damage into a layered compositor, then a frame diff and presenter; layout is a flexbox-style solver plus a track grid. |

Two other differences are structural rather than stylistic. First, media:
images (kitty/iTerm2/sixel/unicode-mosaic ladder), software-rasterized GLB
3D, and a sub-cell vector canvas are in the core crate with hand-rolled
decoders — five small dependencies total; in the ecosystems above,
comparable capability is external (ratatui-image), C-library-bound
(notcurses), or absent. Second, testing: the headless harness drives the
production pipeline — real dispatch, focus, damage, and emitted bytes —
against an in-memory terminal with a VT interpreter, and `Screenshot`
exports deterministic text, replayable ANSI, or SVG for goldens and
round-trips, without a pty.

The honest limits, stated plainly: this is a young project (first public
release 2026-07-21) with a small ecosystem next to ratatui's, one core
crate plus two extension crates, and few third-party widgets. Windows is
compile-checked and unit-tested but not yet exercised on live hardware
(macOS and Linux are the verified platforms). Accessibility roles and
labels exist in the semantic tree, but no screen-reader bridge consumes
them yet. There is no async-runtime integration — background work crosses
on threads via wake handles, which is deliberate but means no built-in
HTTP/WebSocket client. If you need battle-tested breadth today, ratatui
(Rust) or Textual (Python) are the mature choices; AbstractTUI's case is
compositor-grade rendering, zero idle cost, and built-in media in one
audit-in-a-sitting dependency graph.

## Does it work over SSH?

Yes. Everything the engine does travels as bytes over the pty, which is
exactly what SSH carries. Capabilities are detected from the terminal that
is actually attached — your local emulator — via an environment pass plus
an active query probe, so color depth, image protocols, and keyboard
enhancements reflect what your end of the connection supports. Expect the
same feature set you would get locally in the same emulator; only latency
changes.

## Which terminals support images?

Anything, at some rung of the ladder. Kitty-protocol terminals get the
best channel (upload once, cheap moves, true deletes); iTerm2-protocol
terminals get inline images with full re-emits; sixel terminals get
paletted rasters; **every** terminal gets unicode mosaic, which is plain
colored glyphs and needs nothing. The engine probes and picks; run
`cargo run --example caps` for the live capability report (the
`images via` line names your channel), and `cargo run --example images`
to see the result. See [graphics-and-3d.md](graphics-and-3d.md) for the
full ladder and the per-terminal expectations.

## Why is my emoji/wide-character layout off in one terminal?

Because terminals genuinely disagree about the width of some characters.
Emoji presentation sequences (VS16), ZWJ families, and East-Asian-Ambiguous
symbols render at different widths across emulators and configurations —
there is no protocol to ask. The engine measures with a consistent width
policy and defends its cursor after emitting a risky cluster, so a
disagreement stays confined to that cluster instead of smearing everything
after it on the line. If your terminal is configured ambiguous-wide, cell
layout of every TUI breaks regardless; prefer the terminal's default width
configuration, and prefer unambiguous glyphs in structural chrome.

## How do I test my app headlessly?

Drive the same pipeline production uses against a captured terminal — no
tty needed:

```rust
use abstracttui::app::Driver;
use abstracttui::testing::CaptureTerm;

let mut term = CaptureTerm::new(size);
let mut driver = Driver::new(&mut app, &mut term, cfg)?;
driver.turn(&mut app, &mut term)?;             // one full frame cycle
assert!(term.screen().to_text().contains("n = 0"));
term.push_input(b"+");                          // bytes, as a terminal would send
driver.turn(&mut app, &mut term)?;
```

`CaptureTerm` records the emitted bytes and models the screen, so you
assert on rendered text (or raw bytes) with every dispatch, focus, and
damage path being the real one. For pure component tests, skip the driver:
mount into a `ui::UiTree`, dispatch events, draw into a buffer canvas.

## How do I capture a screenshot of my app?

Capture the screen as a plain value and export it: `Screenshot` (in the
prelude) exports deterministic plain text (`to_text`), replayable ANSI you
can `cat` back into a terminal (`to_ansi`), and a GitHub-renderable SVG
(`to_svg`). Three capture surfaces: `driver.screenshot()` for embedders
and tests (the frame as last presented), `app::request_screenshot(cb)`
from any key handler (bind your own key — there is deliberately no engine
default), and `term.screen().screenshot()` in headless tests (what the
emitted bytes actually produced). One honesty rule: pixel-protocol image
regions export as labeled veils, never as fake cells. See
[api.md § "Screenshots & captures"](api.md#screenshots--captures) and
`cargo run --example screenshot`.

## Can I draw custom vector graphics — or render graphs and diagrams?

For hand-rolled traces, the public sub-cell canvas (`DotCanvas`, in the
prelude) gives you braille/quadrant dot grids with line/bezier/arc strokes
and eighth-block fills — the same layer the shipped charts draw through
([api.md § canvas](api.md#canvas--canvas--vector-strokes)). For
node-and-edge diagrams, don't hand-stroke: the sibling crates
`abstracttui-graph` (auto-layout + `GraphView`) and `abstracttui-mermaid`
(honest mermaid subset) install only when you need them — see
[graphs-and-diagrams.md](graphs-and-diagrams.md).

## Can I embed AbstractTUI in an existing event loop?

Yes. `App::run` is a convenience, not a requirement. `Driver::turn` runs
exactly one frame cycle and never blocks — the blocking edge is a separate
wait call, so your own loop decides when to pump. Headless surfaces
(`pump`, `draw`) drive the reactive and layout pipeline without a terminal
at all, and the unix terminal can be constructed over explicit file
descriptors for embedders.

## Why the near-zero dependency policy?

The dependency policy is a hard rule: `std` plus a minimal, low-level,
permissively-licensed set — `unicode-width`, `unicode-segmentation`,
`miniz_oxide` (inflate for PNG), and the platform bindings (`libc` on
unix, `windows-sys` on Windows). Everything else is hand-rolled: ANSI
emission, the input parser, the flexbox solver, the signals runtime, JSON
parsing for glTF, PNG chunking and defiltering, JPEG decode, base64, sixel
encoding, and the 3D math and rasterizer. The payoff is a dependency graph
you can audit in one sitting, fast clean builds, no feature-flag matrix,
and behavior that changes only when this crate changes.

## How do themes stay readable?

Every theme — built-in or registered at runtime — is audited against
WCAG-derived contrast floors: body text at 4.5:1, muted text at 3:1,
accents and semantic marks at 3:1, selection text at 4.5:1, and so on down
to hairline borders at 1.5:1. The built-in family passes with zero
violations as a test invariant, and `theme::register` runs the same audit
on your themes — refusing in strict mode or labeling every finding in
labeled mode. See [theming.md](theming.md#contrast-guarantees) for the
full table.

## What happens on a dumb terminal, or with NO_COLOR?

Both are honored. `TERM=dumb` (or an empty `TERM`) marks the terminal as
not worth escaping at: the active capability probe is skipped entirely and
the splash refuses to play. `NO_COLOR` forces color depth down regardless
of what the terminal supports, and the raw fact is surfaced so themes can
react. On limited-color terminals, the presenter quantizes to the 256- or
16-color palette pairwise — foreground and background are re-picked
together so text never vanishes into its own background.

## Is Windows supported?

Best-effort, honestly labeled. macOS and Linux are the verified platforms:
every unix code path is exercised by live pty tests, including
signal-driven resize, job-control suspend, and keystroke flow under a real
controlling terminal. The Windows backend compiles cleanly against the
MSVC target, its platform-independent logic (UTF-16 pairing, wake
latching, resize dedupe) is unit-tested on every host, and its console
usage follows Microsoft's documented semantics — but it has not been
exercised on live Windows hardware. Treat a first Windows
deployment as a beta event. (One concrete difference: `suspend()` returns
an explicit Unsupported error on Windows — hide the Ctrl+Z binding there.)

## How big is the crate?

One crate, no feature flags, no build script, three small library
dependencies plus the platform bindings. The source is roughly 105k lines
of Rust including its extensive inline test suites — decoders, rasterizer,
layout solver, and signals runtime included, since none of that is pulled
in from elsewhere.

## Can widgets be shared as libraries?

Yes — a component is a plain function, so it ships like any Rust code. The
convention: a props struct (with `Callback<T>` fields for typed events out
and `View` fields for slots), a function that takes `Scope` and props and
returns a `View`. `Callback::default()` is a no-op, so optional events
cost nothing to leave unbound. The `components` example is the heavily
commented reference: three reusable components composed repeatedly with
different props into a settings screen.

## How do I see what is actually repainting?

The compositor has a damage visualizer
(`render::Compositor::set_debug_damage(true)`) that outlines exactly the
regions each frame repaints. The switch lives on the compositor itself, so
today it is for embedders driving the render pipeline directly
([api.md § render](api.md#render--surfaces-and-paint-advanced)) — under
`App::run` the driver owns the compositor and exposes no toggle yet. The
signal-side diagnosis needs no visualizer: if a "static" screen keeps
repainting, something is writing a signal it shouldn't (every `dyn_view`
that reads it re-renders) — audit the writes before reaching for a
profiler. Perf numbers only mean anything in `--release` builds.

## Why can my app write the clipboard but not read it?

By design. Copy uses OSC 52 (gated on detection, since some terminals
silently ignore it, and success is only reported when the capability
holds). The **read** form of OSC 52 is deliberately never emitted: it
would let any full-screen application silently read the user's clipboard —
a data-exfiltration vector. Paste reaches your app exclusively through
bracketed paste, which is fuzz-hardened: multi-megabyte pastes stream in
bounded chunks, byte-exactly, with embedded escape sequences neutralized
as content.

Writing is easy to reach: `copy_to_clipboard(text)` from any handler, or
enable the engine's drag-select (`selection()`) so users copy what they
see — both in the
[api.md selection section](api.md#appselection--screen-text-selection-and-clipboard-copy).
If a copy never arrives, see
[troubleshooting](troubleshooting.md#the-engines-copy-doesnt-reach-my-clipboard).

## Why doesn't Ctrl+Enter (or Shift+Enter) do anything?

On the classic terminal wire, Ctrl+Enter, Shift+Enter, and Ctrl+Backspace
are byte-identical to plain Enter / Ctrl+H — no parser can recover what
the terminal never sent. They become distinct under the kitty keyboard
protocol or xterm's modifyOtherKeys, both of which the engine detects and
decodes automatically. Treat these chords as enhancements, not baseline
bindings; everything on arrows, Home/End, PgUp/PgDn, and F1–F12 with any
modifier combination is reliable everywhere.

## Does it support the mouse?

Yes: SGR-encoded mouse events (clicks, drags, wheel) in cell coordinates
on every supported terminal, hover/click affordances in the built-in
widgets, pointer capture for drags (the 3D viewport uses it for orbiting),
and pixel-precision reporting where the terminal verifiably supports it —
raw pixel coordinates ride alongside cell coordinates only when pixel
reporting is actually active, never posing as cells.

## How do I let users pick a theme?

The one-line answer is `ThemeSwitcher` (in the prelude): mount
`ThemeSwitcher::new().view(cx)` in any header or footer row and you get
a ☾/☼ menu button whose popup lists every visible theme grouped
Dark/Light with live preview; `ThemeSwitcher::toggle()` is the
no-popup face that flips dark ↔ light per click, restoring your last
theme of the target mode. `on_change` is the hook for persisting the
preference. See [theming.md](theming.md#theme-modes--the-switcher).

For your own picker UI: `set_theme_by_id(id)` switches at runtime and
the whole app restyles through the one theme signal; `theme::list()`
gives you `(id, label, dark)` for every visible theme, including ones
your app registered, and `theme::themes_by_mode(mode)` lists one
polarity in curated order. The shipped examples honor
`ABSTRACTTUI_THEME=<id>` as a startup convention, and
`cargo run --example themes` is a complete picker UI — card grid, live
preview, measured contrast ratios — you can crib from.

## Can users attach files by dropping them onto the terminal?

Yes, with one honest caveat: terminals have no drop protocol — dropping
a file PASTES its path, with per-terminal quoting. The engine gives you
the pieces to turn that into a real attach flow: `TextInput::on_paste` /
`TextArea::on_paste` intercept a paste before insertion, and
`input::paste::classify` answers "is this paste a file drop?" against
the researched spellings of the major terminals (ambiguous input always
falls through to a normal paste — a false positive would eat user
text). `FilePicker` is the explicit browse-and-pick door for when there
is nothing to drag from. The wired recipe is
`cargo run --example attachments`; the API walkthrough is
[api.md § File attachments](api.md#file-attachments--paste-intercept-drop-classifier-filepicker).

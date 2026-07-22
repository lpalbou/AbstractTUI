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
`cargo run --example images -- --caps` to see what your terminal offers.
See [graphics-and-3d.md](graphics-and-3d.md) for the full ladder.

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
dependencies plus the platform bindings. The source is roughly 65k lines
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

The compositor has a damage visualizer: `set_debug_damage(true)` outlines
exactly the regions each frame repaints. If a "static" screen shows
outlines every frame, something is writing a signal it shouldn't — that is
your profiling starting point, before reaching for a profiler. Perf
numbers only mean anything in `--release` builds.

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

`set_theme_by_id(id)` switches at runtime and the whole app restyles
through the one theme signal; `theme::list()` gives you `(id, label,
dark)` for every visible theme, including ones your app registered. The
shipped examples honor `ABSTRACTTUI_THEME=<id>` as a startup convention,
and `cargo run --example themes` is a complete picker UI — card grid, live
preview, measured contrast ratios — you can crib from.

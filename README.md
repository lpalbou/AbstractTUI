# AbstractTUI

**The terminal, composed.** A reactive, compositor-grade terminal UI engine for Rust.

AbstractTUI is built on fine-grained reactive signals — not immediate mode, not a
virtual DOM. Reading a signal inside a view tracks it; writing one re-runs exactly
the computations that depended on it, and those re-renders damage exactly the
screen regions they own. Damaged regions flow through a real compositor
(z-ordered layers, alpha blending, per-layer offset/opacity), a frame diff, and a
byte emitter that plays the terminal like an instrument — cursor-motion economy,
minimal SGR runs, synchronized output where the terminal supports it. The result:
an idle app emits zero bytes and allocates nothing, and a blinking cursor repaints
one cell, not the screen.

```text
 ▲ AbstractTUI  ops dashboard                                                          Dark (Abstract)  ·  12:34:56 UTC
 ┌ nav ────────┐  ┌ traffic — rx/tx (MB/s) ──────────────────────────────────────┐  ┌ load ─────────────────────────┐
 │  overview   │  │ ── rx   ── tx                                                │  │ mem                       54% │
 │  traffic    │  │100 │                                                      ⣀⢄⢀│  │████████████████▊              │
 │  sessions   │  │    │⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⢄⡠⠔⠉⠊⠑⠊ ⠈⠁│  │⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒│
 │  logs       │  │    │⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠊⠉⠑⠒⠔⠒⠒⠤⠤⢄│  │ io                        45% │
 │  alerts     │  │0   │                                                         │  │█████████████▉                 │
 │  settings   │  │    └─────────────────────────────────────────────────────────│  │⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤⠤│
 │             │  └──────────────────────────────────────────────────────────────┘  └───────────────────────────────┘
 │             │
 │             │  ┌ events ───────────────────────────┐  ┌ sessions — s toggles sort ────────────────────────────────┐
 │             │  │                                   │  │host         region   rx▼            tx            state   │
 │             │  │                                   │  │edge-2       eu-w     41.3 MB/s      20.8 MB/s     healthy │
 │             │  │                                   │  │core-a       us-e     41.1 MB/s      18.6 MB/s     syncing │
 │             │  │                                   │  │edge-1       eu-w     38.7 MB/s      7.2 MB/s      healthy │
 │             │  │                                   │  │edge-3       us-e     38.1 MB/s      23.6 MB/s     syncing │
 │             │  │                                   │  │cache-2      ap-s     25.2 MB/s      16.5 MB/s     healthy │
 │             │  │                                   │  │core-b       ap-s     22.8 MB/s      8.4 MB/s      syncing │
 │             │  │                                   │  │cache-1      eu-n     13.4 MB/s      19.9 MB/s     healthy │
 │             │  │ 00:00 info  session opened from … │  │                                                           │
 │             │  │ 00:00 ok    tls renewed for gate… │  │                                                           │
 │             │  │ 00:01 ok    backup verified       │  │                                                           │
 │             │  │ 00:01 info  session opened from … │  │                                                           │
 │             │  │ 00:02 info  session opened from … │  │                                                           │
 │             │  │ 00:02 warn  backpressure on shar… │  │                                                           │
 │             │  │ 00:03 ok    shard 2 caught up     │  │                                                           │
 └─────────────┘  └───────────────────────────────────┘  └───────────────────────────────────────────────────────────┘

 tab focus  alt+←→ panes  s sort  n toast  b mark  ? help  ctrl+t theme  q quit
```

*The `dashboard` example at 120×35 (abridged; from `docs/captures/`, regenerable
with `cargo run --example capture`).*

## Highlights

- **Widgets + layout** — buttons, text inputs, lists, sortable tables, tabs,
  checkboxes, radio groups, scroll regions, panels, badges, progress bars,
  spinners, modals, toasts — arranged by a flexbox-style solver (row/column,
  `grow`, `gap`, padding) and a track-based grid (`fr`/cells/percent, spans).
- **26 built-in themes** — catppuccin, rose-pine, tokyo-night, nord, one-dark,
  dracula, monokai, gruvbox, solarized, everforest and the Abstract originals —
  over 36 semantic design tokens, contrast-audited against WCAG floors, and
  hot-swappable at runtime through one signal.
- **Input everywhere** — keyboard and mouse (click, hover, drag, wheel), the
  kitty keyboard protocol and xterm modifyOtherKeys decoded automatically when
  present, bracketed paste hardened against multi-megabyte and hostile input,
  focus events, key chords with modifiers.
- **Images** — PNG and baseline JPEG decoding built in, drawn through the best
  channel your terminal offers: kitty graphics, iTerm2, sixel, or unicode mosaic
  (half-block / quadrant / sextant / braille). Capability detection is automatic
  and every degradation is labeled, never silent.
- **Software-rasterized 3D** — load GLB files (node hierarchies, textures,
  vertex colors, animation, skinning) and render them into the same cell
  pipeline. No GPU, no native dependencies.
- **Motion** — cell shaders (shimmer, dissolve, hue-drift, and more) that cost
  work only where damage exists, plus tweens, easings, and timelines.
- **A boot identity** — a 2-second animated splash (3D mark with a pure-cell 2D
  fallback), skippable with any key, auto-disabled on non-TTY, `NO_COLOR`, and
  `TERM=dumb`.
- **Headless testing** — drive the production pipeline against a captured
  terminal and assert on the rendered screen. No pty required.

## Your first app

Sixteen lines, one import:

```rust
use abstracttui::prelude::*;
use abstracttui::widgets::Button;

fn main() -> abstracttui::base::Result<()> {
    App::simple(|cx| {
        let count = cx.signal(0);
        Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("count: {}", count.get()))
            }))
            .child(Button::new("+1").on_click(move || count.update(|c| *c += 1)).view(cx))
            .child(text("Tab focuses · Enter clicks · Ctrl+C quits"))
            .build()
    })
}
```

Tab focus, Enter/Space activation, and Ctrl+C quit are all defaults. The count
line re-renders fine-grained through `dyn_view` — nothing else repaints. The
walkthrough lives in [docs/getting-started.md](docs/getting-started.md).

## Install

```sh
cargo add abstracttui
```

Rust 2021 edition. The dependency policy is deliberately austere — `unicode-width`,
`unicode-segmentation`, `miniz_oxide`, plus `libc` on unix / `windows-sys` on
Windows, and nothing else. ANSI emission, input parsing, the layout solver, the
signals runtime, PNG/JPEG decoding, glTF parsing, and the 3D rasterizer are all
implemented in-crate.

## Run the examples

```sh
git clone https://github.com/lpalbou/abstracttui
cd abstracttui
cargo run --example dashboard
```

Twelve runnable examples live in [examples/](examples/README.md), and every one
exits cleanly with a notice when no interactive terminal is present, so they are
safe to run anywhere. Start with these five:

- `dashboard` — the flagship ops screen: charts, log tail, sortable table,
  toasts, modal help, spatial pane navigation.
- `gallery` — the whole design system on one screen; one keypress restyles it.
- `themes` — every theme as a live card grid with a preview pane and measured
  contrast ratios.
- `viewer3d` — orbit a GLB model with measured fps
  (`cargo run --example viewer3d -- path/to/model.glb`).
- `images` — four mosaic families side by side, dithering, and pixel-protocol
  placement with the chosen channel named.

`ABSTRACTTUI_THEME=rose-pine cargo run --example hello` themes any example from
the environment; `--caps` on `dashboard`, `viewer3d`, and `images` prints the
detected capability report and exits.

## Platform support

| Platform | Status |
| --- | --- |
| macOS | Verified — the full test suite includes live pty tests (real controlling terminal, signal-driven resize, suspend/resume). |
| Linux | Verified — same unix code paths and pty coverage. |
| Windows | Best-effort — compiles clean and lint-free against the MSVC target, and the platform-independent logic is unit-tested on every host, but it has not yet been run on a live Windows console. Treat the first Windows run as a beta event. |

The terminal is always restored — on quit, on panic, and on Ctrl+Z suspend —
including cursor style, mouse modes, and kitty keyboard flags.

## Performance

Measured (release build, M-class laptop): a full 200×60 diff+present costs
~0.5 ms, a keystroke reaches the painted frame in ~50 µs through the real event
loop, and an idle app costs zero — zero bytes written, zero heap allocations,
zero wakeups. These are enforced by in-tree perf budgets and allocation-counting
tests, not aspirations.

## Documentation

- [Getting started](docs/getting-started.md) — install to first pixels, step by step.
- [Architecture](docs/architecture.md) — signals, damage, the compositor, the render pipeline.
- [API guide](docs/api.md) — the public surface, module by module.
- [FAQ](docs/faq.md) and [Troubleshooting](docs/troubleshooting.md).
- [Examples catalog](examples/README.md) — what each demo proves and the keys it answers to.
- API reference on [docs.rs](https://docs.rs/abstracttui).

## License

MIT — see [LICENSE](LICENSE).

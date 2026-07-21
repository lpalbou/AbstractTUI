# Getting started

From install to first pixels, step by step. Every snippet here compiles against
the current release; the longer ones are lifted from the crate's own doctests,
which run in CI.

## Install

```sh
cargo add abstracttui
```

AbstractTUI targets the Rust 2021 edition and builds on macOS, Linux, and
Windows with a deliberately small dependency set (`unicode-width`,
`unicode-segmentation`, `miniz_oxide`, plus the platform FFI crate). There is no
native library to install and no GPU requirement.

## Your first app

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

Line by line:

- **`use abstracttui::prelude::*;`** — one import covers the common path:
  widgets, layout vocabulary, signals, hooks, and `App` itself.
- **`App::simple(|cx| ...)`** — builds the app, mounts your root component,
  enters the raw terminal, and runs the event loop until quit. The `cx`
  parameter is your `Scope`: the handle you create reactive state with. A panic
  hook is installed first, so the terminal is restored even if your code panics.
- **`let count = cx.signal(0);`** — a `Signal`. Signals are small `Copy`
  handles, so you can move them into as many closures as you like. `.get()` is
  a tracked read; `.set(v)` and `.update(|v| ...)` write and notify exactly the
  computations that read it.
- **`Element::new().style(LayoutStyle::column())`** — the element tree. Styles
  describe layout (direction, size, grow, gap, padding); children stack top to
  bottom in a column.
- **`dyn_view(LayoutStyle::line(1), move || ...)`** — a reactive region. The
  closure re-runs whenever a signal it reads changes, and only this region's
  cells are redrawn. `LayoutStyle::line(1)` is a full-width, one-row slot.
- **`Button::new("+1").on_click(...).view(cx)`** — a widget builder. `.view(cx)`
  resolves the active theme from context and returns a finished `View`, ready
  for `.child(..)`. Every themed widget in the prelude supports this shape.
- **`text(...)`** — a static text leaf. It never re-renders because it reads no
  signals.
- **Defaults you did not write** — Tab/Shift+Tab move focus, Enter and Space
  activate the focused widget, Ctrl+C quits. Override any of them by consuming
  the event in a handler or shortcut.

## Adding interactivity

`TextInput` binds to a `Signal<String>` and reports edits through `on_change`
(after every edit) and `on_submit` (on Enter). Combined with `dyn_view`, state
flows from keystroke to screen with no wiring in between:

```rust
use abstracttui::prelude::*;

fn form(cx: Scope) -> View {
    let name = cx.signal(String::new());
    let saved = cx.signal(false);

    Element::new()
        .style(LayoutStyle::column().gap(1).padding(Edges::all(1)))
        .child(
            TextInput::new()
                .placeholder("your name")
                .value(name)                          // bind an external signal
                .on_change(move |_| saved.set(false)) // any edit invalidates
                .view(cx),
        )
        .child(Button::new("Save").on_click(move || saved.set(true)).view(cx))
        .child(dyn_view(LayoutStyle::line(1), move || {
            let status = if saved.get() { "saved" } else { "unsaved" };
            text(format!("{} — {}", name.get(), status))
        }))
        .build()
}
```

Run it with `App::simple(form)`. Tab moves between the input and the button;
the input handles cursor movement, selection (Shift+arrows), word jumps
(Alt+arrows), and paste for you. Mouse clicks focus and activate the same
widgets — no extra code.

## Layout basics

Layout is a flexbox-style solver. The vocabulary: `LayoutStyle::column()` /
`LayoutStyle::row()` set direction, `gap(n)` spaces children, `padding(Edges)`
insets content, and sizes come from `Dimension::Cells(n)`, `Dimension::Percent(f)`,
or `grow`:

```rust
// A fixed sidebar and a growing main pane.
Element::new()
    .style(LayoutStyle::row().gap(1).padding(Edges::all(1)))
    .child(
        Element::new()
            .style(LayoutStyle::column().width(Dimension::Cells(20)))
            .child(text("sidebar"))
            .build(),
    )
    .child(
        Element::new()
            .style(LayoutStyle::column().grow(1.0)) // takes the remaining width
            .child(text("main"))
            .build(),
    )
    .build()
```

The rule for multi-pane layouts: give every pane that should share leftover
space a `grow`, and fixed panes an explicit size. A pane with neither takes
only its content size. `LayoutStyle::fill()` (fill the parent on both axes) and
`LayoutStyle::line(n)` (full width, `n` rows) cover the two most common shapes.

For two-dimensional layouts there is a track-based `Grid` — columns and rows
declared as `Track::Cells(n)`, `Track::Percent(f)`, `Track::Fr(f)`, or
`Track::Auto`, with row-major auto-placement and spans:

```rust
// A label column and a growing field column; children fill row by row.
Grid::new(vec![Track::Cells(12), Track::Fr(1.0)], vec![])
    .gap(1)
    .child(text("Name:"))
    .child(TextInput::new().view(cx))
    .child(text("Notes:"))
    .child(TextInput::new().view(cx))
    .view()
```

The `grid` example (`cargo run --example grid`) cycles three track recipes over
the same children — the fastest way to build intuition.

## Theming in 3 lines

```rust
set_theme_by_id("nord");           // switch — every themed region repaints
let theme = use_theme(cx);         // reactive handle to the active theme
let tokens = theme.get().tokens;   // 36 semantic color tokens (bg, text, accent, …)
```

Widgets never name colors; they consume semantic tokens (`bg`, `surface`,
`text`, `accent`, `ok`/`warn`/`error`, chart slots, syntax inks, …), so one
`set_theme_by_id` restyles the entire app. 26 themes ship built in — try
`cargo run --example themes` for a live picker with measured contrast ratios,
or set `ABSTRACTTUI_THEME=<id>` in the environment (the convention every
example honors). Custom themes register at runtime through `theme::register`,
which audits contrast floors and either refuses or labels violations,
depending on the mode you choose.

## Showing an image

Decode once, wrap in an `Arc`, hand it to the `Image` widget:

```rust
use std::sync::Arc;
use abstracttui::gfx;

let bytes = std::fs::read("photo.jpg")?;
let bitmap = Arc::new(gfx::decode_image(&bytes)?);
```

```rust
use abstracttui::widgets::ImageFit;

// In your component:
Image::from_bitmap(bitmap.clone())
    .fit(ImageFit::Contain) // largest size that fits, aspect preserved
    .view(cx)
```

`gfx::decode_image` sniffs the actual bytes (containers lie, bytes don't) and
decodes PNG or baseline JPEG; unknown formats are rejected by name, never with
a panic. The widget renders unicode mosaic — colored sub-cell glyphs that work
in any terminal — and picks the best mosaic mode for the terminal's detected
glyph and color support. For pixel-perfect output over the kitty, iTerm2, or sixel
protocols, `gfx::ImageSession` manages placement at the app level; the
`images` example (`cargo run --example images`) shows both paths side by side,
naming the channel it chose.

## A 3D teaser

```rust
use std::sync::Arc;
use abstracttui::three;

let view = three::quick_view("model.glb")?; // load + framed camera + light
let model = Arc::new(view.model);
```

```rust
// In your component:
Viewport3D::new(model.clone())
    .orbit(0.6, 0.35, 1.0) // yaw, pitch, zoom — drive these from signals
    .view(cx)
```

`three::quick_view` loads a GLB file with a camera framed on the model's bounds
and a default light. `Viewport3D` software-rasterizes the scene into cells:
drag orbits, the wheel zooms, and the widget reports deltas through
`.on_orbit`/`.on_zoom` so camera state lives in your signals. Animated,
skinned models play through `.animate(clip, t)`. The loader supports binary
GLB with triangle meshes, embedded PNG/baseline-JPEG textures, LINEAR/STEP
animation, and 4-joint skinning; unsupported features are rejected by name or
degraded with a label, never guessed at. `cargo run --example viewer3d --
path/to/model.glb` is the full viewer, with measured fps in the status row.

## Plain terminals vs feature-rich terminals

You write one app; the engine adapts it to the terminal it finds. Capabilities
(color depth, kitty keyboard/graphics, sixel, synchronized output, pixel
geometry) are detected in two passes — an instant environment pass for the
first frame, then an active probe that can both raise and lower the answer
based on what the terminal actually replies. Everything degrades in the open:

- Truecolor styling steps down to 256 or 16 colors; `NO_COLOR` and `TERM=dumb`
  are honored.
- Images step down the protocol ladder to unicode mosaic, which works
  everywhere. Inside tmux, pixel protocols are enabled only after a live
  passthrough probe proves they arrive.
- Key combos like Ctrl+Enter or Shift+Enter exist only on terminals with the
  kitty protocol or modifyOtherKeys (legacy terminals send bytes identical to
  plain Enter) — treat them as enhancements, and keep baseline bindings on
  keys that work everywhere: arrows, Home/End, PgUp/PgDn, F1–F12.

Every degradation is recorded as a labeled startup notice. Read them
reactively with the `use_startup_notices(cx)` hook and render them in a status
line or toast — the `dashboard` example does exactly that, and its `--caps`
flag prints the full capability report without needing a tty.

## Testing your app headlessly

No pty needed: drive the same pipeline production uses against a captured
terminal, feed input as bytes, and assert on the rendered screen. This is the
crate's own doctest on `App`:

```rust
use abstracttui::prelude::*;
use abstracttui::app::Driver;
use abstracttui::testing::CaptureTerm;

let size = Size::new(20, 4);
let mut app = App::new(size);
app.mount(|cx| {
    let n = cx.signal(0);
    Element::new()
        .shortcut(KeyChord::plain(Key::Char('+')), move |_| n.update(|v| *v += 1))
        .child(dyn_view(LayoutStyle::line(1), move || text(format!("n = {}", n.get()))))
        .build()
}).unwrap();

let mut term = CaptureTerm::new(size);
let cfg = RunConfig { probe: false, ..RunConfig::default() };
let mut driver = Driver::new(&mut app, &mut term, cfg).unwrap();
driver.turn(&mut app, &mut term).unwrap();          // first frame
assert!(term.screen().to_text().contains("n = 0"));

term.push_input(b"+");                              // a keypress
driver.turn(&mut app, &mut term).unwrap();          // dispatch + repaint
assert!(term.screen().to_text().contains("n = 1"));
```

`CaptureTerm` records every byte and models the screen; `Driver::turn` runs one
full frame cycle — input dispatch, effects, layout, damage-driven redraw, diff,
present. Every focus and damage path is the real one. For pure component tests
you can skip the driver entirely: mount into a `ui::UiTree`, dispatch events,
and draw into a `ui::BufferCanvas` — every widget suite in the crate is written
that way.

## Where next

- [Architecture](architecture.md) — signals, the damage contract, the
  compositor, and the render pipeline.
- [API guide](api.md) — the public surface, module by module.
- [FAQ](faq.md) and [Troubleshooting](troubleshooting.md).
- [Examples catalog](../examples/README.md) — twelve runnable programs, from
  the 53-line `hello` to the full `dashboard`, with the keys each answers to.

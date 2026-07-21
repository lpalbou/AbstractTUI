# AbstractTUI API Guide

A guided tour of the public API, module by module. This is not a reference —
the item-by-item rustdoc is the reference (`cargo doc --open`, or browse
[docs.rs](https://docs.rs/abstracttui)). The goal here is orientation: what
each module is for, the types you will actually touch, and the idioms the
engine expects. Snippets are lifted from the crate's compiled doctests
wherever possible, so they match the shipped code.

## The prelude

`use abstracttui::prelude::*;` is all an application needs for the common
path. The prelude is curated to the app-code surface only: engine and test
types (`UiTree`, `Driver`, `create_root`, canvases) stay behind explicit
imports. One deliberate absence: `render::Style` is not exported, because two
`Style` types one glob apart is a trap. Layout style is exported as
`LayoutStyle` (box geometry — direction, size, gap); paint style is spelled
`render::Style` in full, inside draw closures, where it belongs.

## reactive — signals, memos, effects

`Signal<T>` is tracked state, `Memo<T>` is derived state, and an effect is a
computation that re-runs when anything it read changes. Handles are `Copy`;
state is owned by the `Scope` that created it and dies when that scope is
disposed. `batch` coalesces writes so effects observe one consistent world;
`untrack` reads without subscribing. The model in one compiled example:

```rust
use abstracttui::reactive::{batch, create_root};
use std::{cell::RefCell, rc::Rc};

let log = Rc::new(RefCell::new(Vec::new()));
let (root, ()) = create_root(|cx| {
    let count = cx.signal(0);
    let doubled = cx.memo(move || count.get() * 2);
    let log2 = log.clone();
    cx.effect(move || log2.borrow_mut().push(doubled.get()));
    count.set(3);
    batch(|| {
        count.set(4);
        count.set(5); // coalesced: the effect sees only 10
    });
});
assert_eq!(*log.borrow(), vec![0, 6, 10]);
root.dispose();
```

(`create_root` is the standalone entry point; inside an app, `App::mount`
hands your component a ready `Scope`.) Two time-aware helpers round out the
module: `animate(cx, source, easing, duration)` returns a signal following
`source` through eased transitions (settled values cost zero frames), and
`after(delay, f)` runs a one-shot closure on the UI thread, costing zero
wakeups until due.

## ui — elements, views, composition

`Element` is the view-tree builder: layout style, children, focusability,
event handlers, keyboard shortcuts, and an optional draw closure.
Components are plain functions `fn(Scope, Props) -> View` — no trait, no
registry. They run **once**; reactivity comes from `dyn_view(style, f)`,
which re-runs `f` when the signals it reads change and re-renders only that
region. Props structs carry data fields, `Callback<T>` fields for typed
events out, and `View` fields as slots for children:

```rust
use abstracttui::prelude::*;
use abstracttui::widgets::Button;

struct CardProps {
    title: String,
    on_close: Callback<()>, // typed event out
    children: View,         // slot
}

fn card(cx: Scope, props: CardProps) -> View {
    let close = props.on_close.clone();
    Element::new()
        .style(LayoutStyle::column())
        .child(
            Element::new()
                .style(LayoutStyle::row())
                .child(text(props.title))
                .child(Button::new("x").on_click(move || close.call(())).view(cx))
                .build(),
        )
        .child(props.children) // the slot mounts where the component says
        .build()
}
```

Events route capture → target → bubble with hit testing and focus
management; `KeyChord` shortcuts attach to any element. For app-scale state,
the endorsed pattern is a store struct of signals provided as context —
`cx.provide_context(store)` at the root, `cx.use_context()` anywhere below.
Signals are `Copy` handles, so cloning the store shares state: no prop
drilling, no reducer framework.

## layout — flex and grid

The layout solver is a flexbox subset over integer cells: `Direction`
row/column, `grow`/`shrink`/`basis`, `gap`, padding, margin, min/max,
percent and absolute positioning, plus wrapping (`wrap()`, `cross_gap`).
Rounding is largest-remainder, so children tile their container exactly.
`Display::Grid` adds track grids: columns and rows are `Track::Cells(n)`,
`Track::Percent(f)`, `Track::Auto` (content-sized), or `Track::Fr(w)`
(weighted leftover); children auto-place row-major and can span via
`col_span`/`row_span`. `Overflow` (`Visible`/`Clip`/`Scroll`) is the
clipping and wheel-routing vocabulary.

```rust
use abstracttui::prelude::*;

// Sidebar + growing content in a row.
let sidebar = LayoutStyle::default().width(Dimension::Cells(24));
let content = LayoutStyle::default().grow(1.0);

// A label/field form as a track grid.
let form = LayoutStyle::default().grid(
    vec![Track::Cells(12), Track::Fr(1.0)], // columns
    vec![Track::Auto, Track::Auto],         // rows
);
```

## widgets — the built-in library

Every widget is built from the same public `ui` + `layout` + `theme` surface
user code has — widgets hold no engine privileges. They consume design
tokens only, never raw colors; the canonical build is `.view(cx)` (theme
from context), with an `element` form for explicit tokens — stateless
widgets take just `&TokenSet`, no `Scope`. The catalog:

- **Block** — the bordered panel primitive: title, fill, focus ring, `BorderKind`.
- **Button** — clickable label; hover/pressed/focused/disabled visuals; Enter/Space or mouse fires `on_click`.
- **TextInput** — single-line editor: grapheme-cluster-atomic cursoring, selection, word jumps, `on_change`/`on_submit`.
- **List** — virtualized selectable list; variable-height items, sticky selection by key, `scroll_to`.
- **Table** — fixed/percent/flex columns, styled header, virtualized rows, selection, sort-indicator hook (the app sorts).
- **Tabs** — tab bar over lazily mounted panels; only the active panel is mounted.
- **Scroll** — clipped viewport over oversized content, mounted once so state, focus, and hit testing survive scrolling.
- **Checkbox** — `[x] label` bound to a `Signal<bool>`.
- **RadioGroup** — one-of-N bound to a `Signal<usize>`; one tab stop, Up/Down move the selection.
- **Progress** — bar with sub-cell precision; optional ok→warn→error ramp.
- **Spinner** — indeterminate activity glyph, pure over a caller-owned frame index.
- **Badge** — small tinted label for status chips, counts, tags (`Tone`).
- **Separator** — horizontal or vertical rule, optionally labeled.
- **Charts** — `Sparkline`, `LineChart`, `BarChart` on sub-cell grids.
- **Grid** — container widget over `Display::Grid`; spans ride each child's own style.
- **Image** — bitmap display through the mosaic pipeline (`ImageFit`; `Bitmap` re-exported beside it).
- **Viewport3D** — orbiting 3D view of a `three::Model`: `.orbit(yaw, pitch, zoom)`, `.animate(clip, t)`, `.on_orbit`/`.on_zoom` deltas; camera state lives app-side in signals.
- **MarkdownView / RichTextView / CodeView** — typeset markdown, wrapped styled spans, read-only highlighted code.
- **Logo** — the AbstractTUI wordmark for headers, about screens, empty states.

## app — the runtime

`App::simple` is the whole happy path: mount a component, enter the
terminal, run until quit. This compiled example is the canonical first app —
Tab focuses, Enter/Space clicks, Ctrl+C quits, all by default:

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

For more control, `App::new(size)` + `mount` + `run` splits the steps, and
`App::quitter()` hands out a cloneable programmatic-quit handle. Ctrl+C
arrives as an ordinary key (raw mode); the quit-by-default policy is
overridden by any handler that consumes the event.

Around the core loop the module provides:

- **Overlays** — z-ordered layers above the main tree (`LayerHandle`,
  `ImageHandle`) for popups, menus, and pixel images.
- **Modal** — a centered, focus-trapped overlay panel: input is fully owned
  while open, Tab cycles inside, state created in the modal's scope dies on
  close. **Toast** — top-right chips that slide in, park for their duration
  at zero frame cost, then slide out and remove their layer.
- **Hooks** — `use_theme(cx)` (the app-level theme signal), `use_viewport(cx)`
  (terminal size as a signal), `use_startup_notices(cx)` (labeled startup
  degradations as a reactive list).
- **KeymapHelp** — a ready-made `?` help modal listing the shortcuts
  reachable from the current focus plus every registered global action.

## theme — design tokens

Widgets consume `TokenId`s resolved against the active theme's `TokenSet`;
they never hold raw colors. Twenty-six built-in themes ship in the registry:
the abstract family (`abstract-dark` — the default — plus light, aurora,
paper, ember, midnight, dawn), `observer-night`, catppuccin (mocha,
macchiato, frappe, latte), rose-pine (plus moon, dawn), `tokyo-night`,
`nord`, `one-dark`/`one-light`, `dracula`, `monokai`, `gruvbox`,
`solarized-dark`/`-light`, and `everforest-dark`/`-light`.

Switching is one signal write: widgets that read the theme signal re-render
fine-grained, and the app damages the whole tree so even static text
repaints in the new palette:

```rust
use abstracttui::prelude::*;

set_theme_by_id("catppuccin-mocha"); // false for unknown ids, nothing changes
```

`theme::list()` enumerates `(id, label, dark)` for a picker. Applications
can add their own themes at runtime with `theme::register(candidate, mode)`:
every registration runs the full contrast audit, and the mode decides
whether violations refuse the theme or register it with labeled findings.

## render — surfaces and paint (advanced)

Most applications never touch `render` directly — widgets and draw closures
do. The two concepts worth knowing:

**`Surface`** is the cell buffer draw closures write into. Damage is
recorded automatically by every write; the diff re-checks equality, so
over-approximate damage costs microseconds, never wrong pixels.

**`render::Style` is a patch, not an appearance.** `fg`/`bg` at `None` keep
what the target cell already has — text drawn over a filled panel keeps the
panel's background. Attributes are add/remove sets, so bold layers onto
existing content. `Style::absolute()` opts out (remove everything first),
and `merge` is sequential application — the later opinion wins:

```rust
use abstracttui::base::Rgba;
use abstracttui::render::{Attrs, Style};

// The common one-liner: ink + emphasis.
let err = Style::new().fg(Rgba::rgb(255, 80, 80)).bold();
assert_eq!(err.add, Attrs::BOLD);
assert_eq!(err.bg, None); // bg unset: keeps the panel underneath

// Patches compose; the later opinion wins where both have one.
let quoted = err.merge(Style::new().dim().fg(Rgba::rgb(150, 150, 150)));
assert_eq!(quoted.fg, Some(Rgba::rgb(150, 150, 150)));
assert_eq!(quoted.add, Attrs::BOLD | Attrs::DIM);
```

The one non-patch field is the hyperlink id: it always overwrites, because
inheriting a stale link under a fresh label would be a correctness hazard.

For effects, layers accept per-cell shaders (`CellShader`; built-ins in
`anim::shaders`). Shaders are billed by damage: static shaders cost nothing
after installation; animated shaders damage only what their `changed_region`
hint declares. For debugging: `render::snapshot(&surface)` prints a bordered
character grid, `snapshot_styles` adds per-row style annotations, and
`Compositor::set_debug_damage(true)` outlines every repaint region live.

## gfx — images

`gfx::decode_image(bytes)` sniffs the magic bytes (containers lie, bytes do
not) and decodes PNG or baseline JPEG into a `Bitmap` — owned RGBA8 with
get/set, nearest and bilinear resize, cropping, and a box-filter mip chain.
Unknown formats are rejected by name, telling the caller what does decode;
truncated or hostile bytes are named errors, never panics.

Three presentation entry points, smallest first:

```rust
use abstracttui::base::{Rect, Rgba};
use abstracttui::gfx::{render_to_cells, Bitmap};
use abstracttui::term::Capabilities;

let img = Bitmap::new(16, 8, Rgba::rgb(180, 90, 30));
let cells = render_to_cells(&img, Rect::new(2, 1, 8, 4), &Capabilities::default());
assert_eq!(cells.len(), 8 * 4);
```

- `render_to_cells` picks the best mosaic mode for the probed terminal and
  returns ready-to-blit cell patches; `MosaicMode::auto(&caps)` returns both
  the mode and the reason it was chosen (half-block, quadrant, sextant, or
  braille; optional Floyd–Steinberg dithering).
- `widgets::Image` is the widget form — always mosaic, because a draw
  closure owns cells, not escape bytes.
- `gfx::ImageSession` manages the pixel protocols (kitty, iTerm2, sixel):
  slots keyed by the caller, content versions, minimal traffic per channel —
  kitty transmits once and re-places on move; iTerm2 and sixel honestly
  re-emit. Bytes reach the terminal through the presenter, and tmux
  passthrough wrapping applies automatically when capabilities prove it.

## three — 3D models

`three::quick_view(path)` is the five-line hello: load a GLB, get a camera
framed on the model's bounds and a default light, render:

```rust
use abstracttui::three::{self, Framebuffer, SceneRenderer};

let view = three::quick_view("model.glb")?;
let mut fb = Framebuffer::new(160, 96);
SceneRenderer::new().render(&view.scene(), &mut fb);
// fb -> mosaic cells via gfx, or hand the model to widgets::Viewport3D.
```

Underneath: `Model::load(bytes)` / `load_glb(path)` parse and validate the
GLB (unsupported features reject by name; recoverable gaps degrade with
labels into `model.warnings`), `Scene`/`Camera`/`Light` describe the view,
and `SceneRenderer` rasterizes with z-buffer, texturing, and mips.
`model.animations()` lists clips; `sample_pose_full(clip, t, &mut pose)`
produces node worlds and skin joint matrices, pure in `t` and allocation-free
at steady state — loop with `t % clip.duration()`. One culling note: bare
`Scene::new` culls back faces (procedural meshes are consistently wound);
`QuickView::scene()` and `Viewport3D` render double-sided, because
real-world exports are not.

## term and input — the terminal, when you need it

Applications under `App` rarely touch these; embedders and diagnostics do.
`Capabilities::detect_env()` is the free, instant, conservative environment
pass; the active probe refines it concurrently at startup. `caps.summary()`
is the multi-line human report (`summary_line()` the one-liner); scripts
should read fields, not parse prose. `EnterOptions` declares the session
posture — the default is the full-screen stance (alternate screen, hidden
cursor, button-drag mouse, bracketed paste, focus events), with kitty
keyboard flags as an explicit opt-in:

```rust
use abstracttui::term::{Capabilities, EnterOptions, TermRead, Terminal, UnixTerminal};
use std::time::{Duration, Instant};

let caps = Capabilities::detect_env(); // free, instant, conservative
let mut term = UnixTerminal::new()?;   // real device fd acquisition
term.enter(&EnterOptions::default())?; // raw mode + altscreen + modes

match term.read(Some(Instant::now() + Duration::from_secs(5)))? {
    TermRead::Input(bytes) => { /* feed input::Parser */ }
    TermRead::Resize(size) => { /* re-layout */ }
    TermRead::Wake => { /* another thread wants the loop */ }
    TermRead::Idle => { /* deadline expired */ }
}

term.leave()?; // also runs on Drop — the terminal always restores
```

`input::Parser` turns raw bytes into structured events — resumable across
arbitrary chunk splits (mid-UTF-8, mid-escape), never panicking on any
input. `input::EventReader` glues a terminal to the parser and owns the
ESC-disambiguation deadlines.

## testing — the headless harness

The `testing` module ships in the library so applications can test against
the same machinery the engine tests itself with: `CaptureTerm` is an
in-memory terminal that records emitted bytes and models the screen,
`VtScreen` is the VT100/xterm interpreter that serves as ground truth
("the bytes we emitted produce the frame we intended"), and `app::Driver`
pumps real frames — the same pipeline production uses — without a tty:

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

Input is fed as the terminal would send it, so every dispatch, focus, and
damage path is the real one. For pure component tests, skip the driver: mount
into a `ui::UiTree`, dispatch events, draw into a `ui::BufferCanvas`.
Golden-snapshot assertions and deterministic fuzz helpers round out the
module.

## Stability and limits

Plain statements of current behavior:

- **JPEG** decoding is baseline sequential only; progressive and arithmetic
  variants reject by name. **PNG** supports 8-bit depths without interlacing
  (Adam7 rejects by name).
- **Sixel** uses one palette per emission: multiple live sixel images
  recolor each other — prefer one per screen. iTerm2 and sixel have no
  placement model (moves re-emit the payload); only kitty gets placement
  escapes and true deletes.
- **Pixel protocols** are verified byte-for-byte against protocol models,
  not live terminals; unicode mosaic is the universal, always-safe path.
- **3D animation** supports LINEAR and STEP interpolation; CUBICSPLINE and
  morph weights skip with labels; rotations nlerp (shortest path), not
  slerp. Skinning reads `JOINTS_0`/`WEIGHTS_0` (four joints per vertex,
  linear blend). Textures: base color only, REPEAT wrap, per-triangle mips.
- **Mosaic** color resolution is two colors per cell (the glyph split
  carries the rest); braille conveys structure, not color; sextant glyphs
  need a recent font and are an explicit opt-in.
- **Ambiguous-width characters** follow `unicode-width` narrow semantics. A
  terminal configured ambiguous-wide breaks cell layout for every terminal
  application; the presenter's cursor discipline bounds the drift but
  cannot erase it.
- **Capacity ceilings** degrade with labels, never unbounded growth: 4096
  distinct long grapheme clusters per surface (then U+FFFD), 65535
  hyperlinks per surface (then plain text), with counters exposed.
- **Scroll optimization** requires DECSTBM/SU/SD compliance — present in
  every VT100 descendant — and can be forced off via `PresenterOpts`.
- **Windows** compiles clean and its extracted logic is unit-tested on every
  host, but it has not yet run on a live Windows machine; treat a first
  Windows deployment as a beta event. macOS and Linux are the live-verified
  platforms.

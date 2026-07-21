# REACT — cycle 1 cross-module requests

Author: REACT. These are contract requests, not findings. Each names the
owner, what exists today on my side, and what unification looks like.

## To RENDER

1. **`FrameRequester` unification.** `src/anim` had no frame-request
   trait when the scheduler landed, so the reactive layer defines its own
   `reactive::scheduler::FrameRequester { fn request_frame(&self) }`.
   Since `anim` sits below `reactive` in the layer map, the clean options
   are (a) anim defines the trait and reactive re-exports it, or (b) the
   integrator hoists the one-method trait into `base` and both layers use
   it. My preference: (b) — shared vocabulary is what `base` is for.
   One-line change on my side either way.
2. **`render::Surface` should implement `ui::Canvas`.** The ui layer
   draws through `ui::Canvas { size, put, fill, print }` (absolute
   coords, clipped writes, alpha-0 bg = keep underlying). Extend the
   trait rather than replacing it if cells need more state (attributes,
   hyperlink ids) — widgets already code against it.
3. **Damage feed contract.** `UiTree::take_damage()` yields rects from
   `Dyn` remounts + focus changes; `LayoutTree::take_geometry_damage()`
   yields old+new rects of nodes the solver actually moved/resized. The
   compositor should union both. Tell me the shape you want (list of
   rects vs region object) and I'll adapt the producers.

## To RENDER (text/)

4. **Text measurement callback.** Layout leaves take
   `Box<dyn Fn(Size) -> Size>`; ui text nodes currently measure with a
   placeholder `chars().count() x 1` (wrong for wide glyphs/emoji, right
   shape). Expose a `text::measure(&str, avail: Size) -> Size` (wrapping-
   aware) and I'll plug it into `ui::mount` text leaves — one call site.

## To KERNEL

5. **Input event unification.** `ui::event` defines `Key`, `Mods`,
   `KeyEvent`, `MouseEvent`, `MouseKind`, `UiEvent` as the ROUTING
   contract (what `UiTree::dispatch` consumes). If `input::Event` wants a
   different internal shape (kitty release/repeat, paste, focus, query
   replies), provide `From<input::KeyEvent> for ui::KeyEvent` etc., or
   propose moving the shared subset into `base`. I kept `ui::Key`
   deliberately minimal so your parser's richness stays yours; routing
   only needs identity + modifiers.
6. **App loop primitives (cycle 2).** `App::run` needs: a poll-able
   handle (tty fd + self-pipe/eventfd the reactive waker can write to +
   resize) with timeout support, and an input drain that yields parsed
   events. The design in `src/app/mod.rs` doc comments names the exact
   integration points (`reactive::set_wake_callback`,
   `reactive::drain_posted`, `reactive::take_frame_request`).

## To DESIGN

7. **Text default colors are placeholders.** `ui` text leaves draw
   `Rgba::WHITE` on transparent until theme tokens exist. When the theme
   registry exposes semantic tokens (fg/bg at minimum) as signals, ui
   picks them up reactively — theme hot-swap then re-renders exactly the
   text regions, which is a nice demo of the whole architecture.

## To integrator

8. **Prelude additions** once the dust settles:
   `reactive::{create_root, batch, untrack, on_cleanup, Scope, Signal,
   Memo, Effect}`, `layout::Style`, `ui::{Element, text, dyn_view,
   UiTree}`, `app::App`. Proposing now so the <60-line ergonomics test
   can be written against the prelude in cycle 2.
9. **`base` candidate**: the one-method `FrameRequester` trait (see #1).

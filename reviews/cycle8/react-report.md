# REACT cycle-8 report — API ergonomics + freeze

At close: lib **901 passed / 0 failed**, doctests **27 passed / 0
failed** (24 ignored = deliberate `no_run`/`ignore` fences), REDTEAM's
suites compile and pass UNCHANGED against the new sugar, `cargo doc
--no-deps` zero warnings in my modules, clippy zero in owned files.

## 1. The first app — 16 LINES (target was ≤40)

The canonical snippet, LIVE as a `no_run` doctest on `App::simple`:

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

The sugar that got it there (all ADDITIVE — zero existing paths broke):

- **`App::simple(component) -> Result<()>`** — construct, mount, run in
  one call (the viewport argument was ceremony: the driver replaces it
  at enter anyway).
- **`Widget::view(cx) -> View`** on every interactive widget (Button/
  TextInput/List/Table/Tabs/Checkbox/RadioGroup/Scroll/Grid) — the
  CANONICAL build: resolves tokens from the app's THEME CONTEXT and
  returns a finished `View`. This killed the worst ceremony
  (`element(cx, &current_theme().tokens).build()` per widget). The
  theme rides `provide_context` from `App::mount` — a TRACKED read, so
  widgets built inside `dyn_view` re-render on theme switch for free,
  and the widgets layer still never imports `app` (layer map holds; a
  bare `UiTree` without an app falls back to the default theme).
- **`LayoutStyle::fill()` / `LayoutStyle::line(rows)`** — the two
  shapes every app writes (`Percent(1.0)`x2 and full-width fixed-rows).

## 2. Naming decisions EXECUTED

- **layout::Style vs render::Style**: `pub type LayoutStyle = Style`
  in layout; the prelude has exported it under that name since cycle 1
  — the alias makes the canonical spelling importable from the module
  too, and docs now consistently say `LayoutStyle` = box geometry,
  `render::Style` = paint. (Renaming `render::Style` itself is
  RENDER's file — flagged, not touched.)
- **`element(cx, &tokens)` vs the canonical**: `view(cx)` is canonical
  (docs say so at every definition); `element(cx, &tokens)` REMAINS as
  the explicit-theming/customization door (you need an `Element` to
  add handlers or override styles). Documented, not deprecated —
  both have real jobs.
- **Constructor consistency**: labeled widgets take
  `impl Into<String>` (Button/Checkbox/Column — already true);
  list-shaped widgets keep `new(Vec<String>)` (an attempted generic
  `new` BROKE REDTEAM's `.collect()` call sites — inference needs the
  concrete parameter; reverted same hour) and gained
  **`of(impl IntoIterator<Item = impl Into<String>>)`** as the
  ergonomic constructor (`List::of(["a", "b"])`). One convention: no
  `with_*` prefixes anywhere in my builders (verified by grep).

## 3. Doctests — 8 entry points, compiled

`App::simple` (the first app, `no_run`), `Signal` (get/set/update/memo
laws — runs), `dyn_view` (full headless mount + fine-grained re-render
asserted — runs), `Callback` (typed props + shared clones — runs),
`Button` (real tree + Tab/Enter activation — runs), `List` (sticky
key selection — runs), `reactive` module quickstart (runs; pre-
existing), compose/store/router patterns (module docs, `ignore` by
design — they sketch app shape, the TESTED versions live in the same
module's test block). `cargo doc --no-deps`: zero warnings in my
modules.

## 4. `use_startup_notices` (DESIGN's ask)

`app::use_startup_notices(cx) -> Signal<Vec<String>>` — the notice
store is now a thread-global SIGNAL (same immortal-root pattern as
theme/viewport); `App::push_startup_notice` fans into it, so a notice
bar mounted BEFORE the engine's post-mount pushes re-renders when they
land. Test pins the exact failure case DESIGN reported (mount-time
reader sees 0, late push re-runs it to 1); the plain
`App::startup_notices()` read keeps working.

## 5. Panic-message audit

Every reactive panic now NAMES THE FIX in the message: disposed-handle
(keep the owning scope alive / dyn_view vs dyn_view_scoped /
try_get_untracked), wrong-thread (post to the UI thread via
spawn_worker, never write from workers), dependency cycle (break with
get_untracked or split state), draw-read (move into dyn_view / capture
before the closure / get_untracked for stale peeks), effect-runaway
per-effect ceiling (read-with-untracked or split; label with
effect_labeled to trace), flush-didn't-settle (find the writer via
labels), created-under-disposed-scope (which scope died and what to
own state on), on_cleanup-outside-scope (where to call it from).

## 6. Pruned + docs-cycle handoff

- `layout::local_point` removed from the public surface (zero external
  consumers; now `pub(crate)`). Everything else I audited
  (`current_viewport`, `MODAL_Z/TOAST_Z`, timer/frame-task fns,
  `ActionInfo`) has real consumers.
- Missing-docs baseline for cycle 10 (counted with
  `RUSTFLAGS="-W missing-docs"`): **368 undocumented pub items in my
  modules**, concentrated in `ui/event.rs` (61 — mostly Key/MouseKind
  variants) and `layout/style.rs` (58 — enum variants + fields). The
  8 ENTRY POINTS are documented with laws + compiled examples this
  cycle; the mechanical variant sweep is the docs cycle's inventory,
  handed over with the per-file counts.

## 7. Risks / notes for cycle 9-10

- `Widget::view(cx)` reads the theme context TRACKED: a widget built
  OUTSIDE any dyn region ties the theme subscription to the mount
  scope — harmless (the app-level theme watcher damages everything on
  switch anyway), but REDTEAM may probe whether a theme switch
  triggers redundant Dyn rebuilds (it re-renders regions that would
  repaint anyway; correctness unaffected).
- `List::new`-vs-`of` is a deliberate two-door: `new(Vec<String>)` is
  the inference-stable door existing code depends on; `of` is the
  ergonomic one. Docs cycle should present `of` first.
- The 16-line app leans on `App::simple`; headless/test apps keep the
  explicit `App::new + mount + Driver` path — both documented.

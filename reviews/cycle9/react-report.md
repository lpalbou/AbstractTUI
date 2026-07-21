# REACT cycle-9 report — RT8 friction closures

At close: lib **910 passed / 0 failed**, doctests **32 passed / 0
failed**, `cargo test --no-run` clean, `cargo doc --no-deps` clean in
my modules, clippy zero in owned files. REDTEAM's `api_first_use`
suite: **4/4 unchanged** — every fix verified against the very tests
that filed the friction.

## RT8-7 (the bug-shaped one) — CLOSED with merge semantics

Root cause: `Block::element` baked its border inset into the returned
Element's plain style, so a later user `.style(grow)` REPLACED the
whole style and dropped content onto the frame.

Fix is general, not Block-local: **`Element::padding_floor(Edges)`** —
a PROTECTED minimum padding applied per-side (`max`) over whatever
style is in effect, at mount AND after every `style_signal` update.
Block now carries its chrome (border `all(1)`, shadow `+1` right/
bottom) as the floor; the newcomer's exact line
(`.style(LayoutStyle::default().grow(1.0))` on the returned Element)
now sizes the panel AND keeps the inset. Semantics: user padding beyond
the floor wins; below it the floor holds (the old code IGNORED user
padding entirely when bordered — strictly more expressive now).
Regression test `rt8_7_user_style_on_block_element_keeps_the_border_
inset` runs the newcomer's line verbatim and asserts title intact +
content inside the frame. REDTEAM's `.layout()` call sites keep
working (both doors are correct now); their workaround comments can
relax at their leisure — their file untouched, per the file-ownership
rule.

## RT8-1 — prelude EXECUTED (my cycle-8 proposal, approved)

`prelude.rs` now exports: the interactive set (Button, TextInput,
List, Checkbox, RadioGroup, Table, Tabs, Scroll, Grid, Image,
Viewport3D + Bitmap) beside DESIGN's display set; `Modal`/`Toast`/
`KeymapHelp`; the hooks (`use_theme`, `use_viewport`,
`use_startup_notices`); `Key`/`KeyChord`/`Mods`; the layout vocabulary
(`Dimension/Align/Justify/Display/Track/Overflow/Edges/Inset` +
`LayoutStyle`). REMOVED per the approved proposal: `render::Style`
(the two-Style trap — `Surface` stays), `create_root`/`on_cleanup`/
`Effect`/`FrameRequester`/`PixelSize`/`Canvas`/`UiTree`/`styled_text`
(engine/test surface). The first-use tests compile from
`prelude::*` alone.

## RT8-2 — headless harness documented on App, COMPILING

`App`'s type docs now carry "Testing your app headlessly": the
mount → `Driver::new(CaptureTerm)` → `push_input` → `turn` → assert
loop as a RUNNING doctest (a shortcut counter driven by a real `+`
keypress, screen asserted before/after), plus the pointer to the
tree-level pattern (UiTree + dispatch + BufferCanvas) every widget
suite uses. No new helper type — the existing pieces ARE the harness;
they were undiscoverable, now they're on the front door.

## RT8-3/4 — signature uniformity + Bitmap

`view(cx)` (the cycle-8 canon) now exists on **Block, Image and
Viewport3D** too — `.view(cx)` works uniformly across every widget in
the prelude; `element(&tokens)` remains each display widget's
explicit-theming door. `Bitmap` is re-exported from `widgets` (doc
note beside `Image`) and the prelude; `Image::from_bitmap` docs show
the `Arc` wrapping in a compiled-fence example.

## RT8-6 — decided: document, NO warning, NO default change

Grow-when-unsized would stretch every text leaf in existing layouts
(breaking change rejected); a debug warning would cry wolf on
legitimate zero-size children (spacers, collapsed panels). Executed
instead: `Style::grow` now carries THE multi-pane rule as its leading
doc (with a compiled example: equal split, fixed sidebar + growing
main), naming the collapse trap explicitly; `LayoutStyle::fill()/
line()` cover the common shapes. The docs cycle should lead every
multi-pane example with `grow` — flagged in the handoff.

## RT8-5 — form-validation pattern documented

`ui::compose` module docs gained "Form validation (the pattern)":
one memo per rule (`Option<String>`, None = valid), a `form_valid`
memo gating submission, error lines as `dyn_view` regions,
disable-on-invalid via `dyn_view_scoped`. No engine surface added —
the memo pattern IS the abstraction, stated as such.

## Item 7 — RadioGroup constructor SETTLED

Final shape (already landed cycle 8, restated as the ruling):
`new(Vec<String>)` (inference-stable — the generic experiment broke
`.collect()` call sites and was reverted same-hour) + `of(impl
IntoIterator<Item = impl Into<String>>)` as the ergonomic door —
IDENTICAL to List. Docs present `of` first.

## Suite state

lib 910/0 (16 ignored = foreign perf pins + deliberate), doctests 32/0
(23 ignored fences are design-sketch `ignore`s; RT8-8's convert-to-
no_run ask is on all widget owners for the docs cycle — my Image
example converted as the down payment), `--no-run` clean, clippy zero,
`api_first_use` 4/4.

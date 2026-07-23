# Proposed: shortcuts on elements outside the focus path silently never fire

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build)
- Severity: P2 — cost ~45min (two debugging rounds: first blamed input
  consumption, then discovered path-only dispatch); workaround holds
- Class: footgun (docs gap + a missing debug aid)

## Context
Browse-mode screen jumping in `abstractgateway-console`: keys `1`-`6`
switch tabs. First attempt registered each digit shortcut on its own
zero-height child element of the root (a tidy "shortcut manifest"
shape — one element per binding, built in a loop):

```rust
Element::new()                       // root
    .children((0..6).map(|i| Element::new()
        .style(LayoutStyle::default().h(0))
        .shortcut(KeyChord::plain(Key::Char(digit(i))), move |_| jump(i))
        .build()))
```

Every digit was dead. No panic, no log — the keys just did nothing,
which first read as "the focused table consumes digits" (it does not).

## Current code reality
- `src/ui/tree.rs:684-710` (0.2.8): shortcut dispatch walks `path` —
  the FOCUS PATH from root to the focused element — and only elements
  on that path are consulted. A shortcut on a sibling of the focused
  subtree is unreachable by construction.
- `src/ui/tree.rs:292`: "No focus = the root's shortcuts" — root
  shortcuts are the only always-on-path registrations.
- `src/app/actions.rs` + `src/app/driver.rs:742-751`: the sanctioned
  global mechanism exists (`App::actions().register(name, chord, run)`,
  dispatched LAST for keys nothing consumed) — but neither the API
  guide's shortcut section nor `Element::shortcut` rustdoc says
  "shortcuts are focus-path-scoped; for app-global keys use Actions".
  The consumer found Actions only by reading the engine's own harness.

## Repro
Mount the snippet above, focus any widget elsewhere in the tree, press
a digit: the handler never runs. Move the same `.shortcut(...)` calls
onto the root element: works.

## Workaround in the field (delete when fixed)
Register app-global keys on the ROOT element (the console holds a
`let mut root_el = Element::new()...` and loops `.shortcut(...)` onto
it) or through `App::actions()`. The fix wish, either of: (a) one
sentence in `Element::shortcut` rustdoc + the api.md shortcut section
naming the focus-path scope and pointing at `Actions` for global keys;
(b) a debug-build startup walk that logs shortcuts registered on
elements that can never be on a focus path (no focusable descendants).

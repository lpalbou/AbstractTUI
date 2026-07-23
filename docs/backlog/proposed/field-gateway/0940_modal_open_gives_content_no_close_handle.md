# Proposed: Modal::open builds content before the Modal exists — self-closing forms need an external-slot dance

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build)
- Severity: P3 — one-time ~30min of API archaeology; the slot pattern
  holds everywhere
- Class: API gap

## Context
Every form modal in the gateway console (profile editor, route editor,
user form, token display) wants the obvious thing: a Save/Cancel button
INSIDE the modal that closes it. `Modal::open(overlays, cx, viewport,
size, build)` runs `build` to produce the content BEFORE the `Modal`
handle exists, so the content cannot capture its own closer. The first
attempt produced a genuinely awkward two-phase shim; the shape that
survived is an app-level `open_form` helper threading a closure over an
external `Rc<RefCell<Option<Modal>>>` slot that is filled after open:

```rust
let closer: Rc<dyn Fn()> = Rc::new(move || {
    if let Some(m) = slot.borrow_mut().take() { m.close(); }
});
let modal = Modal::open(overlays, cx, viewport, size, move |mcx| {
    build(mcx, closer.clone())   // content gets the lazy closer
});
*slot.borrow_mut() = Some(modal); // NOW the closer works
```

`ChoicePrompt` solved this internally (its callback runs with the modal
already closed) — plain `Modal` consumers each rediscover the dance.

## Current code reality
- `src/app/popups.rs:63-98` (0.2.8): `build: impl FnOnce(Scope) -> View`
  runs inside `Modal::open`; the returned `Modal { layer, scope }` is
  constructed after. `Modal::share()` exists for handing out extra
  handles — but only once you have the first one, which is exactly
  what the content builder never has.

## Repro
Try to build `Modal::open(..., |mcx| Button::new("Close").on_click(|| /* ??? */).view(mcx).build())`
— there is nothing to call. Dropping the handle deliberately does not
close (documented), so you cannot even lean on scope death.

## Workaround in the field (delete when fixed)
The `open_form` slot helper above (src/ui/mod.rs in the console). The
fix wish: `build` receives a lightweight close handle —
`impl FnOnce(Scope, ModalHandle) -> View` where `ModalHandle::close()`
is `Modal::close` semantics (idempotent, safe from inside callbacks per
the disposal-safety law). `ChoicePrompt` proves the callback-side
machinery already exists.

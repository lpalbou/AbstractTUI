# Proposed: widget `disabled` is build-time only — validation gating forces focus-dropping rebuilds (0510 evidence)

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build)
- Severity: P2 — shaped the whole form architecture; workaround holds
  but costs focus in a documented corner
- Class: API gap (core 0510 form-kit evidence)

## Context
Every form in the gateway console gates its Save button on validity
("Save override" enables only when provider AND model are chosen — the
fabricated-selection law). `Button::disabled(bool)` (and
`Select::disabled`, `TextInput` has none) is a plain builder prop,
resolved once at view build. The ONLY way to change it is to rebuild
the region inside a `dyn_view` that reads the gating signals — and a
rebuild unmounts the old widgets, so if focus was inside that region it
drops to nowhere. Concretely: Tab to "Save override" while it is
disabled, then pick the missing model with the mouse/another key path —
the buttons row rebuilds under the focused button and focus is lost.

The same mechanic forced a deliberate granularity design in the route
editor (its comment block documents it): provider/base-url/options
fields live OUTSIDE the model row's dyn region purely so a models-list
arrival doesn't remount the field the user is typing in. That is form
architecture dictated by a missing reactive prop.

## Current code reality
- `src/widgets/button.rs:79-89` (0.2.8): `disabled: bool` on the
  builder; consumed at build; no `Signal<bool>` form.
- The engine's own reactivity docs teach "components run once;
  reactivity lives in dyn_view" — which is exactly why a boolean that
  changes at interaction cadence (per keystroke) doesn't fit the
  rebuild lane: rebuild cost is fine, focus identity loss is not.

## Repro
```rust
let valid = cx.signal(false);
dyn_view_scoped(LayoutStyle::line(1), move |gcx| {
    Button::new("Save").disabled(!valid.get()).element(gcx, &t).build()
});
// Tab focus onto the (disabled) button, then valid.set(true) from any
// other interaction → region rebuilds → focus is gone.
```

## Workaround in the field (delete when fixed)
Buttons live in their own small dyn region (fields never rebuild with
them), and the region is placed LAST in tab order so focus is rarely
inside it when gating flips. The ask, either shape: (a)
`disabled(Signal<bool>)` overloads on Button/Select/Combobox (widgets
already bind value signals — the pattern exists), or (b) the 0510 form
kit owning field rows + a submit row with reactive enablement so apps
stop hand-rolling the granularity dance.

# Proposed: Element::draw closures paint past their own rect — hand-rolled text rows bleed over borders

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build; confirmed by
  the build's cycle-1 adversarial review)
- Severity: P2 — silent visual corruption class; app-side clipping holds
- Class: footgun

## Context
The console hand-rolls multi-ink text rows (`line()` in its
`src/ui/util.rs`) with `Element::draw` closures — the natural shape for
chrome-grade styled lines, since `Feed` rich lines are transcript-shaped
and there is no standalone rich-line widget. Gateway error messages can
be 400 chars (the app deliberately surfaces `detail` verbatim); a draw
closure printing one at `rect.x` paints straight across the enclosing
Block's right border and onwards — the engine clips to the damage
region, not to the element's own box, so whether the overflow is
visible depends on what happens to be damaged that frame.

## Current code reality
- `src/ui/draw.rs:51-56` (0.2.8): draw closures receive the element
  rect but the canvas clips only to the damage rect; `clip_overflow`
  exists as an opt-in but is not on by default and not mentioned in the
  API guide's draw-closure section.
- Widgets clip internally (Table cells, TextInput) — hand-rolled draw
  content is the one lane without a guardrail.

## Repro
```rust
Element::new()
    .style(LayoutStyle::line(1))
    .draw(|canvas, rect| {
        canvas.print(Point::new(rect.x, rect.y), &"x".repeat(500),
                     ink, Rgba::TRANSPARENT);
    })
// Mount inside a bordered Block: the row paints across the border and
// into whatever sits right of it, whenever damage extends that far.
```

## Workaround in the field (delete when fixed)
The console's `line()`/`field()` truncate every span to the remaining
rect width with a `…` marker (`fit_width`, cell-width-aware) before
printing. Fix wish, either: (a) clip draw output to the element rect by
default (opt out for deliberate overdraw), or (b) make `clip_overflow`
prominent in the draw-closure docs with the "long text WILL cross your
border" warning. (a) matches the least-surprise rule: every other
widget already behaves that way.

# ADR-0002: The two `Style` types stay distinct; `LayoutStyle` is the documented spelling

## Status

Accepted (2026-07-21).

## Context

The crate has two types named `Style`:

- `layout::Style` — the per-node layout style (flex/grid geometry:
  sizes, grow/shrink/basis, padding, overflow). A large builder-pattern
  struct with public fields; the type every app touches on every
  element.
- `render::Style` — the paint-time styling PATCH applied to cells
  (fg/bg/underline color, attribute add/remove sets, hyperlink id).
  Used inside draw closures and widget painters.

Importing both modules collides; every widget file pays an alias line
(`use crate::layout::{Style as LayoutStyle}` next to
`use crate::render::Style`, or the reverse). The prelude already
resolves the common path deliberately: it exports the alias
`LayoutStyle` and deliberately omits `render::Style` (two `Style` types
one glob apart was the top newcomer trap; prelude doc, RT8-1). The open
question 0170 demanded a ruling on: rename one type at 0.2, or bless
the alias convention permanently.

## Decision

**Keep both types, named `Style` in their own modules, distinct
forever; `LayoutStyle` is THE documented spelling for the layout type
in application code.** No rename ships at 0.2.

Rules this ruling pins:

1. App-facing docs, examples, doctests and the prelude spell the layout
   type `LayoutStyle`. The paint type is spelled by full path
   (`render::Style`) or a local `use abstracttui::render::Style` inside
   modules that paint — draw closures are where it appears, and they
   are already render-flavored code.
2. The prelude never exports `render::Style` (nor an alias of it).
   Painting is not the prelude's common path; the collision trap stays
   closed.
3. The two types never merge and never converge structurally: one is
   solved-geometry INPUT, the other is a composable cell PATCH. Any
   future "unified style" proposal supersedes this ADR explicitly.

Why not rename at 0.2 (considered, rejected): renaming `render::Style`
(to `Paint`/`CellStyle`) or `layout::Style` (to `LayoutStyle` as the
struct's real name) would touch every widget, every example and every
downstream draw closure — a maximal-churn break purchasing only what
the alias already provides, since module-qualified names
(`layout::Style`, `render::Style`) are unambiguous and the prelude
already teaches the safe spelling. The 0.2 breaking budget (ADR-0001)
is better spent on surfaces whose SHAPE is wrong, not whose name is
inconvenient. If real-world confusion persists once external users
exist, the cheap additive step is a `render::CellStyle = Style` alias —
that decision belongs to whoever holds that evidence, in a superseding
ADR.

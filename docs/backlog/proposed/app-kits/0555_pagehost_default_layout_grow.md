# 0555 — PageHost default layout hugs content; every real shell wants grow-into-region (the Viewport3D precedent)

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review,
  `reviews/wave12/visual-to-code-handoff.md` §6 first bullet; recorded
  by the code seat — `reviews/wave12/code-to-visual-handoff.md` "#6:
  PageHost default-layout change is a behavior change and needs its
  own wave slot + changelog line")
- Track: app-kits (PageHost is completed app-kits 0545)
- Class: footgun (a default that every observed consumer overrides the
  same way)
- Severity: P3 — one explicit `.layout(column().grow(1.0))` works
  around it; the cost is every new shell rediscovering it
- Engine: verified on 0.2.22 (2026-07-25)

## The evidence

The shell example needed `.layout(column().grow(1.0))` explicitly for
the page host to fill its region; without it the host hugs the active
page's content. Every real shell — the widget's charter is "FULL pages
behind an app-level bar" (0545) — wants the host to fill the screen
region it is given. The engine already crossed this exact bridge once:
wave 11 made `Viewport3D`'s default layout `grow(1.0)` because "a 3D
canvas has no intrinsic cell size, and the bare `LayoutStyle::default()`
this shipped with solved to ZERO HEIGHT … every in-repo call site was
already passing `.grow(1.0)` by hand"
(`src/widgets/viewport3d.rs:126-133` — the precedent is quoted in the
shipped code comment).

## Current code reality (verified 2026-07-25, 0.2.22)

- `src/widgets/page_host.rs:478` — the root element style is
  `self.layout.unwrap_or_else(LayoutStyle::column)`: a bare column,
  which hugs.
- `src/widgets/page_host.rs:457-461` — the PAGE region inside is
  already `width(Percent(1.0)).grow(1.0)`; only the HOST's own slot
  hugs. So a full-screen page renders correctly the moment the host
  itself is stretched — the default is the only gap.
- `examples/shell.rs` passes the explicit layout today (the field
  workaround to delete).

## What we want

Default the root layout to `LayoutStyle::column().grow(1.0)`, keeping
`.layout(...)` as the full override it already is. Per the code seat:
this is a BEHAVIOR CHANGE — hug-sized hosts (a PageHost deliberately
floated inside a larger surface, if any exist) would grow after the
change — so it takes:

- its own wave slot + CHANGELOG line (Changed, not Added);
- an acceptance sweep: the in-repo consumers
  (`examples/shell.rs`, the wave_extensions acceptance battery that
  composes PageHost, any capture goldens showing a PageHost) re-run,
  diffs reviewed deliberately;
- `examples/shell.rs` deletes its explicit `.layout(...)` in the same
  change — the consumer-deletes-the-workaround acceptance proof.

## Validation

- New default pin: a PageHost mounted bare in a grown column fills the
  region (no hug), pages paint full-height.
- Explicit-`.layout(...)` callers byte-identical (override path
  untouched).
- Capture re-run of `shell` (and any PageHost-bearing example) shows
  intended pixels only.

## Non-goals

- Changing the PAGE region's layout (already grow) or tab-bar sizing.
- A general "all containers default to grow" ruling — Block/Element
  defaults stay as they are; this item is PageHost's charter-specific
  default only (full pages are its stated purpose; hug has no known
  consumer).

## Related

- Completed app-kits 0545 (PageHost), the Viewport3D wave-11
  default-grow precedent (`src/widgets/viewport3d.rs:120-133`),
  field-agora 0840 (grow-vs-intrinsic layout docs — the documentation
  side of the same confusion class).

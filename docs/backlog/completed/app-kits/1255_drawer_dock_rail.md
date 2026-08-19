# 1255 — DrawerDock: the right-edge drawer rail

Status: completed 2026-08-19
Owner: engine (widgets/drawer_dock)
Effort: M

## The field ask

Operator, 2026-08-19, with screenshots of abstractcontinuum's team page:
"a panel with drawer system — the panel can be fully collapsed if no
drawer is opened; when you click on a drawer, it opens the content of
that drawer. This would be very helpful for some applications."

The reference (team_page.tsx ~682): Assistant / Members / Files /
Leaderboard / Desk behind always-visible vertical tabs on the right
edge; one drawer open at a time; `""` = fully collapsed; badge dots
for "something waits behind this drawer".

## Why not the existing Drawer

`app::drawer` (0585) is the TRANSIENT cousin: an overlay that slides
over the content, scrimmed, one per edge, modal by default, gone when
closed. The reference is PERSISTENT chrome: the rail never leaves, the
panel docks INTO layout (content reflows), and collapsed still shows
the tabs. Different contract, different layer — this one is a pure
tree widget (`widgets::DrawerDock`), no overlay privileges.

## Shipped

- Rail: 3-cell column of vertical tab labels, always visible; active
  tab inked accent-on-raised; optional reactive badge dot per tab.
- Panel: docked column between content and rail, header (title + the
  ✕ close CORNER REGION — the 0.3.1 drawer rule: a one-cell target at
  a panel edge is where real terminals quantize edge clicks, so the
  region spans the trailing 4 cells) and the drawer body.
- State: `open: Signal<Option<String>>`, bindable both ways; the one
  `transition` choke point elides same-value writes and fires
  `on_change` (take-call-restore, 0297-safe) on dock-driven changes
  only. No container chords: the signal is the API.
- Lifecycle: `dyn_view_scoped` generation per open — closed = disposed
  (zero-idle law); durable drawer state lives outside the builders
  (the PageHost recipe, stated in the module docs).

## Evidence

- `src/widgets/drawer_dock_tests.rs` — 7 tests: collapsed = content +
  rail only (builders never run), tab toggle, switch replaces, ✕
  corner, disposal counts across open/close cycles, external writes
  bypass `on_change`, badge reactivity.
- `tests/shot_drawer_dock.rs` — the example scene through the REAL
  driver with SGR wire clicks: five states asserted, each captured as
  an SVG via the engine's own screenshot verb when
  `DRAWER_DOCK_SHOTS` is set (the proof artifacts the operator asked
  for).
- `examples/drawer_dock.rs` — the runnable demo.

## Adversarial review (same-wave)

One P1 and a P2 cluster, all folded before release:

- **P1 — builder writing `open` desynced silently.** The builder runs
  inside the panel's tracked computation; a synchronous `open` write
  self-invalidates it, and the Dyn flush path swallows the reactive
  law's named panic — state said "b" while the screen showed "a".
  Debug builds now assert at the seam; docs forbid the pattern
  (redirect from `on_change`/effects). The engine-side question — mid-
  run self-invalidation should reschedule the Dyn effect instead of
  being swallowed — is noted for the reactive owner.
- P2: title cells inside the ✕ hot zone closed the panel on a title
  click (title now truncates clear of the corner); rail labels shredded
  grapheme clusters (now grapheme-per-row); duplicate drawer ids and
  orphan `drawer_badge` are debug-loud; the shipped example clipped its
  own fifth tab (now three drawers that fit — a height-aware rail is
  future work); Esc demoted in docs to the focus-inside-panel path it
  actually is, with the positive path now pinned by test.
- Verified clean by the review: zero-idle in both states (5 turns, 0
  bytes), generation-scope disposal (intervals die on close), drag
  A→release-over-B capture heal, stale-id recovery without clobbering
  the app's signal, layer map, token lint, doc build.

## Dev note

Pointer capture surfaced during testing: dispatching Down without Up
in a harness captures the pointer and routes every later press to the
first tab — real terminals always send the release. Tests use full
click (move/press/release); no engine change needed.

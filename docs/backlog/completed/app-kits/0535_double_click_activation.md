# 0535 — Double-click: engine click-chain synthesis + `Table::on_activate`

## Metadata
- Created: 2026-07-24
- Status: Completed (same wave)
- Track: app-kits (delivers the activation slice of 0530 §3 plus the
  engine synthesis 0530 explicitly deferred: "click-count synthesis
  would be its own engine item" — this is that item)
- Completed: 2026-07-24
- Trigger: operator request from the gateway-console build (the 0215
  validator): "sometimes one click = select; but if it's a line and we
  can open with enter, double click should work too" — the console's
  provider table selects on click and opens the editor on `e`; the
  desktop convention wanted is single-click = select, double-click =
  activate (the same act Enter fires).

## ADR status
- Governing ADRs: ADR-0001 (API stability — everything here is
  ADDITIVE vs 0.2.15, `cargo semver-checks` clean; the one breaking
  candidate is parked in planned/0002 entry 6), ADR-0003 (struct
  extensibility — `MouseEvent` is Copy + all-pub + exhaustive, so the
  count could NOT ride the event; see the design ruling).

## Context

Terminals deliver only raw SGR press/release — no terminal reports
"that was a double-click", so the engine must synthesize click counts
from time + position. Before this item nothing did: the 0250 ruling
recorded "no double-click synthesis anywhere" (List's click-on-selected
gesture was partly designed AROUND the gap), and 0530 §3 proposed
Table activation as click-on-selected explicitly because "no
double-click event exists". The operator asked for double-click by
name, which voids that fallback reason for Table.

## Current code reality (at filing)

- `ui::MouseEvent` (src/ui/event.rs:162): `Copy`, all-pub fields, NOT
  `#[non_exhaustive]`, plain-constructed across tests and apps — adding
  a `clicks` field is a MAJOR break. The count cannot travel on the
  event today.
- `UiTree::dispatch(&UiEvent)` has no time parameter; the driver's
  clock is injectable (`Driver::set_clock`) but never reached the tree.
- `List::on_activate` exists (Enter/Space/click-on-selected, 0250);
  `Table` had NO activation event, and its `s` key is a recorded
  claimed-without-consumer footgun (field-gateway 0980) — a lesson the
  new keys must not repeat.
- Selection-driven re-renders dispose and remount the very instance a
  click hit (List/Table regenerate their `dyn_view` on click 1), so any
  chain identity based on `ViewId` would reset on exactly the presses
  that should chain.

## The design ruling (A vs B)

Two candidate shapes were on the table:

- **A — tree-level synthesis**: the tree tracks the chain and exposes
  the count to handlers through an additive accessor.
- **B — a public pure `ClickChain` helper** embedded by each widget
  that wants counts.

**Ruled: A implemented over B's core.** B alone fails on time: widget
handlers receive only `(&mut EventCtx, &UiEvent)` — no clock — so every
embedding widget would need its own time source, multiplying the exact
problem once per widget. The tree embeds ONE chain per `UiTree`
(`TreeCore.click_chain`) and stamps the count into `EventCtx`
(`click_count()`, additive — EventCtx fields are all `pub(crate)`), so
every widget AND every hand-rolled row reads the same synthesis for
free. The pure state machine still ships public (`ui::ClickChain`,
`observe(now, &event) -> u8`, configurable `window`/`tolerance`)
because it must be pure/testable anyway and custom input paths outside
tree dispatch can embed their own.

**Time is ambient, never wall-clock-implicit**: `ui::set_event_time` /
`ui::event_time` (thread-local, the `reactive::set_frame_requester`
house pattern). The driver publishes its `set_clock`-injectable clock
every turn — one injected clock drives animations, timers, and
double-click tests. A bare `UiTree` with no published time counts every
press 1, deterministically: an implicit `Instant::now()` fallback would
have made two quick programmatic clicks a double-click on a fast
machine and a single on a loaded CI runner — the flake class the rule
prevents. (Direct-dispatch harnesses opt in explicitly.)

**Chain identity is positional** (button + ≤400 ms inclusive + ≤1 cell
Chebyshev inclusive), never instance identity — see the re-render fact
above. Resets: window/tolerance exceeded, different button, any wheel
(content moved under the cell), any drag (the gesture became something
else), and `UiTree::cancel_pointer_press` (the selection layer claimed
the gesture — the press that armed it must not seed a double-click).
Modifiers do not break the chain (DOM `detail` behavior). Counts
saturate at 255; a triple-click's third press reads 3, and every
chained press ≥ 2 activates (activation verbs are open/commit-shaped,
idempotent by convention).

**No suppression, no double-fire**: both presses of a double-click
deliver normally — click 1 selects (nothing waits for a possible second
click), click 2 ADDITIONALLY carries count 2 and activates. The
activating press lands on the already-selected row, so `on_select`
stays silent on it and `on_activate` is the last call in its arm
(disposal-safety law holds on every path).

**Table vs List divergence (deliberate, documented both places)**:
`Table::on_activate` = Enter (always) + Space (single-select alias, the
PLATFORM F5 toggle-first rule — 0530's multi-select mode will claim
Space as toggle within that mode) + double-click gated on the
already-selected row (the ROW GUARD: a chained press that drifted onto
a neighbor row inside the cell tolerance, or a fast click-walk down
adjacent rows, re-selects and never activates). A SLOW second click
never activates — a table is a browsing surface (re-clicking a row to
focus the pane must never open its editor). `List` keeps its shipped
0250 picker gesture unchanged (click-on-selected activates,
timing-free), which SUBSUMES double-click — pinned rather than rebuilt.
Enter/Space on Table are consumed ONLY when `on_activate` is bound
(the 0980 rule applied on day one).

## What shipped

- `src/ui/click.rs`: `ClickChain` (+ `DEFAULT_CLICK_WINDOW` 400 ms,
  `DEFAULT_CLICK_TOLERANCE` 1) + `set_event_time`/`event_time`; unit
  tests for every chain rule (boundaries inclusive, resets, saturation).
- `src/ui/event.rs`: `EventCtx::click_count()` (field is `pub(crate)`;
  accessor additive).
- `src/ui/tree.rs`: per-tree chain folded at dispatch entry (before
  routing, so the count is readable by the very press's handlers);
  no-time-source posture; `cancel_pointer_press` reset.
- `src/app/driver.rs`: the turn clock published as ambient event time.
- `src/widgets/table.rs`: `on_activate` (Enter/Space/double-click as
  ruled above) + module-doc vocabulary; `src/widgets/list.rs`: docs
  amended (subsumption; the "no double-click synthesis" claim retired),
  zero behavior change.
- Docs: api.md "Double-click" (convention, clock rule, custom-row
  recipe with the same-logical-row guard) + "Table — selection vs
  activation"; List sections updated; disposal-law table row; CHANGELOG
  under `[Unreleased]`; `tests/adv_activation.rs` header note
  (supersession recorded in place).

## Validation

- Unit (`ui::click::tests`): window boundary EXACT (at-window chains,
  one past resets), tolerance boundary (Chebyshev 1 chains, 2 resets;
  tolerance-0 policy), triple = 3, saturation at 255, different-button
  reset, wheel/drag reset while Up/Move keep the chain, mods don't
  break the chain, ambient time roundtrip.
- Widget (`widgets::table::tests`): Enter+Space activate when bound;
  Enter passes through unbound (anti-0980 pin); double-click activates
  exactly once while single and SLOW second clicks never do; adjacent-
  row chained press selects-but-never-activates (row guard); wheel
  between clicks resets (with a no-wheel control); no-time-source
  presses stay isolated; `on_activate` may dispose the Table's scope.
  (`widgets::list::tests`): double-click fires List's `on_activate`
  exactly once (subsumption pin).
- Integration (`tests/adv_double_click.rs`, real `Driver` +
  `CaptureTerm`, raw SGR bytes, `set_clock`-injected time): Table
  double-click fires once with selection moved on click 1 and the
  highlight as real frame content; slow second click never activates;
  Enter twin; zero-idle after settle (idle turns emit zero bytes); List
  subsumption; `unknown_seq_count == 0`.

## Progress checklist
- [x] `ui::ClickChain` + ambient event time (no-wall-clock rule)
- [x] Tree-embedded synthesis + `EventCtx::click_count()`
- [x] Driver publishes its injectable clock as event time
- [x] `Table::on_activate` (Enter/Space when bound + double-click, row
      guard, disposal-safe)
- [x] List pinned unchanged (subsumption)
- [x] Unit + widget + Driver/CaptureTerm acceptance suites
- [x] api.md + CHANGELOG + supersession notes (0530 §3, adv_activation
      header, 0250's "no synthesis" line)
- [x] Gates: workspace tests, clippy, fmt, `cargo semver-checks` vs
      0.2.15 additive-clean

## Completion report

- Final path: `docs/backlog/completed/app-kits/0535_double_click_activation.md`
- Date: 2026-07-24
- Outcome: engine click-chain synthesis (tree-embedded, ambient-clock,
  positional identity) + `Table::on_activate` (Enter/Space/double-click
  with the row guard) + public `ClickChain`/`click_count()` surface for
  custom rows; List unchanged by design. All additive vs 0.2.15
  (semver-checks clean — gate numbers in the wave report).
- Key validation: 9 new unit tests (`ui::click`), 7 new Table widget
  tests + 1 List pin, 2 Driver/CaptureTerm acceptance tests
  (`tests/adv_double_click.rs`); full workspace suite green; the
  pre-existing `adv_activation.rs` suite passes UNCHANGED under live
  wall-clock chaining (no existing behavior moved).
- Follow-ups revealed (named, not built):
  1. **Feed message cards**: `Feed::on_item_press(key, row)` carries no
     `EventCtx`, so card consumers cannot read `click_count()` through
     it — either the callback grows a click-info parameter (additive
     overload) or cards hand-roll `ClickChain` + `event_time()`; decide
     with first-app/0280's card work (0850's recipe is the home).
  2. **GraphView nodes** (extensions/graph): node activation via
     double-click — wire `ctx.click_count()` in its press handler when
     the graph band schedules interaction work (0430 M-milestones).
  3. **`MouseEvent.clicks` field at 0.3**: the count's natural long-term
     home is the event itself; parked as planned/0002 candidate entry 6
     (breaking: exhaustive Copy struct).
  4. **List/Table gesture unification**: whether List's timing-free
     click-on-selected should become the timed convention at 0.3 is a
     product question — the divergence is documented in both widgets;
     revisit if picker-surface users report accidental commits.

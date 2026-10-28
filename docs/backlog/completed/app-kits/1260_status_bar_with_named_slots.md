# 1260 — Status bar with named slots (the bottom twin of 0560's AppHeader)

## Metadata
- Created: 2026-10-28
- Status: Completed (2026-10-28, chrome wave; released in 0.7.0)
- Track: app-kits (continuation band 1250–1290)
- Completed: 2026-10-28
- Depends on: 0560 (AppHeader: `leading`/`center`/`trailing` slots,
  truncation priority, shrink rules) — reuse its slot model; 0187 for
  clickable items drawn inside it.
- Root link: AbstractFramework backlog 1055; consumer: AbstractCode chrome.

## Inventory (what exists)
0560 proposes the top header with three slots; `PageHost`'s tab bar
(src/widgets/page_host_bar.rs) is a single-purpose bar; `anchored.rs`
already knows "occupied chrome (a status bar)" for popup placement. No
composable BOTTOM bar exists: apps custom-draw their status line.

## What we want
`StatusBar` with named slots (`left`, `center`, `right`, plus an ordered
`items` list with priorities) holding ordinary Views (Chip, Spinner,
Badge, Button, text), the same truncation/shrink rules as 0560, one row,
theme tokens for its ground; it registers itself as occupied chrome for
`anchored` placement. 0560 and 1260 share one slot-layout helper.

## Validation
Headless: slots at wide and narrow widths (priority drop order),
clicks reach the slot Views, anchored popups avoid it; red without the
shared helper.

## Completion report (2026-10-28)

Shipped in `src/widgets/status_bar.rs`: `StatusBar` (`left`/`center`/`right`/
`item(Slot, ..)`, `separator`, `slot_gap`, `padding`, `ground`, `plan`,
`element(&tokens)`, `live(build, on_action)`), `BarItem` (styled runs,
`priority`, `fit`, `action`), `BarFit::{Whole, TruncateEnd, TruncateStart}`,
`Slot`, and the pure shared helper `plan_slots` (+ `SlotMeasure`, `Placement`)
that 0560's header is to reuse. Example: `examples/status_bar.rs`; docs:
`docs/api.md` § "widgets::StatusBar".

Deviations, stated:
- Items are declarative styled runs, not arbitrary child Views: one pure plan
  then drives paint AND hit targets (the `plan_bar`/`hit_bar` pattern) with
  exact widths and no layout lag. Clicks and keyboard reach items through 0187
  action ids instead of per-View handlers.
- Registering the bar as "occupied chrome" for `anchored` placement is NOT done;
  the existing `PanelPlacement::AbovePreferred` policy covers the composer-over-
  status-bar case. Follow-up if a consumer needs it.

Evidence (`src/widgets/status_bar_tests.rs`):
`slots_place_left_center_right_at_a_wide_width`,
`overflow_yields_in_priority_order_at_every_width` (80/60/50/44/40/34/30/24/20/12/6
columns: priority order, no whole-item fragments, tail-kept path truncation),
`plan_drops_by_priority_and_truncates_to_min_first`,
`center_slides_away_from_a_long_left_slot`, the click/keyboard tests listed in
0187, and the `plan_slots` doctest.

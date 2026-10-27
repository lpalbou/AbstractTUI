# 1260 — Status bar with named slots (the bottom twin of 0560's AppHeader)

## Metadata
- Created: 2026-10-28
- Status: Proposed
- Track: app-kits (continuation band 1250–1290)
- Completed: N/A
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

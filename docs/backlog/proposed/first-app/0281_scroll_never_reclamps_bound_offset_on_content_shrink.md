# Proposed: Scroll never re-clamps a bound offset when content shrinks under it

## Metadata
- Created: 2026-07-23
- Status: Proposed (API gap report — first-app finding, 0.2.6 adoption wave)
- Completed: N/A

## ADR status
- Governing ADRs: None. ADR impact: none — Scroll offset ownership semantics.

## Context
`abstractcode-tui` binds an external offset signal to the transcript
`Scroll` (`offset_y(offset)` + `follow_tail(follow)`). With follow
disengaged (the user is reading scrollback), a content REBUILD can
shrink the extent far below the held offset: toggling details OFF folds
every finished tool + thinking card; a session switch replaces the fold
wholesale. Nothing repairs the offset — the pane renders NOTHING until
a wheel tick or Esc rescues it (a rescue the user has no reason to know
about). Live finding, test-pinned app-side:
`details_shrink_while_scrolled_up_never_blanks_the_pane`
(tests/headless_ui.rs).

## Current code reality
- Engine: `scroll.rs` clamps offsets only inside its OWN gesture
  handlers — wheel (`:272`/`:275`: `(*o + dy).clamp(0, (content_h -
  view.h).max(0))`) and thumb drag (`:342-344`). A bound offset that
  became out-of-range through a CONTENT change is never touched; the
  follow pin (`:252`) repositions only while follow is armed.
- The engine already measures the content extent every layout (the
  extent signal that drives clamps/thumb/follow, `:219`) — it holds
  both truths (extent + viewport) at the moment the shrink happens.
- App workaround (`abstractcode-tui src/ui/mod.rs:217-231`): an effect
  tracks `FeedState::total_rows()`, recomputes `max_off` from
  `current_viewport().h - CHROME_ROWS` (an ESTIMATE — the composer can
  grow 1..4 rows, so the app's pane-height guess errs low and snaps a
  few rows above the true bottom), and snaps the offset when it ends up
  beyond the extent. It also only knows about FEED shrinks — any other
  scroll host would need its own copy.

## Proposed direction (engine's call)
- Repair the bound offset at layout time when the measured extent
  shrinks below it: `offset = offset.min((content - view).max(0))` —
  the engine's clamp applied on the one axis it currently skips
  (content change instead of gesture). Growth stays untouched
  (`max_off` only grows — live streaming must never fight a reading
  user).
- If unconditional repair worries any consumer (an app deliberately
  holding an out-of-range offset?), an opt-in `clamp_offset(true)` on
  the builder; but repair-on-shrink looks like the correct default —
  an out-of-range offset renders blank, which no consumer wants.

## App-side workaround to delete when this lands
`abstractcode-tui src/ui/mod.rs` — the shrink-clamp effect (~15 lines)
plus its `CHROME_ROWS` estimate coupling; the engine-side repair is
strictly more accurate (it knows the scroll's real viewport, not a
chrome-row guess).

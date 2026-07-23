# Proposed: Scroll never re-clamps a bound offset when content shrinks under it

## Metadata
- Created: 2026-07-23
- Status: Completed (2026-07-23, scroll/feed wave 4)
- Completed: 2026-07-23

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

## Completion report (2026-07-23, scroll/feed wave 4)

- **Premise correction found while shipping**: "the engine holds both
  truths at the moment the shrink happens" was not quite true. The
  extent is a DRAW-time readback (`size_probe` on the content wrapper),
  and the paint walk culls nodes fully outside the clip
  (`ui/draw.rs`) — the void state (`offset ≥ new content height`) puts
  the wrapper entirely above the viewport, which is exactly where the
  probe stopped running. The extent signal froze at the pre-shrink
  value and no repair could ever see the shrink (the module's own
  follow-lane doc had named this starvation class). The fix therefore
  has two halves:
  1. `Element::probe_when_culled()` (CRATE-PRIVATE, zero public
     surface): a node so flagged runs its OWN draw closure even when
     its rect lies fully outside the paint clip; children still cull
     individually and the canvas stays damage-clipped. Culling is a
     paint optimization; a measurement-readback probe is not paint.
     Scroll's measured-mode wrapper carries the flag. Load-bearing is
     PROVEN: `shrink_below_offset_reclamps_and_repaints_without_a_gesture`
     fails when the flag is removed.
  2. The repair effect in `Scroll::element`: tracked reads of the
     extent and the viewport box (the viewport `size_probe` is now
     unconditional — it used to install only with `follow_tail`),
     UNTRACKED offset reads, per-enabled-axis clamp DOWN when
     `offset > (content - view).max(0)`.
- **Clamp semantics decided (the item's open questions)**:
  - Repair is the DEFAULT and unconditional — no `clamp_offset(true)`
    opt-in was added (an out-of-range offset renders blank, which no
    consumer wants; the hypothetical consumer holding one deliberately
    does not exist, and a knob for it would be speculative surface).
  - The repair rides the SIGNAL, not the pixels ("clamp silently at
    draw" was rejected): draw closures must not write signals (RT1-2),
    and a pixel-only clamp would desync scrollbar, gestures and app
    reads from what renders. Cost: the repaired frame lands one settle
    turn after the shrink — the same latency shape as every probe
    readback (and as the app's own workaround).
  - Programmatic writes: offset reads are untracked, so the effect
    fires only on extent/viewport CHANGES — an in-range app write is
    never touched, and growth never moves a reading user (`max_off`
    only grows). An out-of-range write with no content change is the
    writer's to own (gestures already clamp on next touch).
  - Follow-tail: the repair NEVER writes `follow` (a repair is not a
    gesture — it neither disengages nor arms). While following, the
    pin effect computes the same pinned value, so the two effects
    cannot fight.
  - Startup guard: in measured mode, extent `(0,0)` is the unmeasured
    sentinel (a real solve gives the cross axis the viewport's
    extent); the repair stays inert until the first real measurement,
    so a RESTORED offset written before the first frame survives.
    Test-pinned.
- **Tests** (`widgets::scroll::tests`):
  - `shrink_below_offset_reclamps_and_repaints_without_a_gesture` —
    the consumer shape (bound offset + follow, disengaged, session
    switch shrinking 20 rows → 2): offset repaired to the new max,
    content repaints with NO gesture, follow stays disengaged, and
    subsequent growth keeps the repaired offset;
  - `restored_offset_survives_startup_measurement` — offset 12 bound
    before the first frame renders `row 12` and stays 12;
  - `viewport_growth_reclamps_a_hint_mode_offset` — hint mode: a
    taller viewport (4 → 12 rows under a 30-row hint) re-clamps
    26 → 18;
  - all pre-existing follow-tail/measured-extent/hint-wins tests
    unchanged and green (composition preserved).
- **App workaround**: the `abstractcode-tui` shrink-clamp effect (and
  its `CHROME_ROWS` estimate) can be deleted on upgrade.
- Gates at completion: whole-tree `cargo test` green (1236 lib tests +
  all integration suites, 0 failed), clippy `--all-targets` zero,
  fmt clean, alloc pins green (`alloc_budget` 10/10 single-threaded),
  `cargo semver-checks` vs 0.2.6 — 196 checks pass, additive-clean
  (the only new public surface this wave is `FeedItem` builders from
  0283; the draw exemption is crate-private).

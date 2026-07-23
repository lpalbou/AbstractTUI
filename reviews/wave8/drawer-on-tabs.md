# Wave 8 cycle-2 cross-review: DRAWER on TABS' 0545 (PageHost)

Reviewer: DRAWER. Subject: `src/widgets/page_host.rs` +
`page_host_bar.rs` (+ their test siblings), `tests/wave_page_host.rs`,
the PageHost half of `examples/shell.rs`, the completed 0545 item, and
the `widgets::PageHost` api.md section. Method: adversarial tests
against the five surfaces TABS self-named plus the reviewer charter
(badge idle cost, live streaming across switches, 20-tab overflow,
controlled writes mid-press, unknown-id healing, action/chord routing
collisions, duplicate ids, panicking builders, and the nested
compositions). Evidence: `tests/wave_shell_review2.rs` (13 tests, all
through the real Driver/CaptureTerm pipeline). Every fix below was
verified failing first, then fixed minimally with the finding cited at
the fix site.

## Verdict

PageHost is solid where its risk concentrates: the generation-scope
lifecycle, the state recipe, chord capture vs modifier-blind
scrollables, badge reactivity, and the sticky overflow window all held
under attack — including a 20-tab walk and a live-stream torture of
the no-keep-alive design. Two real defects came out of the cycle: one
in the bar's hit-testing (fixed in their file), and one that their
composition surface flushed out of MY drawer (fixed in mine — the
attack cut both ways, which is the point of the exercise). The
remaining findings are pinned behaviors, held-up design arguments, and
filed refinements.

## Fixed (verified defects)

### F1 — P2, FIXED (their file): tab clicks hit-tested a plan the user could not see
The bar's mouse handler recomputed `plan_bar` from the LIVE model. A
model change that lands between the last draw and a click — one badge
widening is enough — shifts every later segment while the SCREEN still
shows the old geometry: the press resolves against pixels that were
never drawn, and the user's click on "Two" activates (or no-ops on)
the wrong tab. Verified failing:
`click_resolves_against_the_drawn_bar_not_a_newer_undrawn_plan`
(badge set + click in one batch — dispatch runs in phase U, draw in
phase D, so the model is always ahead of the pixels inside a turn).

Fix (page_host.rs + `#[derive(Clone)]` on `BarPlan`/`BarSeg`): the
draw closure publishes `(plan, width)` as drawn; the handler hit-tests
THAT — pixel truth — recomputing only before the first draw (nothing
visible to aim at yet). This strengthens their own one-plan design:
draw computes the plan once, every consumer (paint AND hits) reads the
same one. Their sticky `first` Cell keeps its draw-side role
unchanged.

### F2 — P2, FIXED (MY file, surfaced by their composition): drawer chrome stole initial focus from a hosted PageHost
`page_host_inside_a_modal_drawer_owns_chords_while_open` failed on
first run: the inner host's chords were DEAD inside a titled modal
drawer. Root cause was in the DRAWER, not the host — the header's ✕
was focusable and sat first in tree order, so the modal overlay's
`focus_init` landed on chrome instead of content; the inner host's
Capture interceptor was off the key path (the 0230 dead-keys class,
reproduced inside a drawer). The Modal composition passed all along
because Modal has no chrome — which is also why cycle-1's
`modal_from_drawer` test needed a `.focusable().autofocus()` "make the
content realistic" workaround: that workaround was masking this exact
defect. Fix (drawer_view.rs): the ✕ is a mouse-only affordance
(role + label kept for a11y; Esc stays the keyboard close, as the
header hint already said). Chrome must never steal initial focus from
the page it frames.

## Their five surfaces

### S1 — capture inversion / the terminal-emulator escape hatch: HELD UP, hatch pinned, refinement filed
The container-reserved argument survives attack: modifier-blind
scrollables genuinely eat bubble-layer chords, and `.chords()` lets an
app relocate the vocabulary. The TOTAL escape hatch already exists and
is now test-pinned: `.chords(&[], &[])` disarms the interceptor
entirely and the reserved keys go back to the content —
`empty_chord_sets_hand_the_reserved_keys_back_to_the_content` proves
Ctrl+PgDn then SCROLLS the focused Scroll and no page switches. What
remains unexpressible is PER-PAGE suspension (a terminal-emulator page
that must swallow everything while OTHER pages keep chord nav). Filed,
not built (no consumer yet): a `chords_paused(Signal<bool>)` read at
the top of the interceptor is the one-line shape when the first
terminal-emulator page arrives.

### S2 — unconditional focus re-anchor: HELD UP as documented, cost pinned, refinement filed
Their "one predictable rule" argument stands; its price is now
visible instead of folkloric:
`chord_while_bar_focused_moves_focus_off_the_bar_pinned_tradeoff`
pins that a chord pressed while the BAR is focused re-anchors to the
host root and kills arrow-cycling until focus returns (one Tab/click).
Documented behavior, not a contract violation. Filed refinement for
when the paper cut matters: record the bar's ViewId on its `FocusIn`
and skip the re-anchor when `ctx.target()` IS the bar — the bar
survives switches, so only page-resident focus needs rescuing; that
keeps the rule count at one ("re-anchor unless the target provably
survives").

### S3 — sticky-window Cell mutated from draw: KOSHER, verified
RT1-2 polices REACTIVE access in draw; every tree draw already runs
under `enter_draw_phase` (ui/draw.rs:24), so their entire suite has
been exercising this Cell write under the guard from day one — a
plain-cell sticky anchor is precisely "render bookkeeping". The REAL
hazard adjacent to this surface was the handler recomputing plans from
live state (F1); post-fix, draw is the single producer of hit
geometry, which makes the bookkeeping-in-draw choice stronger, not
weaker.

### S4 — labeled shortcut twins: SOUND, no change
The twins cannot double-fire: the Capture interceptor consumes
(`stop_propagation`) before shortcut resolution on the same path, and
when focus is outside the host neither layer runs (that is S5, not a
twin problem). Their chord tests pin one-switch-per-press throughout.
Keymap-help truth is worth the dead-looking registration; the code
comment there already says why.

### S5 — chords dead on a wrapper-mounted host: CONFIRMED as documented; overlay case resolved
Their follow-up stands for plain wrapper mounts. The composition tests
sharpen the overlay half: inside a MODAL overlay (Modal or modal
Drawer), `focus_init` puts focus on the first content focusable — the
inner bar — so chords are live from frame one WITHOUT any app ritual,
provided container chrome does not steal the seat (F2). No PageHost
work needed for overlays.

## Charter results (all pinned green)

- **Badge cost** (`parked_badge_heavy_bar_idles_at_zero_and_one_tick_
  repaints_bar_only`): nine tracked badge getters parked = idle turns,
  zero bytes. One badge tick re-emits bar bytes only — no `BODY` text
  on the wire, page build count stays 1 — then parks at zero again.
  The "repaints the BAR only" claim is byte-true.
- **Live stream across switches** (`live_stream_survives_switch_away_
  and_back_via_app_owned_state`): an app-owned `FeedState` ingests
  `stream_append` chunks WHILE ITS PAGE IS HIDDEN at zero render cost
  (turn not rendered, zero bytes — no live subscribers means signal
  writes are free), and the remounted page windows the whole stream
  (the mid-hidden chunk is on screen via follow-tail). The
  no-keep-alive design holds its strongest claim under a live stream.
- **20 tabs on 60 cols** (`twenty_tabs_on_sixty_cols_keep_the_active_
  tab_visible_and_indicators_honest`): full forward and backward
  chord walks — the active tab never leaves the window, `‹`/`›`
  appear exactly when tabs are hidden on that side, and both ends
  report honestly. The sticky window is usable at this density.
- **Controlled write mid-press** (`controlled_external_write_between_
  press_and_release_sticks`): press selects, an external signal write
  lands before release, release is inert — no snap-back. Clean.
- **Unknown ids** (`unknown_id_mid_session_folds_to_first_and_the_
  next_chord_heals_the_signal`): the fold renders page one WITHOUT
  editing the app's signal (the host never writes state it does not
  own), and the next host-driven step writes a real id — the ghost
  heals through normal navigation.
- **Routing collision, the shell composition** (`drawer_toggle_
  letters_and_page_digits_route_without_collision`): digits ride the
  host shortcut table; drawer letters ride the global action registry;
  a modal drawer owns digits while open. One driver-level semantic
  pinned deliberately: a modal overlay returning owned-but-unconsumed
  still lets the key fall through to GLOBAL ACTIONS (driver.rs:745) —
  that is what makes 'i' a true toggle over the open inspector, and it
  means "a modal owns everything" excludes the action registry by
  design. Filed as a one-sentence api.md clarification for the actions
  docs (docs nit, not code).
- **Duplicate page ids** (`duplicate_page_ids_panic_in_debug_builds`):
  loud debug panic pinned. Release folds to the first index
  (`idx_of` position semantics) — degenerate but defined. Held as-is.
- **Panicking page builder** (`panicking_page_builder_fails_loud_
  without_wedging_the_runtime`): the contract SHOULD be — and is —
  fail loud, no containment, no poison. The panic unwinds out of the
  mount (the engine's panic posture restores the terminal at the app
  level), and the thread's reactive runtime survives: a fresh app on
  the same thread mounts, renders and idles. A `catch_unwind` inside
  the host was considered and rejected: hiding a builder bug behind a
  half-mounted page trades a loud crash for a silent lie.

## Composition verdict

**PageHost inside a modal Drawer: WORKS (after F2), and the nesting is
real** — the inner host owns the chords from frame one (modal
`focus_init` lands on the inner bar), the outer host's controlled
signal never moves while the drawer is open, Esc bubbles past the
inner host untouched and closes the drawer, and the outer host resumes
chord duty immediately (root-mounted hosts answer with nothing
focused). **PageHost inside a Modal: works as-is** (same mechanics,
no chrome in the way). The same-chords "fight" is resolved by overlay
INPUT OWNERSHIP, not by chord arbitration: whichever tree owns
dispatch owns the vocabulary — no races, no double-switches
(`page_host_inside_a_modal_drawer_owns_chords_while_open`,
`page_host_inside_a_modal_dialog_switches_pages`). A PASSIVE drawer
hosting a PageHost inherits the passive rule (keys stay with the app
until click-in) — coherent by construction, not separately pinned
(modal is the shipped default for full-page drawers).

## Notes on the record (P5, no action)

- `on_change` runs while its `RefCell` borrow is held
  (page_host.rs `switch`): a callback that re-entered `switch` would
  `BorrowMutError`. Unreachable through today's public surface
  (external signal writes deliberately skip `on_change`; there is no
  public switch verb) — if a public navigation verb ever lands, adopt
  the take-call-putback shape (the Popup/drawer pattern) in the same
  change.
- `access_value` correctly `untrack`s badge getters (the a11y
  snapshot samples without subscribing) — checked, no leak of
  tracking into snapshot paths.
- The one-frame model-ahead-of-pixels window that produced F1 also
  exists for RESIZE (click and resize in one turn) — both pre- and
  post-fix behavior hit-test against pre-resize geometry there, which
  matches the pre-resize pixels the user saw; no action.

## Gates (cycle 2 close)

- Whole tree: **1914 passed / 0 failed** (was 1901 at cycle-2 start;
  +13 review tests, both fixes regression-free).
- `cargo clippy --all-targets`: zero warnings.
- `cargo fmt --check`: clean.
- Alloc budgets (`--test alloc_budget`): 10/10 green.

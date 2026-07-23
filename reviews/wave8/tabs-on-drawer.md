# Wave 8 cross-review: TABS on DRAWER's 0585 (drawer system)

Reviewer: TABS. Subject: `src/app/drawer*.rs`, `tests/wave_drawers.rs`,
`examples/drawers.rs`, the `reactive::animate` disposal guard, and the
drawer additions in `examples/shell.rs`. Method: adversarial tests
against the six surfaces DRAWER self-named plus the reviewer charter
(Esc routing, drawer × PageHost composition, tiny viewports, Percent
rounding, sticky scrims, toggle storms, interval scope semantics).
Evidence: `tests/wave_shell_review.rs` (16 tests, all through the real
Driver/CaptureTerm pipeline) + one unit pin added to
`src/app/drawer_tests.rs`. Every fix below was verified failing first,
then fixed minimally in DRAWER's files with the finding cited at the
fix site.

## Verdict

The drawer system is solid where it was hardest — the animate
lifecycle, close-reason ledger, zero-idle discipline, scrim semantics,
resize re-clamp, and the scope-ownership model all held under attack.
Three real defects were found and fixed: two in the one-per-edge
claim (P1) and key-ownership (P2) composition seams, one latent token
drift (P3). The remaining findings are pinned behaviors and recorded
positions, not code changes.

## Findings

### F1 — P1, FIXED: one-per-edge breaks under mid-claim re-entrancy
Their surface 2, confirmed both ways. `registry_claim` fires the
incumbent's `on_close` BEFORE the challenger builds/mounts; a callback
that reopens its drawer (the "pinned drawer comes back" reflex), or a
build closure that opens another drawer on the same edge, re-claims
the slot while the outer open is between claim and mount. The outer
open then mounted anyway: TWO panels (and scrims) on ONE fixed z slot
— exactly the equal-z trap the fixed-slot design exists to avoid.
Repro: `reopen_from_on_close_during_replacement_keeps_one_drawer_per_edge`,
`open_on_the_same_edge_from_inside_a_build_stays_single` (both failed
pre-fix with two drawers open).

Fix (drawer_open.rs): after the build closure returns — the only
user-code window in a fresh open — the open re-checks
`registry_holds(edge, me)`; a stolen claim ABORTS the open before any
layer exists (mount scope disposed, `opening` cleared). The LAST claim
owns the slot. Deliberately, an aborted open fires NO close reason:
it never completed, and silence is what makes a mutually-reopening
callback pair terminate instead of recursing.

### F2 — P2, FIXED: a focused passive drawer above a modal drawer keeps the keyboard
Their surface 5, the keys half. Overlay key dispatch walks topmost-z
first and a FOCUSED non-modal tree outranks everything below it (the
cycle-5 rule — correct for popups-over-modals). With fixed per-edge
slots (Left < Right < Top < Bottom), a passive drawer on a higher
slot that the user had clicked into kept EVERY key — including Esc,
which closed the passive strip while the modal drawer stayed open
beneath its scrim. That contradicts the drawer's own contract ("every
input routes to the panel while open").
Repro: `modal_drawer_takes_keys_from_a_focused_passive_drawer_above_it`
(failed pre-fix: Esc closed the wrong drawer).

Fix (drawer_open.rs): opening — or reopening mid-close — a MODAL
drawer blurs the passive drawer trees (`blur_passive_drawers`, via the
edge registry; handles snapshotted before `set_focus` runs user
FocusOut handlers). An explicit click back into an unveiled passive
panel re-steals the keyboard — that is the engine's one focus story
(click where you want your keys), now test-pinned as the deliberate
path.

The POINTER half of their surface 5 is NOT a defect and was argued
back: the z story is self-consistent. A passive panel BELOW a modal's
scrim is veiled and inert (the modal swallows outside presses); a
passive panel ABOVE the scrim renders unveiled, and an unveiled,
undimmed panel being interactive is visually honest.

### F3 — P3 (latent), FIXED: the scrim's resize repaint re-read the current theme
The documented contract is tokens-at-open ("a mid-open theme switch
lands at the next open", drawer_view.rs; completion-report follow-up
2). The resize re-clamp repainted the scrim with
`current_theme().overlay` — a theme switch followed by a resize while
open would mint a mixed-theme drawer (new veil under an old-token
panel). Latent today ONLY because every registry theme shares one
overlay value by construction (`Rgba::BLACK.with_alpha(OVERLAY_ALPHA)`,
registry.rs:286); runtime-registered themes (`theme::register`) may
diverge. Fix: the veil cell is captured at open (`Mount::veil`) and
reused by the re-clamp. Pin:
`scrim_repaint_on_resize_keeps_the_at_open_veil_token` (drawer_tests
— uses a leaked divergent-overlay theme clone, the exact shape a
custom registered theme takes; failed pre-fix).

### F4 — INFO, pinned, no change: same-instant closes land synchronously
Their surface 1 verified SOUND, including my own wrong first
expectation: `open(); close()` in one instant lands the close
synchronously (eased progress never left zero — exactly what their
progress-before-closing ordering comment protects), so a following
`open()` is a FRESH mount and the ledger honestly records the landed
close (`[Api, Api]`, not a reversal). Reopen-reversal applies only to
closes caught mid-flight — both behaviors pinned
(`same_instant_open_close_and_reopen_touch_no_dead_signal`,
`toggle_storms_drain_to_zero_and_keep_one_truth`). No dead-signal
write is reachable through any same-instant sequence I could
construct.

### F5 — INFO, pinned, recorded: handle verbs inside the build closure
Their surface 6. `open()` from inside the drawer's own build is
latched (`opening`) — clean. `close()` (and therefore `toggle()`)
from inside the build is SWALLOWED: no mount exists yet, so
`begin_close` no-ops and the open completes. Pinned
(`recursive_open_from_own_build_is_latched_and_close_is_swallowed`).
Defensible (verbs during construction are ill-formed), but the module
docs only document the open latch — a one-line doc addition would
close the gap. Left to DRAWER's judgment.

### F6 — INFO, argued back, no change: per-thread registry residue
Their surface 3. The registry holds `Weak`s and every close path —
including `HostGone` via scope cleanup — releases the slot, so a
world that dies with a drawer open mid-flight leaves only inert
residue: the stale weak upgrades to None for the next claimant and
the orphaned animate flight cancels through their new disposal guard.
Verified end-to-end
(`sequential_app_worlds_leave_no_registry_or_task_residue` — world 1
dropped mid-slide, world 2 claims the same edge cleanly,
`frame_tasks_pending() == 0` after). The remaining theoretical case —
TWO Apps LIVE simultaneously on one thread replacing each other's
drawers through the shared registry — is a non-goal (one App per
thread is the runtime reality) and is recorded here rather than
guarded in code.

### F7 — INFO, position recorded: Esc with a focused editor closes the whole drawer
The documented contract (substrate-owned Escape, content-first) holds:
`TextInput` leaves Esc unconsumed, so Esc closes a modal drawer even
mid-edit; the draft survives when the app follows the state recipe
(install-scope signals) — both pinned
(`esc_in_a_modal_drawer_with_a_focused_editor_closes_the_drawer`).
Tension worth recording: the 0515 ChoicePrompt ruling adopted
layered Esc (an engaged editor retreats first, the second Esc
dismisses — the engine's innermost-surface idiom). A drawer hosting a
form gives one-press dismissal instead. Not a defect (documented
contract, and content may consume Esc itself to opt into retreat);
filed as a UX question for a future drawer-forms consumer.

### F8 — the composition contract (drawer × PageHost), verified
The verdict the wave brief asked for. Scope semantics ARE the
contract, and they hold exactly as both components document:
- An APP-SCOPED passive drawer stays open while chords switch pages
  beneath it (`page_switch_under_a_passive_drawer_keeps_it_open`) —
  keys fall through an unfocused passive tree to the page host.
- A PAGE-SCOPED drawer (installed on the page's generation scope)
  dies with its page as `HostGone`, layers gone, tasks drained
  (`drawer_installed_inside_a_page_closes_with_host_gone_on_switch`).
- A MODAL drawer blocks page chords while open BY DESIGN (it owns
  every key); closing it restores them
  (`modal_drawer_blocks_page_chords_while_open_by_design`).
The shell example's app-scoped installs are the right default; page
tools that should die with their page install on the page scope and
get exactly that.

### F9 — geometry, storms, scrims, timers: pass as designed
- Percent rounding at odd axes is exact (41×0.5 → 21 cells hugging
  the edge; 0.25 → 10; 11×0.5 → 6) and floors at one cell; oversize
  clamps to the axis (`percent_sizes_round_and_clamp_at_odd_axes`,
  `top_and_bottom_drawers_survive_tiny_viewports` — incl. a 2×2
  viewport).
- A sticky modal scrim (`close_on_outside(false)`) swallows presses
  without leaking them to the page beneath — verified against a live
  Button, not just the dispatch return
  (`sticky_modal_scrim_never_leaks_presses_to_the_page`).
- Toggle storms (mid-flight reversals + replace churn across turns)
  drain to zero frame tasks with one coherent truth and an exact
  reason ledger (`toggle_storms_drain_to_zero_and_keep_one_truth`).
- An interval-driven page hosted in a drawer ticks ONLY while open:
  the timer rides the mount scope and dies at close; closed turns are
  idle and byte-free (`interval_page_in_a_drawer_ticks_only_while_open`)
  — the zero-idle law through the drawer's scope model.
- Theme switches while open are calm; the next open retints
  (`theme_switch_while_open_is_calm_and_lands_at_next_open`).

## Gates at review close

Whole tree `cargo test`: 1901 passed / 0 failed (includes the 16
review tests — `tests/wave_shell_review.rs` + its
`wave_shell_review_parts/charter.rs` sibling — and the new unit pin in
`src/app/drawer_review_tests.rs`). `cargo clippy --all-targets`: zero
warnings. `cargo fmt --check`: clean. `cargo semver-checks
check-release --baseline-version 0.2.11`: 196 pass, no semver update
required (the fixes are all crate-private). Files touched by fixes:
`drawer_open.rs` (+~70 incl. comments, now 453 lines), `drawer.rs`
(+3), `drawer_tests.rs` (my pin split to a `#[path]` sibling to keep
it under the 600 budget) — all files within budget.

# REACT cycle-6 requests

## To DESIGN

1. **The a11y defaults want your eye**: widgets now announce
   role/label/value (report §3). Two open vocabulary questions before
   apps bake them in: (a) should `focus_announcement()` strings be
   theme-/locale-agnostic engine text (current) or a DESIGN-owned
   format? (b) Table's value line is
   "N rows x M cols, selected row K" — say the word if the gallery
   wants different phrasing and I'll treat your spelling as the
   contract.
2. **Table keyboard sort** ships as `s` = request-next-column
   (round-robin from the sorted column). If you want a visible COLUMN
   CURSOR in the header (arrow keys move it, Enter sorts), that is a
   joint visual+behavior change — file the visual spec and I'll build
   the behavior half.
3. **Grid is ready for the gallery**: `widgets::Grid` +
   `Track::{Cells,Percent,Auto,Fr}` + spans; forms lay out as
   `Grid::new(vec![Cells(12), Fr(1.0)], vec![])`. Cell alignment
   follows `align_self` on both axes in v1 — flag real cases needing a
   separate `justify_self` and it gets built.

## To KERNEL

4. **Constructors adopted** (`KeyEvent::char/with_mods`) at my sites —
   thanks, exactly the right shape. No asks.

## To REDTEAM

5. **New assertable surface**: `accessibility_tree_text()` (stable,
   documented format), `focus_affordance_visible()` (the §3
   focus-visible rule as a one-call check — point it at any widget),
   `KeymapHelp::entries()` (chord+description fold). Doc §20 risks
   9–12 pre-name the attack spots: grid first-fit complexity, the
   Auto+span approximation, `access_value` over disposed foreign
   signals, focus-visible's synchronous-flush assumption.
6. **Wrap property tests** cover no-overlap/left-alignment/per-line
   tiling with random fixed-width children; the grow-inside-wrapped-
   lines interaction beyond that is lightly pinned (one directed test)
   — a randomized wrap+grow+margins round would be welcome.

## To GFX3D

7. **Mid-cycle breakage note** (process, not blame): the `three` wave
   left the LIB uncompilable at several points tonight (missing
   modules, changed struct fields), which blocks every other seat's
   test runs — my waves sat on `cargo check` retry loops. If a
   multi-commit refactor can keep intermediate states compiling
   (stub-first, delete-last), the whole room stays unblocked.

## To the integrator

8. **Prelude**: added `Callback` + `Role` (my files). R4-3 precedent
   says prelude refreshes are yours — fold `widgets::{Checkbox,
   RadioGroup, Grid}` and `app::KeymapHelp` in whenever you next sweep,
   or say the word and I'll add them.
9. **`Overflow` replaced `Style::clip_overflow`** (bool -> enum;
   `.clip()` builder unchanged, `.scroll()` new, `clips_children()` is
   the read). Grep found no external consumers beyond the ones I
   migrated, but it IS a public-type change worth a line in the next
   integrator note.

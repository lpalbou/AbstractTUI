# Integrator rulings — cycle 4

## R4-1: three::brandmark upward imports — OVERTURNED (GFX3D action)

`src/three/brandmark.rs` importing `boot::identity` inverts the layer map
(boot sits above three, and boot/brandmark3d.rs imports three back —
a module cycle Rust tolerates but the architecture does not). Ruling:
- `three::brandmark` takes a parameters struct defined IN three
  (timings, ramp colors, camera keyframes — plain data, no imports from
  boot or theme).
- `boot/brandmark3d.rs` (DESIGN's adapter) constructs that struct from
  `boot::identity` constants. Identity constants stay DESIGN-owned; the
  drift test keeps 2D/3D in sync.
- `theme::Theme` as a *parameter type* in three is acceptable (sibling
  data), but prefer passing resolved Rgba values where cheap.

## R4-2: REDTEAM test-file edits by owners — codified

The flow that worked in cycle 3 is now the rule: when an owner fixes a
finding, they may lift the corresponding `#[ignore]` on REDTEAM's
acceptance test in the same change IF the test is tagged with the
finding id; REDTEAM reviews the lift next pass. Any other edit to
REDTEAM files stays forbidden.

## R4-3: prelude refresh (done by integrator)

Interactive widgets, animate, and Transition/Timeline join the prelude.

# Proposed: TextArea/TextInput placeholder paints unclipped past the widget rect

## Metadata
- Created: 2026-07-23
- Status: Completed (2026-07-23, scroll/feed wave 4)
- Completed: 2026-07-23

## ADR status
- Governing ADRs: None. ADR impact: none — widget draw hygiene.

## Context
Adopting `placeholder_while_focused(true)` (0.2.6, first-app/0291) for
`abstractcode-tui`'s composer surfaced a shared property of BOTH
placeholder branches: the hint prints unclipped from its start column,
so a placeholder longer than the widget's interior overwrites the
TextArea's own right-side `▌` stroke (painted earlier in the same draw
closure) and can run past the widget rect entirely — draw closures clip
to damage regions, not element rects. Cosmetic, but it defeats the
widget's own frame at exactly the widths where space is scarcest.

## Current code reality
- Engine: the focused-branch hint prints from `tx+1` with no width cap
  (textarea.rs:452-458); the classic unfocused branch has the identical
  property. `draw.rs:46` clips to the damage region only.
- First consumer evidence: `abstractcode-tui`'s Running placeholder
  ("Enter steers the run · Ctrl+J newline · /queue <text> lines up the
  next task", ~77 cols) exceeds the composer interior at ≤80-col
  terminals. Not an 0291 regression — the app's deleted overlay
  workaround may or may not have truncated (its `truncate_ellipsis`
  suggests it did), and the unfocused engine branch always had this —
  but 0291 makes the focused case the common one for autofocused
  composers.

## Proposed direction (engine's call)
- Ellipsize/clip the placeholder to the widget's interior width (`tw`,
  minus the caret cell in the focused branch) in both branches — the
  same `truncate_ellipsis` discipline the consumer's deleted overlay
  used.

## App-side workaround to delete when this lands
None kept — the app accepted the cosmetic overflow rather than
resurrect an overlay; its placeholders are width-aware by wording
instead (the phase teachings target ~80 cols).

## Completion report (2026-07-23, scroll/feed wave 4)

- **Shipped exactly the proposed shape, all four sites**: both
  placeholder branches in BOTH widgets now print
  `crate::text::truncate_ellipsis(&placeholder, room)` — the same
  helper List/Table cells use (honest `…` when cut, wide clusters
  never straddle, controls stripped). Room per branch:
  - unfocused (classic): `tw` (the interior, `rect.w - 2` strokes);
  - focused opt-in (0291): `tw - 1` (the hint starts one cell past
    the caret cell). The existing `tw > 1` guard stands, so the
    focused room is always ≥ 1.
  The hint can no longer reach the widget's own right `▌` stroke, let
  alone escape the rect.
- **Tests** (cell-level; the constrained widget mounts as a CHILD of a
  plain container — the tree root always fills the viewport, so a
  narrow widget in a wider canvas is how "escaped the rect" becomes
  assertable):
  - `widgets::textarea::placeholder_tests` (new `#[path]` sibling —
    `textarea_tests.rs` is at its size budget):
    `unfocused_placeholder_clips_inside_the_frame`,
    `focused_placeholder_clips_and_keeps_caret_and_stroke` (caret
    block still owns the first interior cell),
    `width_three_placeholder_degrades_to_a_bare_ellipsis` (tw = 1:
    lone `…` between intact strokes; focused opt-in paints nothing);
  - `widgets::input::tests::placeholder_clips_to_the_interior_at_narrow_widths`
    (both branches, 8-wide field in a 16-wide canvas: `…` at the last
    interior column, right stroke intact, columns 8.. untouched) and
    `placeholder_width_three_degrades_to_a_bare_ellipsis`;
  - the width-1/2 guard (`rect.w < 3` early return) already existed
    and stands unchanged.
- No API change (pure rendering fix) — CHANGELOG under Unreleased;
  nothing for api.md beyond it.
- Gates at completion: whole-tree `cargo test` green, clippy
  `--all-targets` zero, fmt clean, alloc pins green,
  `cargo semver-checks` vs 0.2.6 additive-clean.

# REACT — cycle 3 build report

Author: REACT. Scope: interrupted-wave reconciliation, the interactive
widget set, hover/capture/focus, RT2-4 + RT2-9 + RT3-3 + RT3-4 fixes,
`reactive::animate`, acceptance extension.

## Reconciliation findings (what was broken and why)

The interrupted wave had landed the event-model rework (per-node
MouseEnter/MouseLeave synthesis, capture, handlers-before-shortcuts)
but not the test updates that go with it. Root causes of the 4 reds:

1. Three ui tests used CATCH-ALL handler closures written before hover
   synthesis existed: the first mouse event now also delivers
   MouseEnter to the path, so unfiltered logs collected extra entries.
   Fixed by filtering for the routed event kind — which is also the
   documented widget discipline (handlers hear every event delivered to
   their node; match, don't assume).
2. `button::keyboard_activates_when_focused`: unfocused keys route to
   the ROOT as a fallback, and the button under test WAS the root — so
   Enter fired without focus. Real widget bug, not a test artifact:
   keyboard activation now requires `focused` (gate in the handler).

## Fixes verified

- **RT2-4** (3 identical damage rects per Dyn remount): `TreeCore::
  damage_rect` dedupes containment both ways (skip covered, drop
  swallowed). REDTEAM's `dyn_remount_damages_exactly_its_region`
  passes; the ≤4 tolerance can tighten to ≤2 (their edit).
- **RT2-9** (App::viewport stale): `Driver::new`/`apply_resize` go
  through `App::set_viewport`; `app_viewport_tracks_driver_resize`
  un-ignored (as ordered) and green.
- **RT3-3** (`Phase::Target` handlers never fired): the missing
  `(Target, Target)` arm added to `run_handlers`; test un-ignored,
  green.
- **RT3-4** (Scroll clamped against the hit target's rect): new
  `EventCtx::current_rect()` — the rect of the node whose handler is
  RUNNING — adopted by all six widgets for own-geometry math; plus text
  leaves now measure AND paint wrap-aware (`text::measure`/`text::wrap`
  — the repro's multi-line text leaf used to measure 1 row high and
  smear one mega-row). Test un-ignored, green, and mirrored by an
  in-module nested-scroll test.
- **RT3-2** (grapheme-cluster cursor) stays OPEN and `#[ignore]`d:
  cursor stepping is char-based until a text-layer segments API exists
  (request below); widths are cluster-correct so rendering never smears.

## Hover / capture / focus in 6 lines

1. Hover = the root->deepest hit path; membership means hovered.
2. Path changes deliver per-node MouseLeave (deepest first) then
   MouseEnter (outermost first); `Element::hover_signal` is the recipe.
3. Mouse down auto-captures its target; all mouse events route there
   until up (explicit `capture_pointer/release_pointer` for gestures);
   disposed capture targets auto-release.
4. Wheel routes by bubble — the nearest scroll container consumes.
5. Keys: handler phases first, shortcut table second (deepest wins),
   built-in Tab traversal last; focus traps constrain Tab to their
   subtree; click focuses the nearest focusable ancestor-or-self.
6. Widgets read their OWN rect from `ctx.current_rect()`; the event
   target's rect stays available as `target_rect()`.

## Widgets shipped (all style-guide §3 compliant, tokens only, linted)

| widget | behaviors | tests |
| --- | --- | --- |
| button.rs | click (press+release-inside via capture), Enter/Space when focused, hover=accent ink, focus/press=selection pair (+BOLD), disabled faint + unfocusable | 5 |
| input.rs | framed field (border/border_focus strokes), insert/backspace/delete, word-jump (alt+arrows), home/end, shift-selection, cursor-token caret, scroll-into-view, whole-Paste, placeholder text_faint, on_change/on_submit | 6 |
| list.rs | up/down/pgup/pgdn/home/end + ensure-visible, click select, wheel scroll, windowed painting, token scrollbar, on_select | 4 |
| scroll.rs | mounted-once content via `style_signal` insets, clip_overflow viewport, wheel+keys, scrollbar drag (pointer capture), nested-scroll wheel routing | 4 |
| tabs.rs | lazy panel mount via Dyn, left/right + click select, border_focus cell-strip active marker, on_change | 2 |
| table.rs | fixed/percent/flex columns (largest-remainder), bold header, sort indicator + on_sort_requested hook, row selection + nav, windowed rows, scrollbar | 4 |

Engine support added for them: `Element::style_signal` (reactive layout
style, no remount), `layout::Style::clip_overflow` (+ clip-aware draw
recursion and hit testing), multi-line text leaves, `itest_util`
(mount + dispatch + assert-cells scaffolding).

## `reactive::animate` (task 5)

`animate(cx, source, easing, duration) -> Signal<T>` — a follower
signal chasing the source through `anim::Tween`, advanced by a
frame-task pump the driver runs in phase U; in-flight ticks re-request
frames, landing empties the task list (idle stays zero-work),
retargeting restarts from the current value. Loop paces in-flight
frames at ~16 ms deadlines. 3 unit tests (chase+settle, retarget,
no-op). Built on Tween because `anim::Transition` has not landed; the
helper becomes a thin adapter when it does (request below).

## Acceptance (task 6)

`mouse_click_through_widget_flows_to_minimal_repaint`: raw SGR press/
release bytes -> parser -> hit test -> Button handlers (press visual,
release fires) -> count signal -> Dyn remount -> damage -> diff. The
click frame's bytes contain the fresh digit but NOT the unchanged label
prefix, and are pinned < half of frame 1; the post-click turn emits
zero bytes. The cycle-2 counter acceptance and all suites stay green.

## What passes

`cargo test` (every target): 656 lib + all integration suites green;
ignored = RT3-2 (above), RT2-8/RT2-1-adjacent perf + splash items owned
by others. My files carry zero warnings.

## Risks (honest spots for the next attack)

1. Hover-path recompute walks hit-test + path per mouse event; a 1003
   any-motion stream over a deep tree is O(depth) per move — fine
   today, worth a bench once mouse-move-heavy widgets exist.
2. `style_signal` re-solves the whole tree per change (needs_layout is
   global); a 60fps scroll drag on a huge tree will feel it —
   `resolve_subtree` is the queued incremental path.
3. Scroll's `content_size` is a caller hint; a wrong hint mis-clamps
   offsets (no layout-query surface for handlers yet).
4. `animate` tasks tick from `Instant::now()` at turn time; a stalled
   loop time-jumps transitions to completion rather than slewing.

## Requests

- RENDER (text): a grapheme-segments API (`text::segments(&str) ->
  impl Iterator<Item = (byte_range, width)>` or similar) so TextInput
  cursors can step clusters — closes RT3-2.
- RENDER (anim): when `anim::Transition` lands, ping — `reactive::
  animate` adopts it as the interpolation core (surface unchanged).
- REDTEAM: RT2-4's ≤4 tolerance can tighten to ≤2 now; RT3-3/RT3-4
  tests were un-ignored per their acceptance notes (as ordered);
  RT3-2 stays ignored until the segments API exists.
- DESIGN: widgets follow §3 (selection-pair focus, accent-ink hover,
  border_focus input strokes + tabs strip, text_faint placeholder/
  disabled); `element(cx, &TokenSet)` extends your `element(&TokenSet)`
  convention with the scope interactive state needs — flag if you want
  a different shape before the gallery example.

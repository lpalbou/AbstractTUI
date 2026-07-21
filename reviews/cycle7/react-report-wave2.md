# REACT cycle-7 report — wave 2 (performance hardening + interaction polish)

Supplements the earlier cycle-7 report (ergonomics/context wave, same
file directory). At close: lib **884 passed / 0 failed**, `cargo test
--no-run` clean, ZERO clippy warnings in owned files.

## 1. §20 risk closures — all four, with numbers

- **(a) Grid complexity**: closed structurally LAST wave (sparse
  forward-only cursor = O(occupancy area)); this wave adds the ordered
  MEASUREMENT — release build, 12 fr columns, adversarial span mix
  (7/5/11/2-wide, row-spans 1-3):
  | items | solve time |
  | --- | --- |
  | 250 | 463 µs (cold) |
  | 500 | 164 µs |
  | 1000 | 518 µs |
  | 2000 | 695 µs |
  Linear past warmup; 2000 spanned items solve in under 0.7 ms. No
  skyline structure needed at these numbers — bound documented in the
  grid module + design doc.
- **(b) Auto+span**: pinned last wave by
  `auto_span_boundary_contributes_ceil_to_start_track_only` (the exact
  clipping consequence stated as asserts). Stands.
- **(c) access_value over disposed signals**: inert WITH LABEL —
  `Signal::try_get_untracked` (endorsed read, `None` on disposed) plus
  the snapshot's unwind backstop yielding `"<stale>"` (the label) for
  closures that panicked anyway. Test-pinned.
- **(d) focus-visible flush**: the hook flushes effects EXPLICITLY
  around both comparison draws; the synchronous-focus-visuals contract
  is documented as design (a widget deferring focus visuals through
  timers fails the check deliberately).

## 2. Hover under mouse-move storms — coalescing + numbers

THE COALESCING RULE (documented at the driver call site + doc §15.7):
within one phase-U batch, only the LAST of each consecutive run of
plain Move events dispatches. Intermediate hover positions were never
visible (no frame rendered between them) so nothing observable is
lost; Drag/Down/Up/Wheel NEVER coalesce (capture and click handlers
see every event); a non-mouse event between moves breaks the run
(ordering with keys preserved). `coalesce_moves` is an order-preserving
in-place compaction, unit-pinned (runs collapse to their last member,
drags survive untouched).

Measured (release, 1000-node tree with handlers, 10k dispatches
through the REAL `UiTree::dispatch`):

| scenario | total | per event |
| --- | --- | --- |
| every move a new position | 7.6 ms | ~0.76 µs |
| 10k moves at ONE position (memo) | 4.7 ms | ~0.47 µs |
| 10k Down events (baseline) | 1.7 ms | ~0.17 µs |

Even worst-case hover is sub-microsecond per event; with coalescing a
10k-move burst pays for ONE dispatch per frame. The cycle-5 memo plus
this batch rule close the named risk with headroom.

## 3. Focus polish

- **Focus memory**: `Element::focus_memory()` marks a container; every
  focus change records into all enclosing memory containers; Tab
  ENTERING one from outside restores its last-focused descendant
  (moving within stays tab order; dead/unfocusable memories fall back
  silently). Test: leave a 3-item pane, re-enter, land where you left.
- **Initial focus**: `Element::autofocus()` — mount focuses it (last
  mounted wins; works through Dyn regenerations via the pending slot
  consumed AFTER the mount borrow releases); `UiTree::focus_first()`
  is the explicit fallback policy. Nothing auto-focuses without one of
  the two.
- **Spatial nav**: `UiTree::focus_next_in(Key::Up/Down/Left/Right)` —
  candidates in the direction's half-plane from the focused rect
  center, scored `primary + 2x orthogonal` (nearest-in-direction),
  trap-scoped, returns whether focus moved. Unit test (2x2 grid full
  cycle) + ACCEPTANCE through the real driver: Alt+arrow ACTIONS
  (registered via `app::actions`, fed as raw CSI bytes through
  CaptureTerm) move focus pane-to-pane, asserted via the a11y
  snapshot's focused label.

## 4. List hardening

- **Variable heights**: `item_heights(fn(idx, item) -> rows)` —
  prefix-sum windowing (offset in CONTENT ROWS; first visible item by
  binary search; ensure-visible on item top/bottom rows). ONE code
  path: uniform lists are the identity prefix. v1 honesty: extra rows
  reserve space, the label renders on the item's first row (wrapped
  multi-row content is a later decision, documented).
- **Sticky selection by key**: `key_fn` + `selection_key(Signal
  <String>)` — element build re-finds the key's CURRENT index (data
  mutations rebuild through the caller's Dyn, so the same logical item
  stays selected — test: two rows inserted above the selection, index
  moves 1 -> 3, key unchanged); selecting writes the key back.
- **`scroll_to(Signal<Option<usize>>)`**: command signal, scrolls the
  item's top row into view, consumed (reset to `None`) after — one
  bounded extra effect run, labeled.
- Click row->item mapping is the same binary search (a click on any
  row of a tall item selects that item; test-pinned). Table inherits
  the shared scrollbar; its rows are uniform (1 row = 1 item) so the
  prefix machinery does not apply — key-selection for Table is
  deferred until DESIGN's dashboard asks (noted, not hidden).

## 5. Startup notices

`App::push_startup_notice` / `App::startup_notices() -> &[String]`.
`App::run` (unix) reads `UnixTerminal::degraded()` AFTER `Driver::new`
— the concrete-type read point before erasure to `dyn Terminal` — and
records `"input: degraded (<label>)"`; both `run`/`run_on` record a
one-line caps summary (`"caps: truecolor+kitty-kbd+sync+tmux"`,
honest about what is off: `16color` when nothing better). DESIGN's
examples can render the list at boot. Filed to KERNEL: a defaulted
trait accessor would let `run_on` (dyn) see degradations too.

## 6. Risks

- Coalescing is GLOBAL: a future free-draw widget wanting raw motion
  trails needs an opt-out knob (none exists in-tree; the rule + the
  gap are documented at the call site).
- Focus memory holds ViewIds of possibly-disposed instances; lookups
  verify liveness+focusability before restoring, and stale entries are
  overwritten on the next focus — the map is never proactively swept
  (bounded by container count; a sweep is a later nicety).
- `focus_next_in`'s metric is deliberately simple (center-to-center,
  half-plane): overlapping panes or L-shaped layouts can pick a
  surprising neighbor; the dashboard should report real cases before
  the metric grows edges/projection refinements.
- Variable-height items + `draw_scrollbar` share the rows vocabulary;
  Table still passes item counts (rows there) — if Table ever gains
  variable heights, it adopts the prefix machinery rather than forking.

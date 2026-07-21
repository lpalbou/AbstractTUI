# REACT cycle-7 report

Ergonomics + robustness polish, all ordered items shipped. At close:
lib **876 passed / 0 failed** (a transient foreign `boot::fallback2d`
failure resolved by its owner before my close), `cargo test --no-run`
clean, ZERO clippy warnings in owned files. KERNEL items verified: `present_caps` undercurl fix
confirmed closed (the cycle-5 test `present_caps_mapping_delegates_to_
kernel_including_underline` pins it); all my `input::KeyEvent`
constructions already use `char()/with_mods()` (adopted cycle 6); I
construct NO `input::MouseEvent` anywhere (the conversion only READS
fields; `ui::MouseEvent` literals are my own same-crate type).

## 1. Ergonomics acceptance — both proofs TESTED, not just written

- **The <60-line real app**: `ui::compose::tests::todo_app` — a 40-line
  module (imports and blanks counted) with a TextInput (placeholder,
  submit-on-Enter, clear-after-add), a selectable List rebuilt
  fine-grained via `dyn_view_scoped`, and a live "N todos" line.
  `sixty_line_app_proof_renders_and_reacts` drives it through real
  dispatch (Tab, typing, Enter) and asserts the screen.
- **Shareable component across two apps**:
  `one_component_reused_across_two_apps` mounts the SAME Card component
  function into two independent trees/scopes with different props —
  the "define once, use anywhere" requirement made literal.
- **The hard pass found one real API wart and fixed it**: TextInput ran
  `on_change`/`on_submit` INSIDE `value.with_untracked(..)` — holding
  the cell borrow through user code, so a submit handler that clears
  the input (every real form) panicked "RefCell already borrowed". The
  proof app hit it immediately. Fixed: `notify` clones the value out,
  then calls (one String clone per event — the honest price); both
  paths share the helper. This is exactly what the acceptance exercise
  was for.
- Remaining awkwardness filed to the integrator (below): widget
  `element(cx, &TokenSet)` verbosity inside `dyn_view` closures.

## 2. RT6 risks — all four closed

- **Risk 9 (grid first-fit complexity)**: auto-placement switched to
  CSS-DEFAULT SPARSE packing — a forward-only cursor; total scan work
  is bounded by the occupancy area (linear in input), never
  O(children²). Behavior delta from dense: later children never
  backfill earlier gaps — which is what CSS does by default, so this
  is a correctness alignment too (`sparse_placement_never_backfills_
  and_is_linear` pins it; all prior grid tests unchanged — their
  expectations matched both packings).
- **Risk 10 (Auto+span approximation)**: documented PRECISELY and
  pinned by `auto_span_boundary_contributes_ceil_to_start_track_only`
  — the test states the exact clipping consequence at the boundary
  (a spanner's cell can be smaller than its intrinsic height when the
  later spanned rows are sized smaller by their own children). A future
  multi-pass distributor changes those asserts deliberately.
- **Risk 11 (access_value over disposed signals)**: two layers.
  `Signal::try_get_untracked()` (+ `Signal::is_alive`) is the endorsed
  read for closures that outlive their data — `None`, never a panic;
  and the a11y snapshot wraps every value closure in an unwind guard
  (`"<stale>"` fallback) so even a panicking closure cannot kill the
  snapshot. Test: `try_get_untracked_is_inert_after_disposal`.
- **Risk 12 (focus-visible flush assumption)**: the hook now flushes
  effects EXPLICITLY around both comparison draws — no reliance on
  `set_focus`'s internal batch. The CONTRACT is kept and documented:
  focus visuals must be synchronous (signal -> Dyn/draw); a widget
  deferring them through timers/frame tasks fails the check BY DESIGN.

## 3. State management + context (the lighter option, as ordered)

- **`Scope::provide_context<T: Clone>(value)` /
  `Scope::use_context::<T>()`** shipped in the reactive core: nearest-
  provider lookup via a new `Node.parent` link (mirrors the ownership
  edge, set at creation), sparse side-map storage (only providers pay),
  entries removed in the dispose walk and dropped OUTSIDE the borrow
  (a value's Drop may re-enter the runtime). Nested provides shadow
  their subtree; one value per type per scope; distinct types coexist.
  Tests: flow-down/shadowing/dispose, and the signal-as-shared-store
  pattern (`context_signal_is_the_shared_store_pattern`).
- **Signals-as-store convention** documented (ui::compose + design doc
  §18.1): a `Clone` store struct of signals provided at the root,
  actions as plain functions, memo chains for derived state — no
  reducer framework (deliberate: signals + context already give
  fine-grained subscriptions; a reducer would add ceremony, not power).
- **Memo cookbook** added to the compose doc (filtered views, chained
  memos, lazy-expensive derivations).

## 4. Router decision

NO router type, documented with rationale and a copyable pattern
(compose doc + §18.1): page switching is an enum signal +
`dyn_view_scoped` (page-local state dies on navigation — the scope IS
the unmount semantics), optionally provided as context so any component
navigates. `Tabs` is this pattern with a bar attached. A router earns
its keep when a real app demonstrates history/deep-linking needs — not
speculatively.

## 5. Clippy + verification

Owned files at zero (one new `ContextEntries` type alias resolved the
type-complexity nit in the runtime). `cargo test --no-run` clean at
close; lib 876/0.

## 6. Risks / notes

- Sparse packing is a BEHAVIOR CHANGE for grids that relied on dense
  backfill — no in-tree consumer did (all grid tests passed unchanged),
  and sparse is the CSS default users expect; flagged in the doc.
- The snapshot's unwind guard prints panic output to stderr when a
  value closure panics (the hook still returns "<stale>"): noisy but
  honest — the closure HAS a bug; the guard exists so a11y dumping
  never masks it into a crash.
- `use_context` walks parents on every call (no caching): fine at UI
  scale (walks are tens of nodes); a memoizing layer is a measured
  later decision if a profile ever shows it.

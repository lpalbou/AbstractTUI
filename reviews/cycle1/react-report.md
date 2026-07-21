# REACT — cycle 1 build report

Author: REACT. Scope: `src/reactive`, `src/layout`, `src/ui`, `src/app`,
`docs/design/reactive-ui.md`.

## Shipped

- `src/reactive/`: `arena.rs` (hand-rolled generational slotmap),
  `node.rs` (node kinds + paired-slot edge bookkeeping), `runtime.rs`
  (thread-local runtime, tracking, two-phase marking, disposal, flush,
  batch/untrack/on_cleanup), `execute.rs` (computation runs, equality
  cut-off, pull phase), `signal.rs`, `memo.rs`, `effect.rs`, `scope.rs`
  (+ `RootScope`, `create_root`), `scheduler.rs` (`FrameRequester`,
  `WakeHandle`, posted-work drain, coalesced frame requests), `tests.rs`
  (behavioral suite).
- `src/layout/`: `style.rs`, `tree.rs` (arena + measure callbacks +
  geometry damage), `flex_math.rs` (largest-remainder distribution,
  freeze-loop resolution), `solve.rs` (row/col, justify, align,
  gap/padding/margin, grow/shrink/basis, min/max, percent, absolute),
  tests in `mod.rs`.
- `src/ui/`: `view.rs` (Element/Text/Dyn blueprints + builders),
  `canvas.rs` (`Canvas` trait + `BufferCanvas` test impl), `event.rs`
  (keys/mouse/phases/chords/EventCtx), `mount.rs` (mount lifecycle, Dyn
  reactive regions), `tree.rs` (instances, damage, hit test, dispatch,
  focus, shortcuts), tests in `mod.rs`.
- `src/app/`: headless-capable `App` (mount/pump/draw/shutdown, wake
  handle), full terminal-loop design in doc comments, honest
  `Unsupported` from `run()` until cycle 2.
- `docs/design/reactive-ui.md`: design + literature survey (SolidJS,
  reactively, leptos 0.7, sycamore 0.9) + decisions with rejected
  alternatives + REDTEAM attack map.
- `reviews/cycle1/react-requests.md`: contract requests to RENDER,
  KERNEL, DESIGN, integrator.

## The reactivity design in 10 lines

1. All nodes (signal/memo/effect/scope) live in one thread-local arena;
   handles are `Copy` (u32 index + u32 generation + runtime stamp).
2. Reads record `source -> current observer` edges; edges are unlinked
   O(1) via paired slot indices and re-tracked on every run.
3. Writes run a two-phase mark: direct observers `Dirty`, transitive
   `Check`; effects hit along the way are queued; first-staleness-only
   descent keeps repeated writes O(1).
4. Effects flush in creation order, synchronously after un-batched
   writes; `batch()` defers to one flush; cascades settle in-flush.
5. Each queued effect PULLS its sources up to date (`Check` resolution),
   so it sees one consistent world — diamonds run leaves exactly once.
6. Memos are lazy (born dirty, computed on observation) and cut off
   propagation when the recomputed value compares equal.
7. Computations are owners: re-runs dispose children + run cleanups
   (LIFO) first; disposal is children-first, iterative, slot-freeing.
8. Cross-thread work arrives only as posted closures via `WakeHandle`;
   the graph never crosses threads (misuse panics on the stamp).
9. UI `Dyn` regions = one effect owning a per-generation child scope;
   re-run disposes the old subtree, mounts the new, damages its rect.
10. Frame requests coalesce; idle means empty queue, zero work.

## Decisions + rejected alternatives (full rationale in the design doc)

- Two-phase marking over topological levels (dynamic deps break heights;
  laziness) and over pure push (glitches, no cut-off).
- Synchronous flush (Solid 1.x) over microtask-style deferral (no
  microtask queue in a TUI; `batch` covers coalescing; revisit if input
  storms say otherwise).
- Explicit scope ownership (`cx.signal(...)`) over implicit current-owner
  creation (leak visibility at call sites); `on_cleanup` stays implicit
  (targets the running computation — the Solid contract).
- Single-threaded graph + message passing over `Send + Sync` graph
  (lock-free reads; terminals serialize output anyway).
- `Send` handles with runtime-stamp checks over `!Send` handles (posted
  closures must carry handles across threads; misuse = loud panic).

## What passes

`cargo check` clean. Crate-wide `cargo test --lib` at the time of
writing: 269 passed, 1 failed — the failure is
`testing::snapshot::tests::diff_detects_whitespace_only_change` in
`src/testing/snapshot.rs` (REDTEAM's own in-progress file, outside my
paths). All 57 tests in my areas are green (26 reactive incl. arena/
node/scheduler units, 19 layout, 9 ui, 2 app, 1 doctest), including: diamond ×2, memo cutoff, `set_if_changed`, memo
laziness, batch coalescing (incl. nested), effect creation-order flush,
disposal order (children-first, LIFO, sibling reverse-creation), effect
cleanup-before-rerun, slot-freeing, stale-handle panic, 10k
create/dispose no-leak (live nodes AND slot capacity bounded), Dyn-style
re-render no-leak, dynamic dependency retracking, in-flush write
settling, wrong-thread panic, posted-closure wakeups, cycle detection,
edge-invariant churn; layout: 4/3/3 rounding, exact tiling, padding/gap,
margins, percent-of-content-box, min/max freeze redistribution,
justify + space-between rounding, align/stretch, absolute insets,
measure callbacks, intrinsic container sizing, nested solve, subtree
removal; ui: mount+draw, Dyn remount damage + instance stability, full
unmount, hit testing, capture/target/bubble order, stop_propagation,
Tab/Shift-Tab focus cycling + FocusIn/Out synthesis, deepest-wins
shortcut resolution; app: headless mount/pump/draw cycle + idle
zero-work, honest `run()` error.

## Gaps deferred (named, not hidden)

- Terminal `run()` loop (cycle 2, needs KERNEL poll + RENDER present).
- Text measurement is 1-cell-per-char placeholder until text/ lands.
- Region-filtered painting (draw walks everything; damage rects are
  produced but the compositor consumes them in cycle 2).
- Whole-tree re-solve on structural change (`resolve_subtree` exists;
  wiring incremental solving needs the damage plumbing end-to-end).
- Mouse capture (drag outside), hover enter/leave synthesis, percent
  insets, auto margins, flex wrap.
- Layout `solve` treats deep intrinsic sizing recursively (tree-depth
  stack use, documented).

## Riskiest spots for REDTEAM (please attack)

1. `runtime::track_read` epoch dedupe + nested-pull fallback scan —
   interleave nested memo pulls with repeated/overlapping reads and hunt
   for missing or duplicated edges (missing = missed update).
2. Mid-flush disposal: an effect disposing later-queued effects, its own
   ancestor scope, or nodes a suspended `update_if_necessary` walk still
   references (`sources[idx]` re-read is guarded — break it).
3. Dependency DEPTH: user computations nest on the native stack (memo
   reading memo reading...); 10k-deep chains will overflow. Framework
   traversals (mark, dispose, uin) are iterative — verify no recursion
   crept in elsewhere (layout intrinsic sizing is recursive by tree
   depth; ui draw/hit-test are iterative).

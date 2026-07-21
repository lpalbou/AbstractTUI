# Reactive UI: signals runtime, layout, component tree

Owner: REACT. Status: cycle 2 — runtime, layout solver, ui contract AND
the terminal frame loop shipped (see §13 for the loop as built and the
pinned dispatch semantics; the loop obeys
`docs/design/01-damage-contract.md`, which is binding).

This document records the design of `src/reactive`, `src/layout`,
`src/ui`, `src/app`, the literature it draws on, and the decisions that
were made against alternatives (with reasons, so future cycles don't
relitigate them blind).

## 1. The bet, restated precisely

A TUI frame is cheap to *emit* but expensive to *decide*. Immediate-mode
stacks re-decide the whole frame every time; retained-widget stacks
re-decide coarse subtrees. Fine-grained reactivity moves the deciding
into the data graph: a `Signal` write re-runs exactly the computations
that read it, and each UI region is owned by one such computation, so the
set of re-run computations IS the damage set. Idle means an empty effect
queue: zero work, zero wakeups.

## 2. Sources studied

- SolidJS's reactive core: dependency tracking at read time via a
  running-observer context, computations as owners, `onCleanup`,
  batching. Ryan Carniato, "Building a Reactive Library from Scratch"
  (<https://dev.to/ryansolid/building-a-reactive-library-from-scratch-1i0p>)
  and "A Hands-on Introduction to Fine-Grained Reactivity"
  (<https://dev.to/ryansolid/a-hands-on-introduction-to-fine-grained-reactivity-3ndf>).
- reactively (Milo M.), the cleanest statement of the lazy two-phase
  ("graph coloring") algorithm this runtime implements:
  <https://github.com/milomg/reactively/blob/main/Reactive-algorithms.md>
  and the companion write-up "Super Charging Fine-Grained Reactive
  Performance" (<https://milomg.dev/2022-12-01/reactivity>). Leptos 0.7's
  `reactive_graph` adopted this algorithm family.
- Leptos: arena/slotmap storage of nodes behind `Copy` handles, and the
  ownership tree ("a garbage collector where data lifetime is tied to UI
  lifetime, not lexical scope") — `ARCHITECTURE.md`
  (<https://github.com/leptos-rs/leptos/blob/main/ARCHITECTURE.md>), the
  book's "Life Cycle of a Signal" appendix
  (<https://book.leptos.dev/appendix_life_cycle.html>), and
  `reactive::owner::Owner` docs (cleanup order: children first, then own
  cleanup functions, then arena values).
- Sycamore 0.9 "Reactivity v3": the migration from lifetime-scoped
  signals to `'static + Copy` handles over a slotmap in a singleton root
  (<https://sycamore.dev/post/announcing-v0-9-0>). Confirms the arena
  approach is what makes handles ergonomic in Rust closures.
- Solid 2.0 direction: default microtask batching with `flush()`
  (<https://github.com/solidjs/solid/blob/next/documentation/solid-2.0/01-reactivity-batching-effects.md>).
  We deliberately stay on Solid 1.x-style synchronous flush semantics —
  see §6.

## 3. Storage: hand-rolled generational arena

`reactive::arena::GenArena<T>`: `Vec<Slot<T>>` + free list; `Key` =
`u32` index + `u32` generation; generations start at 1 and bump on every
free, so stale keys can never resolve. No `unsafe`, no dependency —
`slotmap` semantics per the dependency policy.

Why an arena instead of `Rc` nodes: the dependency graph is cyclic by
construction (sources list observers, observers list sources), and `Rc`
cycles leak; `Weak` everywhere is slower and noisy. Arena keys are
`Copy`, so `Signal<T>`/`Memo<T>`/`Effect`/`Scope` handles move into any
number of closures without clones or lifetimes — exactly the ergonomic
lesson of sycamore 0.9 and leptos.

Handles carry a third field: the id of the runtime (thread) that minted
them. Two threads each own a runtime with overlapping indices; without
the stamp, a handle crossing threads could silently alias a foreign
node. With it, cross-thread use panics with a precise message. Handles
stay `Send` on purpose — they must ride inside `WakeHandle::post`
closures; the check moves misuse detection from the type system (which
would also forbid the legitimate transport) to the first use.

The same arena stores the layout tree's nodes and the ui instance tree
(`LayoutId`, `ViewId` newtypes), so "stale id cannot corrupt" holds
across all three trees with one mechanism.

## 4. Dependency tracking

The runtime keeps `current_observer: Option<Key>`. Computations
(memos/effects) set it while running (saved/restored on the native call
stack, panic-safe via a drop guard); signal/memo reads add an edge
`source -> observer`. `untrack(f)` clears it for `f`.

Edges are stored twice with **paired slot indices** (SolidJS's
`sources/sourceSlots` + `observers/observerSlots`): each side remembers
its position in the other side's list, so unlinking one edge is O(1)
(`swap_remove` + one back-pointer repair). A computation re-run unlinks
all its old edges first and re-tracks from scratch — dynamic
dependencies (branching reads) are handled by construction, not by
diffing dependency sets.

Per-run read dedupe: each run stamps a fresh epoch on the observer;
sources remember the epoch that last recorded them. Epoch hit = O(1)
skip (a computation reading the same signal 1000 times in a loop adds
one edge). Epoch miss falls back to scanning the observer's current
source list — necessary because a *nested* run (memo pulled mid-compute)
overwrites the stamp on shared sources; a pure global-epoch check would
either duplicate or (in the inverted formulation) *drop* the outer
computation's edge. That subtlety is diamond-correctness territory and
has a dedicated code comment.

## 5. Invalidation: two-phase marking (chosen) vs topological levels (rejected)

**Chosen — two-phase marking** (reactively/leptos-0.7 family). Three
states: `Clean`, `Check` ("maybe stale"), `Dirty` ("definitely stale").

- Down phase (at write time, iterative, no user code): direct observers
  of the written signal become `Dirty`; transitive observers become
  `Check`. Only a node's FIRST transition away from `Clean` descends —
  repeated writes touch already-marked regions in O(1). Effects
  encountered are pushed onto the effect queue.
- Up phase (at observation time): reading a `Check` node asks each of
  its sources, in read order, to `update_if_necessary` themselves; the
  first source that actually recomputes *to a different value* marks its
  direct observers `Dirty`, and the reader recomputes. If every source
  resolves clean, the reader un-colors to `Clean` without running — this
  is how memo equality cut-off stops whole subgraphs.

Glitch freedom (the diamond): `a -> b, c -> d`. The write marks b and c
`Dirty`, d `Check`, and queues d once (the second path finds it already
non-clean and stops). When d runs, it PULLS b then c up to date before
using them — d executes exactly once per write and never observes b and
c from different generations of a. This is test-pinned
(`diamond_runs_leaf_once_per_write`, `deep_diamond_converges_once`).

**Rejected — topological levels/heights** (each node stores its height;
a priority queue processes dirty nodes lowest-height first). Two
reasons. (1) Dynamic dependencies break heights: a conditional read can
re-parent a node under a *taller* subtree mid-flush, forcing re-heighting
of an arbitrary portion of the graph (MobX pays real complexity here).
Two-phase marking has no global order to maintain — correctness comes
from pulling, which follows the actual current edges. (2) Laziness:
height-ordered pushing runs every dirty node; we want memos that nobody
observes to *never* run. Pull-based resolution gives that for free.

**Rejected — pure push (Svelte-store style)**: simplest, but over-runs
diamonds (glitches) and cannot cut off propagation on equal memo values
without extra machinery.

## 6. Scheduling and batching

Effects never run inline inside a write. A write marks + enqueues, then
(outside `batch` and outside an ongoing flush) flushes synchronously.
`batch(f)` defers the flush to the end of the outermost batch, so N
writes cost one flush. Writes performed BY effects during a flush enqueue
into the same drain loop (settling cascades in one flush); a hard
iteration ceiling (100k runs) converts "effect writes its own
dependency" from a frozen UI into a diagnosable panic — and true
self-reads are caught earlier by a `running` re-entry check.

The queue drains in **creation order** (each node carries a monotonic
creation stamp). Rationale: parents are created before the effects of
children they might dispose; running outer effects first lets an outer
re-render dispose stale inner effects, which then skip via the
generation check instead of running against dead state. Ordering is
deterministic, which the test suite pins.

Why synchronous flush instead of Solid 2.0's microtask default: a TUI
has no microtask queue; the analogous design would defer to "next loop
turn". Synchronous flush after each un-batched write keeps
read-after-write coherent for imperative event-handler code (the common
TUI case), while `batch` gives the coalescing behavior where it matters.
Revisit if input-storm profiling says otherwise (the app loop can wrap
each input drain in one `batch` — one line).

## 7. Ownership and disposal

Every node is owned by a scope or by a computation (computations are
owners, SolidJS-style). `cx.signal/memo/effect/child` attach to the
scope EXPLICITLY — implicit current-owner creation makes it too easy to
hang state off a long-lived root invisibly; explicit scopes put
ownership at the call site. The implicit piece is `on_cleanup(f)`
(free function), which targets the *currently running* owner so effect
teardown reads naturally inside effect bodies.

Disposal (`Scope::dispose`, effect re-run "clear", `RootScope` drop):

1. children first, in reverse creation order (children may capture
   parent-provided state — teardown mirrors construction, same order
   leptos documents for `Owner::cleanup`);
2. own cleanups, LIFO;
3. unlink all dependency edges (both directions, O(1) each);
4. free arena slots (generation bump) — stale handles detectably dead.

Cleanups and user value drops run OUTSIDE the runtime borrow (they may
re-enter). The whole walk is iterative — disposal depth never rides the
native stack. Effect re-run performs steps 1–3 keeping the node itself,
so `Dyn` regions dispose their previous render generation for free.
Leak-boundedness is test-pinned: 10k create/dispose cycles leave
`live_nodes` at baseline and slot capacity bounded by peak concurrency;
owner child-lists sweep amortized so explicit-dispose churn under a
long-lived scope stays bounded.

## 8. Single-threaded core + message-passing wakeups (vs Send+Sync graph)

The graph is a `thread_local`; nothing about it is `Sync`. Reasons:

- Reads are the hot path (every computation, every frame). A shared
  graph means a lock or atomic dance per read (leptos 0.7 pays
  `RwLock`-family costs to be `Send + Sync` because it must serve
  multi-threaded async servers). A terminal app has ONE output stream
  and ONE input stream; parallelizing the graph buys nothing there.
- Single-threaded interior mutability is `RefCell`, checked, no unsafe;
  the borrow discipline (never run user code under the runtime borrow)
  is enforceable by construction and documented at the one entry point.

Other threads exist — timers, IO, decoders. They get a `WakeHandle`
(`Arc` of a mutex-protected job list + wake flag + waker callback):
`post(closure)` enqueues work that the UI thread executes at its next
`drain_posted()`, and fires the waker (a self-pipe write once the term
layer lands) so a blocked `poll` returns. The CLOSURE crosses threads;
the graph never does. This is the actor pattern: cheaper than locking
every read, and structurally deadlock-free.

`Scheduler` bridge: dirty effects queue in the runtime; frames are
requested via `request_frame()` (coalesced — one `FrameRequester` call
per taken frame) and consumed by the loop via `take_frame_request()`.
`FrameRequester` is defined locally in `reactive::scheduler` because
`src/anim` doesn't expose its clock traits yet; unification is filed in
`reviews/cycle1/react-requests.md`.

## 9. Layout: flexbox subset, integer-exact

`src/layout` is a pure solver: `LayoutTree` (arena of styled nodes +
measure callbacks for text leaves) and `solve(tree, root, rect)` which
assigns absolute `Rect`s. Direction row/column, justify
(start/center/end/space-between), align (start/center/end/stretch) with
per-child `align_self`, gap, padding, margin, grow/shrink/basis, fixed
and percent sizes (fractions of the parent's content box), min/max
clamps with freeze-and-redistribute resolution, and absolute positioning
against the padding box (insets; both-insets + auto size derives the
size).

Terminal constraint that shapes everything: **integer cells**. All
proportional distribution (grow shares, shrink shares, space-between
gaps) uses largest-remainder rounding — floor every share, hand leftover
cells to the largest fractional remainders, ties to the lowest index. So
3 children growing in width 10 get 4/3/3 and the sum is EXACTLY the
container: no lost or invented columns, deterministic across runs and
platforms (both properties test-pinned). Solved rects are compared on
assignment; geometry changes accumulate into a damage feed
(`take_geometry_damage`) so a moved sibling repaints even though its own
content never changed.

v1 non-goals (documented, not forgotten): wrapping, `auto` margins,
percent insets, baseline alignment. Intrinsic sizing of containers is a
single-pass content aggregation (sum main / max cross + padding + gaps)
— good enough for TUI cards and bars; measure-driven leaves are exact.

## 10. UI contract: views, Dyn regions, events, focus

`View` blueprints are inert descriptions: `Element` (style, draw
closure over a `Canvas` region, handlers, shortcuts, focusable flag,
children), `Text` leaf, and `Dyn` — the fine-grained re-render unit.
`UiTree::mount(cx, view)` instantiates blueprints into an instance arena
mirrored by layout nodes.

`Dyn` mechanics (the load-bearing part): mounting a `Dyn` creates one
effect. Each run disposes the previous generation's child scope (whose
cleanups remove the previous instances + layout nodes), evaluates the
build closure TRACKED (subscribing the region to exactly the signals it
read), mounts the result UNTRACKED, damages the region and requests a
frame. Unmount is not a special case: any ancestor scope disposal
cascades through the same cleanups. A signal that no view reads
re-renders nothing; a signal read by three `Dyn` regions re-renders
exactly three subtrees.

Event routing (W3C semantics adapted): target = hit test (mouse, by
solved rects, later siblings win) or focus (keys). Shortcuts resolve
first: the root→target path is walked collecting chord matches; the
DEEPEST registration wins (local overrides global), one action fires.
Then capture (root→target), target, bubble (target→root); handlers get
`EventCtx` (stop_propagation, request_focus, request_repaint) whose
mutations are applied by the tree after dispatch — handlers never hold
the tree borrow. Unconsumed Tab/Shift+Tab cycles focus in DFS order;
focus transitions synthesize FocusOut/FocusIn delivered target-only and
damage both rects (focus ring redraw).

Draw: pre-order walk; elements' draw closures paint their solved rects
through the `Canvas` trait (`BufferCanvas` for tests; RENDER's `Surface`
implements it when it lands — request filed). Text leaves print with
placeholder measurement (1 cell/char) until the text layer's measure fn
is exposed.

## 11. App shell (headless surfaces)

`App` wires the headless pipeline: `mount(component)` under a
`RootScope` (plus the theme watcher, §13), `pump()` (drain posted work →
flush effects → report frame request), `draw(canvas)`, `shutdown()`
(dispose root → cleanups unmount everything). These surfaces are shared
by tests and the real loop; `App::run` (cycle 2, §13) adds the terminal
session around them.

## 12a. Pinned dispatch semantics (RT1-3, option a — cycle 2)

`UiTree::dispatch` wraps the WHOLE routing pass (shortcut resolution,
capture, target, bubble, focus commands) in `reactive::batch`. Signal
writes made by handlers mark and queue but do not flush mid-route;
effects — and therefore `Dyn` disposal/remounting — run when the batch
closes, after routing completes. Consequences, all test-pinned
(`ui::tests::rt1_3_*`):

- Routing completes over the tree as it stood when the event arrived.
  Every handler that fires belongs to a then-live instance; a handler of
  a DISPOSED scope can never fire, because disposal cannot happen while
  routing is in progress.
- A capture handler closing a modal that contains the target does not
  yank the bubble path: the modal's own handlers still see that event
  (they were live when it was dispatched), and the modal is gone by the
  time `dispatch` returns.
- Event N+1 routes over the tree event N produced (per-event batches,
  not per-drain), so intra-turn ordering stays intuitive.

The rejected alternative (re-validate every instance at every step,
skip handlers whose scope died mid-route) was more surprising: whether a
sibling handler fired would depend on flush timing inside the same
event. The batch rule is one sentence and holds by construction.

## 12b. Draw purity (RT1-2) and runaway/worker diagnostics (RT1-15)

- The runtime carries a `draw_depth` flag set by `enter_draw_phase()`
  (a nesting-safe, panic-safe guard the ui layer holds during phase D).
  A TRACKED read while it is set — a `signal.get()` inside a draw
  closure — is the permanently-stale-pixels bug: debug builds panic
  naming the node (and its label if any); release builds count and keep
  the first few descriptions, surfaced by `reactive::diagnostics()`.
  Deliberate peeks use `get_untracked` (never guarded); a lazy memo
  pulled untracked during draw recomputes legitimately (its internal
  reads run under its own observer context and are not violations).
- Effects accept an optional creation label (`cx.effect_labeled`). Each
  flush stamps per-effect run counters; ONE effect exceeding 1k runs in
  a single flush panics naming its label — a write-feedback pair
  ("ping"/"pong") is diagnosed in milliseconds with a culprit, instead
  of the global 100k backstop's anonymous freeze (that backstop stays,
  as the collective-storm net).
- `reactive::spawn_worker(label, f)` wraps background threads in
  `catch_unwind`; a panic posts a labeled report that the app loop
  drains (`take_worker_failures`) and surfaces as an app error — a dead
  decode worker is a diagnosable failure, not silently-missing images.
  Posted jobs run ONLY on the UI thread at `drain_posted()` (which the
  loop calls only in phase U — the epoch rule's enforcement point).

## 13. The frame loop as built (cycle 2)

`app::Driver` implements the damage contract's phase sequence; `App::run`
is enter + `turn`/`wait` until quit + leave. Key decisions:

- **Non-blocking `turn`, blocking `wait_for_activity`**: one turn = U
  (drain posted -> dispatch each available event in its own batch ->
  flush) then, only if a frame is wanted (`take_frame_request()` or
  pending tree work), L (layout; geometry damage folds in), D (clear +
  redraw damaged regions through `SurfaceCanvas` into the root layer,
  under the draw guard, clipped per rect), C (`Compositor::flatten`),
  P (`FrameDiff::compute` -> `Presenter::emit` -> at most one
  `Terminal::write` + exactly one `flush`), S (`prev.blit(frame)`).
  Tests drive `turn` frame-by-frame against a scripted terminal; only
  the real loop ever blocks.
- **Idle = a blocking read**: no events, no damage, no frame request →
  the loop parks in `EventReader::poll_event(None)`. Cross-thread posted
  jobs and frame requests interrupt it through KERNEL's `TerminalWaker`
  (wired into `set_wake_callback` and the `base::FrameRequester`
  installed at driver start). Zero wakeups, zero CPU.
- **Startup caps (RT1-6)**: frame 1 paints with env-pass capabilities
  immediately. The active probe's queries are written at enter (never at
  a dumb terminal); replies ride the ordinary event stream as
  `CapsReply` events folded during phase U; on the DA1 sentinel the
  presenter caps upgrade, `prev` is poisoned and the root layer damaged,
  so the next frame re-presents everything at the new fidelity.
- **Resize**: surfaces resize, `prev` is POISONED (filled with an
  impossible color pair) because the terminal's post-resize content is
  unknowable — trusting a model of a screen that no longer exists is the
  self-sustaining-corruption failure mode. Full repaint follows.
- **Ctrl+C policy (KERNEL request 3)**: raw mode delivers Ctrl+C as an
  ordinary key. Default: quit, UNLESS the app consumed the event
  (handler or shortcut on the routing path). Programmatic quit:
  `App::quitter()`.
- **Theme (§5 of the damage contract)**: ONE app-level
  `Signal<&'static Theme>` (`app::use_theme/set_theme`), living under a
  deliberately-leaked per-thread root (process-lifetime state). Widgets
  resolve tokens at view build inside `Dyn` regions; `App::mount` also
  installs a watcher effect that invalidates the whole tree on switch so
  default-styled text (which resolves its color at draw) repaints too.
- **Damage translation (§3)**: the driver unions `UiTree::take_damage()`
  (Dyn remounts, focus) — into which `UiTree::layout()` already folded
  `LayoutTree::take_geometry_damage()` — clips to the viewport, drops
  contained duplicates, clears each rect to the theme background on the
  root layer and redraws only intersecting instances. Screen-cell
  coordinates end to end (root layer at origin).
- **Frame emission honesty**: a rendered frame whose diff finds nothing
  emits zero bytes and skips write/flush entirely. The acceptance test
  pins: initial full paint, single-cell emission for a counter change
  (~40 bytes: CUP + SGR + digit), zero bytes when idle, one flush per
  emitting frame.

## 15. The interactive layer (cycle 3)

### 15.1 Hover, pointer capture, wheel

- **Hover** is a PATH (root->deepest under the pointer, recomputed on
  every uncaptured mouse event); membership = hovered. On change, nodes
  leaving receive `MouseLeave` (deepest first), nodes entering receive
  `MouseEnter` (outermost first) — per-node target-only delivery, DOM
  `mouseenter` semantics. An ancestor is hovered while the pointer is
  anywhere in its subtree, so `Element::hover_signal(sig)` is the whole
  widget recipe (read the signal in a `Dyn` for hover visuals). Handlers
  hear these like any event: match on the event kind, never assume.
- **Pointer capture**: mouse DOWN captures its hit target; every mouse
  event routes there until UP releases (hover is frozen while captured;
  release-inside checks use rects, not hover). `EventCtx::
  capture_pointer/release_pointer` is the explicit form. A disposed
  capture target auto-releases; scrollbar drag is the canonical consumer.
- **Wheel** events route capture->target->bubble like any mouse event;
  a scroll container consumes them in its bubble handler — "nearest
  scrollable ancestor" IS bubble order, no extra machinery.

### 15.2 Focus

Tab/Shift-Tab cycle focusables in DFS pre-order with wrap; a
`focus_trap()` element constrains the cycle to its subtree while focus
is inside (modal pattern; traps gate TRAVERSAL — programmatic
`set_focus` crosses them). Clicking focuses the NEAREST FOCUSABLE
ANCESTOR-OR-SELF of the hit target (clicking a button's label focuses
the button); clicking non-focusable space changes nothing (keyboard
stays anchored). Disabled widgets simply aren't focusable — a
focused-disabled state cannot exist (style guide §3.2). Resolution
order for keys, documented on `dispatch`: handler phases first (a
focused input consumes its characters), THEN the shortcut table
(deepest-wins along the path), THEN built-in Tab traversal.

### 15.3 EventCtx geometry: `current_rect` vs `target_rect` (RT3-4)

`target_rect` is the EVENT TARGET's rect — under bubbling that is a
deep descendant, and mid-drag it is the captured node. Widgets doing
their OWN geometry (row under the pointer, scrollbar proportions, page
size) use `current_rect()` — the rect of the node whose handler is
running. RT3-4 was exactly this confusion: a scroll container clamping
its offset against a full-content-height child computed max-offset 0
and ate the wheel without scrolling.

### 15.4 Widgets (behavior set) + the reactive-style primitive

Button, TextInput, List, Scroll, Tabs, Table — one file each under
`src/widgets/`, builder props, tokens only (lint-enforced), visuals per
the binding style guide (theme-identity.md §3), all tested through the
REAL tree (`itest_util`: mount + dispatch + assert cells).

Two engine primitives carry them:

- `Element::style_signal(f)` — a reactive LAYOUT style: `f` re-runs
  (tracked) on signal change and re-applies to the live layout node
  WITHOUT remounting, so descendant state (focus, cursors) survives.
  Scroll = a clipped viewport over a once-mounted wrapper whose
  absolute insets are `(-offset_x, -offset_y)`; solved rects stay
  truthful, so hit testing and geometry damage keep working.
  Invalidation is INCREMENTAL (cycle 4): the change re-solves only its
  ANCHOR subtree — the nearest ancestor whose own size the change
  cannot affect (climb past Auto-sized ancestors; a Cells/Percent-sized
  one absorbs the change inside its fixed box). Measured on a
  2051-node tree, 100 style rounds: 104 µs incremental vs 84.4 ms
  whole-tree (~1 µs vs ~840 µs per change) — a 60 fps scroll drag pays
  for its container, not the screen. Structural changes (mount/
  viewport/theme) still full-solve; a full solve supersedes pending
  subtree work in the same frame.
- `layout::Style::clip_overflow` — draw clips children to the content
  box and hit testing refuses to descend outside it (scrolled-away
  content neither paints nor hits). Layout itself never clips: rects
  stay honest for ensure-visible math.

Data model honesty (v1): List/Table take snapshot rows and virtualize
PAINTING (only the visible window becomes cells; 10k rows cost a
screenful — REDTEAM-pinned); changing data = rebuild via a `Dyn` on the
caller's data signal. Keyed per-row `Dyn`s are the upgrade when per-row
reactivity is needed. TextInput editing is CLUSTER-ATOMIC as of cycle 4
(RT3-2 closed): cursor/selection/Backspace index grapheme clusters via
`text::segments`, and inserts re-anchor on the post-insert cluster map
because an inserted ZWJ or combining scalar can MERGE two clusters into
one (index arithmetic lies; the byte offset does not).

### 15.5 Signal transitions (`reactive::animate`)

`animate(cx, source, easing, duration)` returns a follower signal that
chases `source` through an `anim::Tween`, advanced once per frame by
the driver's frame-task pump (phase U). Each in-flight tick sets the
follower (`set_if_changed`) and re-requests a frame; the task list
empties on landing, so idle is truly idle. Retargeting mid-flight
restarts from the follower's current value. The loop paces in-flight
frames at ~16 ms via a deadline wait; input still wakes it early.

Cycle 4 adds ONE-SHOT TIMERS beside the frame tasks: `reactive::after
(delay, f)` runs `f` on the UI thread at deadline. Timers deliberately
do NOT frame-pace — the idle loop sleeps until `next_timer_deadline()`
and `run_due_timers` fires them in phase U, so a parked toast costs
zero wakeups until its dismissal is due.

### 15.6 Generation-scoped Dyn state (`dyn_view_scoped`, cycle 4)

`dyn_view(style, || view)` closures land created state on the MOUNTING
scope: one signal per rebuild ACCUMULATES until the whole Dyn unmounts.
That is correct for durable state (create it outside, bind it in) and
wrong for throwaway internals. `dyn_view_scoped(style, |gen_cx| view)`
passes the per-GENERATION scope — disposed right before the next
rebuild — so widget internals (hover, pressed, caret) die with their
generation. This is the two-scope discipline of the theme-rebuild
recipe made explicit: durable state on the mount scope, generation
state on `gen_cx`.

## 16. The overlay world (cycle 4)

`app::overlays` is the app-facing face of RENDER's compositor layers.
`App::overlays()` hands out a cloneable `Overlays`; each overlay is a
`render::Layer` (own surface, origin, z, opacity, blend, color
transform, cell shader) plus a CONTENT mode:

- **Manual** (`overlays.layer(z, bounds)`): paint via
  `LayerHandle::with_surface`.
- **Draw** (`layer_draw(z, bounds, closure)`): full-surface repaint
  when `damage()` is called; runs in phase D under the draw-purity
  guard, over a cleared transparent surface.
- **Tree** (`layer_tree(z, bounds, modal, cx, view)`): a full `UiTree`
  with its own reactivity, layout, focus and damage, drawn
  damage-scoped into the layer surface. Layer-local coordinates
  throughout.

`LayerHandle` (`Clone`, weak-backed — outliving the app degrades ops to
no-ops) exposes `set_offset/set_opacity/set_visible/set_blend/
set_color_transform/set_shader/set_shader_t/with_surface/damage/
remove`. Every mutation requests a frame; layer moves/fades record
frame damage inside `render::Layer`, so the compositor repaints exactly
the union of old+new bounds. Removing a layer damages the ROOT surface
under its last bounds — the vacated cells must repaint from below.

THE BORROW RULE: no user code runs while the overlay store is borrowed.
Phases steal a layer's surface (Vec swap), release the store, run draw
closures/tree draws against the stolen surface, swap it back. A
`LayerHandle` mutation from inside a draw closure hits `try_borrow_mut`
and is a debug panic (draw is pure; mutate from effects/handlers).

Driver phase integration: phase L lays out overlay trees; phase D draws
the root then overlay content; phase C flattens root+overlays z-sorted
(the root layer itself lives in the store at id 0); phase P diffs and
presents cells, then emits queued IMAGE payloads through
`Presenter::external_write` — cells first, protocol bytes second, ONE
flush. As of cycle 5 the present path is SCROLL-AWARE
(`FrameDiff::compute_scrolled` + `Presenter::emit_scrolled`: a
band-shift frame scrolls the terminal and repaints residuals — ~8-9x
fewer bytes on list/log workloads; declined detection is byte-identical
plain compute) and the compositor GROUND is the theme bg
(`Compositor::set_ground` per frame — additive light and translucent
veils blend against the theme instead of black).

Input routes topmost-z first: a MODAL tree overlay owns everything
while visible (keys, paste, and mouse anywhere); a non-modal tree owns
pointer events INSIDE its bounds (panels are opaque — no
click-through). KEYS (cycle 5): the topmost non-modal overlay tree
HOLDING FOCUS owns Key/Paste events — same opacity logic as the pointer
rule (a focused popup's Escape must not also scroll the app). Focus
lands in an overlay by clicking its focusable content (or
programmatically); a press that falls through to the ROOT clears every
non-modal overlay tree's focus — one focus story across trees: click
where you want your keys to go. No focused overlay = keys fall to the
root tree as always.

Image overlays (`overlays.image(rect, bitmap)`, GFX3D's seam) run
through `gfx::ImageSession` as of cycle 5 (RT4-1): each `ImageEntry`
carries a CONTENT `version` — `set_bitmap` bumps it (full retransmit),
`set_rect` does not (kitty re-places by id, a tiny `a=p` escape, no
pixel retransmission), removal retires the slot key and the driver's
next image pass RELEASES the terminal-side upload (`a=d` — the kitty
upload leak is closed), and `Driver::finish` releases every live slot
before leaving the alt screen (uploads outlive cell clears). Mosaic
output still blits cells into the root surface PRE-flatten; byte
payloads queue for the post-present bracket. Resize and caps upgrades
mark all images dirty (geometry and channel choice may change; the
session resets slots cleanly on a channel switch).

`app::popups::{Modal, Toast}` ride this API with no private privileges:
Modal = centered focus-trapped tree overlay on the `overlay` token
(z 1000); Toast = draw overlay chip (z 2000) that slides+fades via
`animate` driving `set_offset`/`set_opacity`, parks on a
`reactive::after` timer, slides out and removes itself. The acceptance
test pins the full arc: slide in over content, park at zero bytes,
dismiss, and an idle turn after teardown emits ZERO bytes. They live in
`app`, not `ui` — they need the overlay store, and `ui` sits below
`app` in the layer map (R4-1: no upward imports).

Popup call sites read the viewport from `app::use_viewport(cx)` (a
reactive `Signal<Size>` published by `App::set_viewport` on mount and
every resize) or `current_viewport()` untracked — no more hand-tracking
sizes through resize plumbing (DESIGN cycle-4 nit). Deterministic
timing for tests: `Driver::set_clock` injects the frame-loop clock
(animations, one-shot timers, tmux probe grace all read it) — the toast
acceptance runs on synthetic milliseconds, zero sleeps. (A `RunConfig`
field was considered and rejected: it breaks every struct literal in
foreign test files; a setter breaks nothing.)

## 17. Layout power: wrap, grid, overflow (cycle 6)

Three solver additions, same laws (integer cells, largest-remainder
rounding, purity):

- **Flex wrap** (`Style::wrap()`, `cross_gap`): lines break greedily on
  flex BASES (hypothetical main sizes — CSS semantics: two 6-basis
  children in a 10-wide row wrap even though shrink could fit them);
  each line distributes grow/shrink INDEPENDENTLY with the single-line
  math (a line is a row); Stretch children fill their LINE, not the
  container. Property-pinned: no overlaps, left-aligned lines, per-line
  exact tiling.
- **Grid** (`Style::grid(cols, rows)` / `widgets::Grid`): tracks are
  `Cells`/`Percent`/`Auto`/`Fr` — Auto fits the largest intrinsic size
  of children STARTING in the track; Fr shares the leftover
  largest-remainder (fr tracks tile EXACTLY, property-pinned across
  random extents/gaps/mixes). Children auto-place row-major with
  `col_span`/`row_span` (first-fit occupancy scan); rows beyond the
  spec are implicit Auto. Cell alignment: Stretch fills the cell area;
  explicit sizes / non-Stretch `align_self` size to content and align
  inside the cell (one alignment for both axes in v1 — a
  `justify_self` split is a recorded later decision).
- **Overflow** (`Overflow::Visible/Clip/Scroll` replacing the old
  clip bool): `Clip` = draw+hit clipping to the content box; `Scroll` =
  Clip PLUS the "this node scrolls" hint (wheel routing,
  ensure-visible, and the a11y `scrollarea` role ride it). Layout still
  never clips — solved rects stay truthful.

## 18. Shareable components + Callback (cycle 6)

The React-shaped contract, formalized in `ui::compose` (docs + the
tested Card example): a component is a PLAIN FUNCTION
`fn(Scope, Props) -> View`; props are a caller-built struct of data +
`Callback<T>` event fields + `View` slot fields. State lives in signals
on the passed scope; reactivity comes from `dyn_view` INSIDE the
component — parents never re-render children, there is no VDOM diff.
`Callback<T>` is the event currency: clone-cheap (`Rc<RefCell<FnMut>>`
— clones share ONE callback), `Default = noop` for optional events,
debug-loud on self-reentrancy. A component defined in one module
composes into any other by import; the Card test pins props, per-
instance event routing, slots, and the semantic labels of two
instances.

### 15.7 Interaction polish (cycle 7, second wave)

- **Mouse-move coalescing** (phase U): within one input batch, only the
  LAST of each consecutive run of plain Move events dispatches —
  intermediate hover positions were never visible (no frame rendered
  between them), so nothing observable is lost. Drag/Down/Up/Wheel
  NEVER coalesce (capture and click handlers see every one); any
  non-move event breaks the run (ordering with keys preserved). A
  raw-motion opt-out does not exist until a widget needs one.
  Measured (release, 1000-node tree, 10k dispatches): 7.6 ms moving
  (~0.76 µs/event), 4.7 ms stationary (the position memo), 1.7 ms
  downs — and a 10k-move burst now dispatches ONCE per frame.
- **Focus memory**: `Element::focus_memory()` containers remember their
  last-focused descendant; Tab RE-ENTERING from outside restores it
  (moving within the container stays pure tab order). Recorded on
  every focus change for all enclosing memory containers.
- **Initial focus**: `Element::autofocus()` (mount focuses it, last
  mounted wins, works through Dyn regenerations) or the explicit
  `UiTree::focus_first()` policy call. Nothing focuses automatically
  without one of the two — deliberate.
- **Spatial navigation**: `UiTree::focus_next_in(Key::Up/Down/Left/
  Right)` — nearest focusable in the direction's half-plane, scored
  `primary + 2x orthogonal` from rect centers (the TV-UI metric),
  trap-scoped, `focus_first` fallback when nothing is focused. Apps
  bind it to Alt+arrows via `app::actions` (acceptance-pinned through
  the real driver).
- **List hardening**: variable-height items (`item_heights` callback,
  prefix-sum windowing — offsets are CONTENT ROWS, item lookup is a
  binary search; v1 renders labels on the item's first row, extra rows
  reserve space), sticky selection by key (`key_fn` + `selection_key`:
  rebuilds re-find the key's new index, so data mutations keep the
  logical item selected), and `scroll_to` (a command signal, consumed
  after scrolling). One windowing code path — uniform lists are the
  identity prefix.
- **Startup notices**: `App::push_startup_notice` /
  `App::startup_notices()` — `App::run` records KERNEL's input-path
  degradation (`UnixTerminal::degraded`, readable only before type
  erasure) as `"input: degraded (<label>)"` plus a one-line caps
  summary (`"caps: truecolor+kitty-kbd+sync"`); `run_on` records the
  caps line. Apps render them however they like; the engine only
  collects.

### 18.1 Context + signals-as-store (cycle 7)

`Scope::provide_context(value)` / `Scope::use_context::<T>()` — typed,
scope-tree-scoped shared state (React context parity): descendants read
the nearest provided `T` (self first, then ancestors via the node
parent link that mirrors ownership); a nested provide SHADOWS its
subtree; provided values die with their scope (side-map entry removed
in the dispose walk, dropped outside the borrow). One value per TYPE
per scope; `T: Clone` — the currency is `Signal<T>` handles, `Rc<T>`,
or small structs OF signals.

The endorsed large-app pattern (documented with the cookbook in
`ui::compose`): a `Clone` store struct of signals provided at the root
— consumers `use_context::<AppStore>()` and mutate through signal
methods; actions are plain functions over the store; derived state is
`memo` chains. There is deliberately NO reducer framework and NO router
type: page switching is an enum signal + `dyn_view_scoped` (page-local
state dies on navigation), which `Tabs` already embodies — a router
earns its keep when a real app demonstrates history/deep-linking needs.

Ergonomics acceptance (cycle 7, both TESTED in `ui::compose`): a real
todo app — input, submit, selectable list, live count — in a 40-line
module (`sixty_line_app_proof_renders_and_reacts`), and one Card
component mounted into two separate apps with different props
(`one_component_reused_across_two_apps`).

RT6 risk closures (cycle 7): grid auto-placement switched to CSS-default
SPARSE packing (forward-only cursor — complexity bounded by occupancy
area, never children²; sparse = later children never backfill earlier
gaps, test-pinned); the Auto+span start-track approximation is pinned
by an exact boundary test (`auto_span_boundary_...` states the clipping
consequence); `Signal::try_get_untracked`/`is_alive` make disposed
reads answer `None` and the a11y snapshot guards value closures with an
unwind backstop (`"<stale>"`); the focus-visible hook flushes effects
EXPLICITLY around both draws — the synchronous-visuals contract stands
as design (a widget deferring focus visuals through timers fails the
check by design).

## 19. The semantic (a11y) model + keymap help (cycle 6)

HONESTY FIRST: this is in-engine infrastructure — roles, labels,
values, a snapshot, a text dump — NOT a screen-reader bridge (AT-SPI/
UIA/VoiceOver wiring is platform work, deliberately out of scope). It
is the model such a bridge would read, and the surface tests hold
widgets accountable against.

- `Element::role(Role)` / `access_label(..)` / `access_value(closure)`
  annotate views; interactive widgets ship defaults (button = its
  label; input = its value; list/table = count + selection; tabs =
  active title; checkbox on/off; radiogroup = selected item; modal =
  dialog; scroll = scrollarea).
- `UiTree::accessibility_tree()` (`a11y_tree()` alias) snapshots
  annotated nodes + text leaves, preorder, with focus and solved
  bounds; unannotated containers FLATTEN OUT (structure noise). The
  focused mark lands on the focused node's nearest ANNOTATED
  self-or-ancestor, so a focused inner leaf announces as its widget.
  `accessibility_tree_text()` is the assertable/debug dump;
  `focus_announcement()` derives the "what just got focus" line
  ("input \"query\" = \"teapots\"") for status bars and future speech.
- FOCUS-VISIBLE guarantee: `ui::focus_affordance_visible(&mut tree)` —
  renders with and without focus and demands a visible difference
  inside the focused rect (glyph, color, or attrs). Test-pinned for
  Button/TextInput; the hook is public so every widget suite (and
  REDTEAM) can hold the §3 rule.
- KEYMAP HELP: `Element::shortcut_labeled(chord, label, f)` carries
  descriptions; `UiTree::keymap_of_focus_path()` folds the focused
  node's + ancestors' shortcuts (the §12a resolution order);
  `app::KeymapHelp::open(...)` renders them + all `app::actions`
  chords in a modal — the '?' help screen for free. It lives app-side
  (rides Modal/overlays; the R4-1 layer rule, same as popups).

## 20. Known risks / attack surface (for REDTEAM)

1. **Track-read dedupe under nested pulls** (§4): the epoch fallback
   scan. A crafted graph interleaving nested memo pulls with repeated
   reads is the place to hunt for duplicate or missing edges (missing
   edge = missed update; duplicate = wasted marking, both observable).
2. **Effect-queue semantics under mid-flush disposal**: an effect
   disposing a LATER-queued effect must cause a stale skip (generation
   check), never a run against freed state; an effect disposing its own
   ANCESTOR while running is exercised only lightly.
3. **User-code depth**: framework traversals (marking, disposal,
   update_if_necessary's own walk) are iterative, but a memo reading a
   memo reading a memo... nests user closures on the native stack —
   dependency depth is bounded by stack size. 10k-deep CHAINS will
   overflow; 10k-wide FANOUT will not. Documented engine limit; if
   REDTEAM demonstrates a realistic UI hitting it, the fix is a
   trampoline in the memo-read path.
4. **Layout intrinsic sizing** recursion is also depth-bounded (tree
   depth, not graph size) and percent-inside-intrinsic is approximate.
5. **`update_if_necessary` vs mid-flush source-list mutation**: the
   revisit path re-reads `sources[idx]` fresh each step; disposal during
   a pull is guarded, but adversarial interleavings deserve tests.
6. **Overlay store borrow discipline** (§16): the surface-steal pattern
   assumes no path re-enters `draw_all` while a surface is stolen. A
   draw closure calling `LayerHandle` ops is caught (debug panic); a
   draw closure reaching `Overlays::dispatch` or creating layers is the
   interleaving to hunt.
7. **Incremental layout anchor** (§15): the climb-past-Auto rule
   assumes a fully-sized node's parent solve cannot change the
   GRANDPARENT's arithmetic. Percent-of-Auto and grow-basis edge cases
   are where a stale-rect counterexample would live; a found one
   demotes that change class to full solve, not a redesign.
8. **Same-position hover memo** (§15): correctness leans on
   `layout_epoch` bumping on EVERY geometry-changing path. A future
   mutation that moves rects without a `layout()` pass (direct rect
   surgery) would stale the memo; keep geometry writes inside layout.
9. **Grid auto-placement scan** (§17): first-fit over an occupancy
   matrix — a pathological span pattern (many wide spans in a narrow
   grid) is O(rows x cols) per child. Real UIs are tens of cells;
   a crafted 10k-child grid is the place to measure.
10. **Auto tracks + spans** (§17): a spanning child contributes
    `ceil(size/span)` to its START track only — a deliberate
    approximation; content can overflow when later spanned tracks are
    smaller. Documented, not defended.
11. **`access_value` closures run at snapshot time** (§19): a closure
    reading a DISPOSED signal panics the snapshot. Widget defaults
    capture their own signals (safe: same lifetime), but app-authored
    `access_value` capturing foreign state is the hazard to probe.
12. **`focus_affordance_visible` flushes via batch** (§19): the hook
    relies on `set_focus`'s internal dispatch running effects at batch
    end so focus-driven Dyn rebuilds land before the second draw. If a
    widget defers its focus visual through `after`/frame tasks, the
    hook reports a false negative — such a widget should be caught and
    redesigned (focus visuals must be synchronous).

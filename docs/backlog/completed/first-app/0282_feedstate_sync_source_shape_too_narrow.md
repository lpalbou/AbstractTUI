# Proposed: `FeedState::sync` source shape too narrow — fold-shaped stores cannot adopt

## Metadata
- Created: 2026-07-23
- Status: Completed (shipped as proposed — borrow-based source variant)
- Completed: 2026-07-23

## ADR status
- Governing ADRs: None. ADR impact: none — feed sync-bridge API surface.

## Context
0104 shipped `FeedState::sync(cx, items, SyncSpec)` with exactly the job
`abstractcode-tui`'s `wire_feed` does by hand (key/fingerprint/render/
visible closures, engine-owned rebuild policy, one-writer self-heal).
The 0104 completion report's acceptance line claims "wire_feed +
fingerprint + mirror test (~180 lines) become one `sync` call" — but the
claim does not survive contact with the first consumer's actual store
shape. Verified 2026-07-23 against the app's working tree: adoption
today means either a store restructure through its most contract-laden
type or a clone-mirror that copies the item vec on every fold write.
The app keeps `wire_feed` (~250 lines with the fingerprint + mirror
test) and files this instead.

## Current code reality
- `sync` demands `items: Signal<Vec<T>>` (feed_sync.rs:129-132).
- The app's sources are:
  1. `Signal<Fold>` — items are ONE FIELD of a larger state struct
     (stats/waits/flags mutated together under documented ordering
     contracts, store.rs). Splitting items out is surgery through the
     app's most invariant-laden type; a clone-mirror effect would copy
     the whole item vec on every fold write, including stats-only
     writes (Fold is one signal) — against the app's measured
     zero-copy grain.
  2. `Signal<Vec<EntityConvo>>` with a separate focus signal selecting
     ONE convo's NESTED `items` — the sync source is a focus-dependent
     projection, not any signal that exists.
- `Memo` does not coerce to `Signal`, so a derived-signal adapter
  doesn't exist either.
- `SyncSpec` keys derive from `&T` alone; the app's `Item` variants
  carry no identity (only `Item::Tool` has a `key`) — full adoption
  also needs a minted per-item seq (a mechanical but wide refactor
  across fold/convo construction), which is the app's own homework,
  named here only so the acceptance math is honest.

## Proposed direction (engine's call)
- A borrow-based source variant, e.g.
  `sync_with(cx, read_fn, spec)` where `read_fn: Fn(&mut dyn FnMut(&[T]))`
  (or a lens/`with`-style closure) — the bridge reads the items
  in-place through whatever reactive read the app performs inside the
  closure (`fold.with(|f| …)`, a focus-selected nested slice), zero
  copies, tracking whatever signals the closure touches.
- What the app gains beyond the deletion: the one-writer self-heal,
  the one-truth visibility closure (the app's mirror-drift pin test
  dissolves), and engine-owned rebuild policy.

## App-side workaround to delete when this lands
`abstractcode-tui src/ui/transcript_view.rs` — `wire_feed` (the sync
effect with positional `i{index}` keys + focus-dimension bookkeeping),
the FNV fingerprint (~100 lines), the `is_visible` mirror predicate,
and the mirror-drift pin test (~250 lines total), once item identities
exist app-side.

## Completion report (2026-07-23, 0.2.6 field wave)

Shipped the proposed borrow-based source variant, exactly the
`read_fn: Fn(&mut dyn FnMut(&[T]))` shape:

- `FeedState::sync_with(cx, source, spec)` (`widgets/feed_sync.rs`) —
  the source closure runs INSIDE the sync effect and hands the current
  items to the callback borrowed in place (zero copies): a fold-shaped
  store reads `fold.with(|f| read(&f.items))`; a focus-selected
  projection reads `focus.get()` then the nested slice. Every signal
  the closure touches becomes a dependency of the sync effect — the
  drain re-runs on any of them, and unchanged items cost exactly one
  fingerprint compare (a stats-only fold write renders NOTHING,
  test-pinned). Contract text covers the exactly-once callback rule
  (zero calls skip the drain; multiple calls drain sequentially).
- ONE drain core, never duplicated: the diff body moved verbatim into
  a private `drain_into` (fast prefix path, rebuild policy, C-1
  one-writer self-heal, C-2/C-3 doc contracts untouched), and
  **`sync` now delegates to `sync_with`** over the whole-signal read —
  so the entire pre-existing `feed_sync_tests` suite (append-only
  never rebuilds, parity bar, both self-heal shapes) pins the shared
  core through the new door. All 12 pass unchanged.
- The minted-seq identity homework stays APP-SIDE as the item names it
  (keys must be real identities; nothing engine-side can mint them) —
  no engine recipe was added beyond the existing key-uniqueness
  contract on `SyncSpec::new`.

Tests (`widgets/feed_sync_with_tests.rs`, child module of the sync
suite sharing its rig):
- `fold_shaped_source_syncs_and_stats_only_writes_render_nothing` —
  the 0282 evidence shape: items one field beside stats under ONE
  signal; stats-only writes leave the render counter untouched
  (fingerprint path), appends through the fold stay O(1), fingerprint
  bumps update in place.
- `focus_switched_nested_source_rebuilds_on_focus_and_stays_fast_within`
  — two-signal projection: focus switch rebuilds to the other convo,
  background writes to the unfocused convo render nothing, the focused
  convo keeps the fast path on both sides of the switch.
- `self_heal_still_fires_through_the_borrow_door` — a manual `push`
  between drains is detected and healed by the next fold write; fast
  paths resume after the heal.

Docs: `docs/api.md` feed-sync section (one-line `sync_with` note),
CHANGELOG under `[Unreleased]`. Gates: whole-tree tests green, clippy
clean, semver-additive vs 0.2.6 (new method only).

# Proposed: `FeedState::sync` source shape too narrow — fold-shaped stores cannot adopt

## Metadata
- Created: 2026-07-23
- Status: Proposed (API gap report — first-app finding, 0.2.6 adoption wave)
- Completed: N/A

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

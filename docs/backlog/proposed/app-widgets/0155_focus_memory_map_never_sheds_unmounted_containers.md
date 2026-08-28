# 0155 — `TreeCore::focus_memory` never sheds an unmounted container (one dead entry per `Dyn` rebuild)

## Metadata
- Created: 2026-08-21
- Status: Proposed (engine-seat finding, measured — not a field report)
- Track: app-widgets (band note below — this is an engine-core
  `src/ui/` defect and there is no engine-core band; app-widgets has
  already absorbed the wave-12 engine findings 0135/0175/0185, so it
  absorbs this one too. **The missing band is itself worth a ledger
  decision** — see "Ledger gap".)
- Severity: P3 (unbounded growth, bounded harm — nothing is unsound
  and nothing renders wrong; a long-lived app pays memory for every
  rebuild it ever did)
- Engine: abstracttui 0.4.1 (unreleased tree)

## The defect

`TreeCore::focus_memory: HashMap<ViewId, ViewId>` records the
last-focused descendant of every `Element::focus_memory` container, so
tabbing back into a pane restores where you were.

`remove_subtree` (`src/ui/mount.rs:278`) is the only unmount path. It
detaches from the parent, removes the layout subtree, drains the
instance arena, and clears `core.focus` when the focused node dies —
and it walks past `core.focus_memory` entirely. Nothing else prunes
it.

So every rebuild of a `Dyn` region containing a memory container adds
a permanent entry keyed by a `ViewId` that no longer exists.

## Measured, not reasoned

`src/ui/mod_tests.rs::focus_memory_sheds_containers_that_unmount`
(`#[ignore]`d, because it asserts the behaviour the map should have):
a `dyn_view` rebuilding a `.focus_memory()` row 20 times, focusing
inside each generation, leaves

```
entries = 20      dead (container no longer in the arena) = 20
```

Both halves are asserted — `dead == 0` is the property, and
`entries <= 1` pins that the survivor is the LIVE container rather
than an empty map trivially satisfying the first.

**Falsified both ways.** Red today for the stated reason. Injecting
the prune into `remove_subtree`'s drain loop —

```rust
c.focus_memory.remove(&id);
c.focus_memory.retain(|_, v| *v != id);
```

— turns it green. A check that could not pass would be decoration;
this one distinguishes the two states.

## What it is NOT

**Not unsound.** The arena is generational
(`src/reactive/arena.rs`), so a stale `ViewId` resolves to `None`
rather than to a reused slot's occupant, and
`restore_memory_target` (`src/ui/focus.rs:195`) already re-validates:
it checks the remembered node is alive AND still inside the container
before honouring it, and falls through to `entering` otherwise. The
read path was hardened; the write path was never balanced.

**Not a leak of the values either way round.** Both the key
(container) and the value (remembered descendant) go stale together,
which is why the fix needs both lines: `remove` for the container,
`retain` for entries whose remembered descendant died while the
container lived.

## Why it surfaced now

Found while designing field-agora **0910** (`Scroll` ensure-visible),
which needs its own map in `TreeCore` — caller key → `ViewId` — and
the design question was *"who removes an entry when the subtree
unmounts?"*. The only precedent in the file answers "nobody", and it
answers it silently.

That answer changed 0910's design: its map is keyed by the caller's
**String**, not by `ViewId`, so a `Dyn` rebuild re-registers the same
key and overwrites in place. Its size is bounded by the number of
distinct keys the app uses rather than by the number of rebuilds —
the property `focus_memory` does not have.

## The fix

The two lines above, in `remove_subtree`'s existing drain loop (it
already visits every dying instance, so this costs one hash lookup
per removed node and no extra walk). The `retain` is the linear half;
if that ever matters, invert the map or store the container on the
`Inst`.

Do it with the ignored test un-ignored in the same change.

## Ledger gap

There is no band for engine-core findings the engine seat makes
against itself. Every findings band is a *consumer* feedback band
(`first-app`, `field-agora`, `field-gateway`, `field-core`), and
`wave11` is a closed audit wave. Internal findings have been landing
in `app-widgets` by default since wave 12 (0135/0175/0185), which
works but makes the track's description untrue. Worth either renaming
the track or opening an `engine-core` band at the next free fifty.

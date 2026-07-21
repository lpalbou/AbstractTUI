# REACT cycle-5 requests

## To RENDER

1. **Both one-liners adopted** (your §3): `set_ground(Some(theme.bg))`
   rides every render_frame (theme switches flow through the existing
   damage_all contract), and phase P is `compute_scrolled` +
   `emit_scrolled` unconditionally. The type-level token pairing made
   the adoption a genuinely two-line diff — nice design. LayerStack
   retirement acknowledged; the u64 registry is now the one layer
   store on record, and your generational-id design stays recoverable
   from this file's history if id reuse ever becomes real.
2. **Scrolled detection under overlays** (data ask, no code): my §8
   risk — with a toast/modal layer over a scrolling band, the
   flattened frame's damage usually stops being one clean band. If you
   have appetite, a workload number for "list scrolls under a corner
   toast" would tell us whether edge-trimming already saves it or the
   decline is total. Pure curiosity for the perf story; correctness is
   safe either way.

## To GFX3D

3. **RT4-1 done end-to-end** per your sketch: version field
   (set_bitmap-only bump), tombstoned release, finish-time
   release_all, BufSink through the post-present bracket. Your
   mid-cycle CORRECT verdict noted — thanks for reviewing against the
   live tree. My acceptance asserts the `a=p`-not-`a=T` move path at
   the driver level as you suggested.
4. **Live-terminal smoke** (your cycle-4 honesty note): the byte-level
   story is now pinned twice (your session tests + my driver
   acceptance) but still never exercised against a REAL kitty/tmux.
   When you run the first live smoke, the driver side is ready — say
   the word and I'll pair on any delete-then-retransmit ordering
   quirks that surface.

## To DESIGN

5. **All three cycle-5 asks landed** (your request 1): `use_viewport
   (cx)`/`current_viewport()`, `List/Table::focus_signal`, and D4-1
   headers (`text_muted`, sorted column in full `text` per your
   suggestion). Dashboard can switch: one signal swap + two
   `.focus_signal` lines. Ping me in cycle 6 if the contrast re-audit
   wants a different sorted-column treatment.
6. **Non-modal popups now have a key story** (§16): focused overlay
   owns keys; outside-press returns them. If the dashboard grows a
   non-modal palette/dropdown, this is the contract to build against —
   tell me if the "outside press clears overlay focus" rule fights any
   interaction you have planned.

## To KERNEL

7. **Both adoptions in**: `poll_many` is the driver's phase-U drain
   (burst semantics exactly as documented — `Ok(0)` = service the
   loop), and the probe upgraded to `for_caps` + `full_query_bytes`
   with TMUX_GRACE handling via a one-shot timer wake. One design
   note: I finalize the grace in phase U off a `reactive::after` wake
   instead of a poll deadline — if you ever see a stuck probe on a
   waker-less terminal, that's the seam to look at (scripted tests
   have no waker; production terminals all do).
8. `MouseEvent::pixel` still ignored in the ui conversion (your
   cycle-4 note 3) — deliberate until an image widget wants sub-cell
   precision; flag when GFX3D's viewport wants it and I'll surface it
   on `ui::MouseEvent`.

## To REDTEAM

9. **Your `image_session_lifecycle_no_leaks` placeholder** is still
   ignored+`unreachable!` — the session API landed this cycle
   (`gfx::ImageSession`), so it's writable now against your KittyModel
   (create→show→move: transmit_count stays 1→resize: retransmit
   allowed, old id freed→drop: live_data_ids empty next frame). My
   driver-level acceptance covers the driver half; yours pins the
   session contract itself.
10. **New surfaces this cycle**: (a) the non-modal key-ownership rule
    (`Overlays::dispatch`) — focus-steal interleavings (two non-modal
    overlays, focus in the LOWER one, keys must still route there);
    (b) `Driver::set_clock` — a clock that goes BACKWARD is undefined
    territory (Tween clamps, timers just re-arm; nothing panics, but
    worth an adversarial pass); (c) retired-image tombstones vs a
    same-frame re-register reusing a rect (release-then-transmit
    ordering is pinned in my acceptance — a same-KEY reuse cannot
    happen since ids are never reused, but a same-RECT new image the
    same frame is the case to poke); (d) the probe grace path with a
    CapsReply arriving AFTER finalization (folded nowhere — dropped
    honestly; is that observable?).

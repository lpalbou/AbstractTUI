# 0299 — Public full-redraw verb (poison-prev semantics) + optional focus-regain repaint

Status: proposed (field evidence from abstractcode-tui, 2026-07-23)
Owner: engine (app/driver)
Effort: S

Renumbered from 0300 (wave-3 CLOSER, 2026-07-23): the original id sat
inside control-plane's band (0300–0390) and collided with
control-plane/0300 (app lifecycle events) in the global numbering —
the same collision class 0292/0294 were renumbered for. first-app's
band is 0220–0299.

## The field failure

The damage contract ("repaint only damaged regions; idle emits zero
bytes") trusts the terminal to keep every cell the engine ever painted.
When that assumption breaks EXTERNALLY — Cmd+K in Terminal.app,
`printf '\033c'` from a stray process, an emulator glitch — nothing
heals, ever:

- The engine's `prev` frame still models the old content, so any
  client-side repaint that produces byte-identical cells emits NOTHING
  (`FrameDiff` correctly suppresses equal cells).
- `UiTree::invalidator()` / `EventCtx::request_repaint()` damage the
  MODEL only — they cannot re-emit cells the model believes unchanged.
- The only paths that re-sync the terminal are resize and caps-upgrade,
  because only they call the driver-private `poison_prev()` +
  `Presenter::invalidate()` pair.

Live consequence (abstractcode-tui, maintainer-reported with a
screenshot): one external clear + one passing toast left the header
blank FOREVER — the toast's vacated rect re-emitted only the session-id
columns; the rest of the bar stayed blank because model == prev there.
A wiped 271×68 screen mid-run recovered only the spinner glyph and two
elapsed digits after 20 seconds. The Python predecessor bound Ctrl+L →
full repaint; a damage-tracked engine needs the equivalent verb.

## Ask 1 (the verb)

A public, component-reachable "the terminal content is unknown" verb —
exactly what `apply_resize` already does, minus the geometry change:

```rust
// e.g. abstracttui::app::request_full_redraw()
// driver side, next turn:
self.poison_prev();
self.presenter.invalidate();
for layer in store.layers.iter_mut() { layer.surface_mut().damage_all(); }
for img in store.images.iter_mut() { img.dirty = true; }
reactive::request_frame();
```

Component-reachable matters: apps only hold `Overlays`/signals after
`App::run()` consumes the `App`. A thread-local flag drained by the
driver's turn (the `request_frame` pattern) fits.

## Ask 2 (optional, kills the class silently)

Full redraw on `Event::FocusGained` (DEC 1004 — already parsed, currently
dropped in `convert_event`). An externally-cleared terminal is nearly
always followed by a focus round-trip before the user looks again;
auto-heal on focus-in would make the failure invisible without any
keybinding. Cost: one full-frame emission per focus-in (bounded, human-
paced). Could be a `RunConfig` opt-in if the default feels too eager.

## Client-side workaround shipped meanwhile (abstractcode-tui)

v2 (2026-07-23, live-verified in a pty against a real run): Ctrl+L +
`/redraw` + a 5s run-heartbeat paint a one-tick TRANSLUCENT VEIL layer —
glyphless cells (`Cell::EMPTY.with_bg`), bg black at alpha 2 — then
remove it next tick. Why that exact shape: a space fill is opaque
content that ERASES glyphs for a frame (visible blink at heartbeat
cadence); a glyphless translucent bg keeps glyphs and veils inks, and
black@2's integer source-over drops every color channel ≥ 1 by at least
one (`floor(253·d/255) < d` for `d ≥ 1`) — so every cell carrying
visible ink differs from the model in frame 1 and from the veil in
frame 2, and both frames re-emit it. Cells it cannot change are
all-channels-zero (black-on-black: invisible either way).

Measured limits that keep this a WORKAROUND, not the fix:

- The automatic heartbeat must confine itself to the CHROME BAND
  (header row + bottom 3 rows): a full-frame heal every 5s would decay
  iTerm2/sixel image placements (beneath-repaints, `overlays.rs`) and
  re-emit the whole frame twice per beat. The transcript pane therefore
  stays wiped until content streams or Ctrl+L — pty-proven honest gap.
- The veil cannot `Presenter::invalidate()`: post-wipe, the presenter's
  virtual cursor and pen are ghosts (the 0298 class); the first heal
  frame can misplace one relative-motion run / emit under a stale SGR
  base until the frame trailer re-syncs. Self-corrects within a frame;
  the verb's invalidate half fixes it exactly.
- Negative proof for "just re-render harder" (pty, live run, 120×36):
  after an external wipe, ~25 header dyn re-runs over 3 seconds
  re-emitted NOTHING for the static cells (t+3s frame: 35/36 rows
  blank; only the spinner/elapsed cells — the ones whose VALUES change
  — returned). Model-side damage cannot heal a terminal-side loss.

## Related (same family)

The composer-placeholder gap is now its own item — **0291**
(placeholder-while-focused opt-in; renumbered from 0310, same
band-collision class as this file). abstractcode-tui currently overlays
its own hint (absolute element, content-derived height so it never eats
caret clicks).

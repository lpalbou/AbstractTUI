# 0299 — Public full-redraw verb (poison-prev semantics) + optional focus-regain repaint

Status: completed 2026-07-23 (0.2.6 field wave)
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

## Completion report (2026-07-23, 0.2.6 field wave)

- **Ask 1 shipped as asked**: `app::request_full_redraw()` (new
  `src/app/redraw.rs`; re-exported in the prelude) — a thread-local
  request flag in the `mouse_capture()` drain shape, exactly the
  component-reachable form the item names (apps hold no driver after
  `App::run`). The driver drains it once per turn in the phase-U
  engine-verb section, BEFORE the frame decision, so a request from
  this turn's own key handler renders — and re-emits everything — the
  SAME turn. The drain target is the existing
  `Driver::resync_unknown_screen` (the I-2 suspend pair): prev poison
  + `Presenter::invalidate()` + damage-all on every layer + image
  re-place + `request_frame()`. Idle after the healing frame is zero
  bytes again (test-pinned).
- **Ask 2 shipped as an OPT-IN**:
  `app::set_redraw_on_focus_gained(true)` (+ `redraw_on_focus_gained()`
  getter). The driver now handles `Event::FocusGained` — previously
  always dropped — with the same resync when the policy is on. Default
  OFF, the item's own hedge, for two reasons: (a) a full-frame emission
  per focus-in is real byte cost under tmux pane-switching cadence and
  would put an asterisk on "idle emits zero bytes" for every existing
  session; (b) the item's `RunConfig` suggestion is unavailable
  additively — `RunConfig` is a literal-constructible pub-field struct,
  so a new field is semver-MAJOR (`constructible_struct_adds_field`);
  the thread-local policy verb carries the opt-in instead.
- **The image half went one layer deeper than the item's sketch**:
  `img.dirty = true` alone does NOT re-place — `ImageSession::sync`
  answers `Unchanged` for an unmoved same-version slot (the resize
  path only re-emits because rects change). `resync_unknown_screen`
  now forgets terminal-side image state per channel: kitty slots
  `release` (delete bytes + full retransmit — the delete is harmless
  where the upload is already gone and mandatory where it survived,
  per the session's kitty no-forget rule), iTerm2/sixel/mosaic slots
  `invalidate_slot`. This also FIXES suspend-resume, which previously
  restored every cell but silently lost every protocol image (the
  suspend tests asserted text only).
- **Tests**: `tests/wave_redraw.rs` —
  `ctrl_l_full_redraw_re_emits_every_cell_then_idles` (end-to-end: the
  app binds Ctrl+L → verb via `Element::shortcut`, `0x0c` through the
  wire; BYTE EVIDENCE: the verb frame's bytes alone, fed to a fresh
  `VtScreen`, rebuild every row — a diff frame against a live model
  would leave a fresh screen blank; then 4 idle turns at zero bytes),
  `full_redraw_re_places_byte_channel_images` (kitty `\x1b_G` re-emits
  on the verb; parked turns stay silent), and
  `redraw_on_focus_gained_is_opt_in` (default: focus round-trip emits
  nothing; opted in: focus-in re-presents the whole screen, then back
  to zero). Unit pins in `app::redraw::tests` (one-shot drain,
  default-off policy). The suspend wave (`wave_inputav`) stays green
  over the shared resync.
- **Docs**: api.md gained a full-redraw section beside the other
  app-runtime verb sections (the `redraw` module itself stays private —
  the verbs export at `app::` + prelude; no public types to house);
  CHANGELOG under Unreleased; the consumer upgrade prompt now retires
  the veil/heal workaround.
- Whole-tree battery: 1,662 tests green, 0 failed (1,212 lib + 403
  across 54 integration suites + 47 doctests; 96 ignored =
  perf/soak/live-pty/fuzz gates + doc fragments — the 0.2.5 baseline
  1,654 plus exactly this wave's 8 new tests), clippy `--all-targets
  -- -D warnings` zero, fmt clean, alloc pins green (alloc_budget 10/10),
  `cargo semver-checks --baseline-version 0.2.5` — 196 checks pass,
  additive-clean at the 0.2.6 minor bump — and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean.

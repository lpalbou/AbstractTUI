# REACT — cycle 2 cross-module requests

Author: REACT. Status notes on cycle-1 requests, plus new seams from the
loop build. The loop is LIVE (acceptance test in `src/app/acceptance.rs`
runs a real App through CaptureTerm + VtScreen); everything below is
refinement, nothing blocks.

## Resolved from cycle 1 (thanks)

- `base::FrameRequester` adopted; my local trait is deleted
  (`reactive::scheduler` re-exports base's for compat).
- `TerminalWaker` + `TermRead::Wake` + defaulted trait methods: wired
  exactly as designed — `set_wake_callback` and the frame requester both
  wake the blocking read. The "poll_event returns None for wake AND
  deadline" choice composes perfectly with a drain-then-decide loop.
- `CaptureTerm: Terminal` + `VtScreen`: the acceptance test drives the
  REAL driver through them; `flush_count`/`take_bytes`/`to_text` were
  exactly the assertion surface needed. The idle-storm guard caught a
  real busy-poll bug in an early draft of my drain loop — good trap.
- `text::width/cluster_width`: ui text measurement now routes through
  the one width authority (placeholder deleted).
- Theme registry (`get/default_theme`, `'static` themes): the theme
  signal carries `&'static Theme` as designed (DESIGN request 5 done:
  `app::use_theme/set_theme`, one signal, damage contract §5).

## To KERNEL

1. **Undercurl + underline-color caps fields.** `render::PresentCaps`
   grew `undercurl` and `underline_color`, but `term::Capabilities` has
   no corresponding fields, so the app maps both to `false`
   (`app::driver::present_caps_from`). Detection is yours per RENDER's
   cycle-1 request 2; when the fields land, the mapping is a two-line
   change on my side.
2. **`From<&term::Capabilities> for render::PresentCaps`** (or a method):
   `present_caps_from` in `app/driver.rs` is the hand-assembly RENDER
   asked you to make unnecessary. Happy to delete mine once either side
   owns it.
3. **FYI — probe driving**: the app does NOT use `input::probe_active`
   (blocking); it writes `ActiveProbe::query_bytes()` at enter and folds
   `CapsReply` events from the ordinary stream during phase U, upgrading
   presenter caps on the DA1 sentinel (RT1-6a: first paint never waits).
   If the probe protocol grows steps, keep `ActiveProbe` sans-IO like
   today and this keeps working.

## To RENDER

4. **`Surface` implementing `ui::Canvas` is no longer needed** — the app
   bridges through `ui::SurfaceCanvas` (thin adapter: ui alpha-0 =
   inherit maps to `Style` patch `None`; `draw_text` handles width). If
   you'd still rather own a direct impl, adopt the same color
   convention and I'll delete the adapter.
5. **FYI — pipeline usage**: driver per frame = `flatten` → `compute` →
   `emit` → one write + one flush → `prev.blit(frame)`, per your recipe.
   Caps upgrades and resizes poison `prev` (impossible color pair) to
   force full re-emission — if `Surface` ever grows a cheaper
   "invalidate diff baseline" primitive, the two `poison_prev` call
   sites in `app/driver.rs` are the customers.

## To DESIGN

6. **Theme signal is live**: `app::use_theme(cx) -> Signal<&'static
   Theme>`, `set_theme`, `set_theme_by_id`; default `abstract-dark`.
   Switch = one signal write; `Dyn` regions that read it re-render, and
   an app-level watcher damages the whole tree (covers default-styled
   text). The themes_gallery acceptance path (Enter swaps theme, screen
   restyles through normal reactivity) is unblocked.
7. **Default text color** is the active theme's `Text` token; the damage
   clear uses the `Bg` token. Widgets with opinions resolve tokens at
   view build (your request 1 contract) — nothing widget-facing changed.

## To REDTEAM

8. RT1-2 / RT1-3 / RT1-15 are implemented with the exact semantics your
   findings demanded; the pinning tests exist on my side
   (`ui::tests::rt1_3_*`, `ui::tests::rt1_2_*`,
   `reactive::tests::{draw_phase_*, runaway_*, spawned_worker_*}`) —
   your independent attack versions are welcome, the contracts are
   documented in reactive-ui.md §12a/§12b.
9. The RT1-1 test you promised (posted-job write during present repaints
   NEXT frame, no lost/double damage) is structurally impossible to fail
   now (posted jobs only run in phase U), but the test is still worth
   having — `Driver::turn` + `CaptureTerm` gives you the harness.

## To the integrator

10. Prelude proposal now has a concrete shape: `reactive::{create_root,
    batch, untrack, on_cleanup, Scope, Signal, Memo, Effect}`,
    `layout::Style`, `ui::{Element, text, dyn_view, UiTree, Key,
    KeyChord}`, `app::{App, RunConfig, use_theme, set_theme}`. The
    <60-line ergonomics app is writable against that set today.

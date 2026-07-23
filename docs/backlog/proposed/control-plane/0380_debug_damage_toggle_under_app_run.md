# 0380 — `Compositor::set_debug_damage` is unreachable under `App::run`; the damage visualizer needs an app-level knob

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review,
  `reviews/wave12/visual-to-code-handoff.md` §5; accepted by the code
  seat — `reviews/wave12/code-to-visual-handoff.md` "#5: a `RunConfig`
  knob is the honest shape")
- Track: control-plane (observability tooling — the 0370
  screenshot-exporters precedent: observe primitives live in this band)
- Class: API gap (a shipped diagnostic with no reachable switch for
  app authors)
- Severity: P3 — docs were corrected to be honest meanwhile, so
  nothing lies; but the "see the minimal-damage proof" tool the design
  docs advertise is embedder-only
- Engine: verified on 0.2.22 (2026-07-25)

## The evidence

faq.md/troubleshooting.md originally promised the damage visualizer to
app authors; the wave-12 review found the Driver owns the Compositor
privately and exposes no toggle — no `RunConfig` field, no env knob
(the only engine env var is `ABSTRACTTUI_NO_SPLASH`,
`src/boot/identity.rs:60`). The docs were fixed the same wave to say
so honestly: "under `App::run` the driver owns the compositor, so
there is no app-level toggle yet" (docs/faq.md "How do I see what is
actually repainting?", docs/troubleshooting.md). This item is the
"if you want the promise back" half.

## Current code reality (verified 2026-07-25, 0.2.22)

- `src/render/compositor.rs:113-118` — `set_debug_damage(bool)` exists,
  runtime-switchable, documented as a diagnostic mode (bytes change;
  never for golden tests). Test-covered
  (`compositor_tests.rs:466-530`).
- The Driver constructs and owns its Compositor; no public accessor,
  no `RunConfig` field reaches it. `App::run` users cannot flip it.
- The code seat's sizing note: "driver is deliberately last in the
  0990 split queue — good moment to add the knob when it's touched"
  (`src/app/driver.rs` is the last remaining wave11/0990 split at
  1115 lines).

## What we want

An app-level switch to the existing compositor diagnostic:

- A `RunConfig` knob (the code seat's named honest shape) — e.g.
  `debug_damage: bool` — threaded to the driver-owned compositor at
  construction. NOTE the 0299 lesson before choosing the exact shape:
  `RunConfig` is a literal-constructible struct, so ADDING A FIELD is
  semver-major — the 0299 wave rejected a RunConfig field for exactly
  that reason and shipped a verb (`app::set_redraw_on_focus_gained`)
  instead. A setter verb or builder method is the additive-safe
  spelling; an env var (`ABSTRACTTUI_DEBUG_DAMAGE`) is the
  zero-API-cost alternative and matches how a developer actually uses
  a visualizer (flip it on a run, not in code). Ruling in-item.
- Docs regain the app-author promise in faq.md/troubleshooting.md
  (both currently carry the honest "embedders only" caveat that this
  item deletes).

## Validation

- A CaptureTerm driver test: with the knob on, a damaged region's
  outline cells appear in the emitted frame under `App::run`-shaped
  driving; with it off, bytes are identical to today.
- Golden suites untouched (diagnostic stays out of goldens — the
  compositor doc's own rule).
- Zero cost when off (idle pins unaffected).

## Non-goals

- New visualizer features (per-layer tinting, damage logging) — the
  switch reaches the EXISTING diagnostic, nothing more.
- The 0310 automation-bus observe verbs (screenshot/semantic tree) —
  different surface, already covered by 0370/0310.

## Related

- Completed control-plane 0370 (screenshot capture — the band's
  observe precedent), wave11 0990 (driver.rs split — the stated
  landing moment), completed first-app 0299 (the RunConfig-vs-verb
  additive lesson), first-app 0240 #3 (the debug-notice class this
  visualizer complements).

# 0291 — TextArea: opt-in placeholder while focused-and-empty

Status: completed 2026-07-23 (0.2.6 field wave)
Owner: engine (widgets/textarea)
Effort: S

Renumbered from 0310 (wave-3 CLOSER, 2026-07-23): the original id
collided with control-plane/0310 (automation bus) and sat outside
first-app's band (0220–0299) — the same collision class 0292/0294 and
0299 were renumbered for.

## The field failure

`TextArea` paints its placeholder only when the field is empty **and
unfocused** (`widgets/textarea.rs` draw guard: `text.is_empty() &&
!focused`). Any app whose primary input `.autofocus()`es — every
chat/composer-shaped app, which is the flagship transcript use case —
therefore NEVER renders its placeholder: the field is focused from boot
and refocused after every modal close.

Live consequence (abstractcode-tui 0.3.0): the phase-aware composer
teaching ("describe a task — Enter sends · Ctrl+J newline · /help",
"Enter steers the run · …") was designed, built, state-tested, and
**never painted one pixel** — the composer rendered as two side strokes
around a blank line (review-current-state.md §4.2, per-cell ink dump).
The unfocused-only rule matches classic form UX (placeholder yields to
the caret), but a terminal composer is a single always-focused field —
the GUI convention every toolkit ships today (VS Code, browsers'
`::placeholder`, iTerm2 command palette) paints placeholders while
focused and empty, beside the caret.

## Ask

An opt-in, default-off so existing apps are byte-identical:

```rust
TextArea::new(cx)
    .placeholder("describe a task — Enter sends")
    .placeholder_while_focused(true) // NEW
```

Draw rule when enabled and `text.is_empty() && focused`: paint the
placeholder starting one cell PAST the caret cell (the caret block must
stay visible — "where am I typing" beats one hint word), same
`placeholder_fg` ink as the unfocused path.

## Client workaround shipped meanwhile (retire on landing)

abstractcode-tui overlays its own hint element (absolute inset `left:3`,
content-derived height so a non-empty draft's caret clicks are never
intercepted; renders only while `value().get().is_empty() &&
focused().get()`; the engine's own path still covers the unfocused
state, so exactly one renderer paints in each state). It works but is
~40 lines of app code, per app, per composer — and it reads TextArea
reactive internals (`value()`/`focused()`) that a widget option would
encapsulate.

## Completion report (2026-07-23, 0.2.6 field wave)

- **Shipped exactly the asked shape, on BOTH editors**:
  `TextArea::placeholder_while_focused(bool)` and — the same guard
  existed verbatim in the sibling (`input.rs` draw:
  `text.is_empty() && !focused`) —
  `TextInput::placeholder_while_focused(bool)`. Draw rule when enabled
  and focused-and-empty: the hint paints starting ONE CELL PAST the
  caret cell (`tx + 1`) in the same `placeholder_fg` (`text_faint`)
  ink; the caret block itself paints through the normal path over
  column 0, so it stays visible beside the ink — "where am I typing"
  beats one hint word, per the ask. A `tw > 1` guard skips the hint in
  the degenerate one-column field (only the caret cell exists there).
- **Default decision: stays OFF** (the item's own ask, kept after
  weighing the flip). The modern convention argument is real, but two
  things pin the default: the crate is published (0.2.x) and a silent
  flip would repaint every focused-empty field in every existing app —
  and the one known field consumer ships its OWN focused-state overlay,
  which would double-paint under a flipped default until upgraded.
  Back-compat honesty wins; the convention is one builder call away. A
  default flip is 0.3-breaking-budget material if ever ruled
  (planned/0002's territory), not a 0.2.x patch.
- **Tests** (all cell-level, colors included):
  - `widgets::textarea::tests::placeholder_while_focused_paints_beside_the_caret`
    — caret blank at column 0 with `cursor`-token bg, hint from column
    1 of the text area in `text_faint`, one typed char hides it and
    the caret advances;
  - `widgets::input::tests::placeholder_while_focused_paints_beside_the_caret`
    — TextInput parity, same assertions;
  - `wave_composer::autofocused_composer_paints_placeholder_beside_caret_on_screen`
    — the exact field shape through the REAL frame loop: an
    `.autofocus()`ed composer under `Driver::turn` + `CaptureTerm`,
    VtScreen cell assertions (caret cell `paint.bg == cursor`, hint
    cell `paint.fg == text_faint`), typing `h` through the wire hides
    the hint, `unknown_seq_count == 0`;
  - the default-off back-compat pins stand unchanged
    (`placeholder_disabled_and_a11y`,
    `placeholder_shows_only_unfocused_empty`: focused-empty without
    the opt-in paints NO placeholder).
- **Docs**: api.md TextArea section teaches the opt-in + the default
  rationale; CHANGELOG under Unreleased; the consumer upgrade prompt
  (`reviews/abstractcode-tui-v3-upgrade-prompt.md`) now tells the app
  to delete its overlay workaround.
- Whole-tree battery: 1,662 tests green, 0 failed (1,212 lib + 403
  across 54 integration suites + 47 doctests; 96 ignored =
  perf/soak/live-pty/fuzz gates + doc fragments — the 0.2.5 baseline
  1,654 plus exactly this wave's 8 new tests), clippy `--all-targets
  -- -D warnings` zero, fmt clean, alloc pins green (alloc_budget 10/10),
  `cargo semver-checks --baseline-version 0.2.5` — 196 checks pass,
  additive-clean at the 0.2.6 minor bump — and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean.

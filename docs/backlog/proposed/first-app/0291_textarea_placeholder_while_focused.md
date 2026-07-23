# 0291 — TextArea: opt-in placeholder while focused-and-empty

Status: proposed (field evidence from abstractcode-tui, 2026-07-23)
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

# 0120 — TextArea: multiline composer with history + completion anchor

## Metadata
- Created: 2026-07-21
- Status: Planned
- Track: app-widgets
- Completed: N/A

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: None expected (new widget; `TextInput` unchanged).

## Context
The bluntest line of the cycle-11 critique: the crate "ships only a
single-line text input". A multiline composer is the input half of every
interactive app class — chat and feeds (message bodies are paragraphs;
one-line messages are a non-starter), consoles and REPLs (multi-row
prompts, history recall, `/command` and `@file` completion), and any
form- or note-taking surface. Both reviews list it P0/P1 (completeness
§2b P0-3; robustness Part 2 P1-1); the two port composers — the coding
console's prompt box (`abstractcode/fullscreen_ui.py`) and the chat
client's markdown composer (agora bodies up to 64 KB) — are the first
validators and supply the reference semantics below.

## Current code reality
- `src/widgets/input.rs:1-16` — `TextInput` is single-line **by design**
  ("TextInput: single-line editable text field"). What it already solved
  and this item must reuse, not re-derive: cluster-atomic editing
  (`ClusterMap`, input.rs:44-96, over `text::segments` — one cursor stop
  per grapheme cluster, widths from the same authority as rendering),
  selection via anchor+cursor, word jumps, whole-`Paste` insertion (never
  per-char synthesis), horizontal scroll keeping the caret visible.
- `src/app/overlays.rs:158,212` — `Overlays::layer` + `on_outside_press`
  exist: a completion dropdown is buildable as an overlay **if** the
  composer exposes where the caret is on screen. Nothing exposes a caret
  cell today.
- `docs/faq.md:164-166` — Ctrl+Enter and Shift+Enter do not exist on the
  classic terminal wire; the kitty keyboard protocol disambiguates where
  supported (the engine already decodes it). Any newline-vs-submit design
  must be honest about legacy terminals.
- Target semantics worth matching, read from the Python console:
  `abstractcode/fullscreen_ui.py:144-163` (`arrow_nav_action`) — Up moves
  within the buffer first, jumps to text start, and only then recalls
  history; Down mirrors it; an empty buffer goes straight to history.
  Completion: fullscreen_ui.py:56-120 curates a `/`-command list whose
  first completion screen is deliberately the app's face; `@` file
  mentions complete from workspace files.

## Problem
There is no multiline editing surface at all: no vertical caret movement,
no logical-line model, no grow-to-content, no history recall, no caret
anchor for a dropdown. Building a serious composer app-side means
re-deriving cluster math the engine already owns.

## What we want
A `TextArea` widget:
1. **Multiline model**: logical lines with soft wrap at the widget width;
   caret moves by cluster horizontally (reuse `ClusterMap`) and by visual
   row vertically with a remembered goal column; Home/End per visual row,
   Ctrl+Home/End for the document.
2. **Grow-to-content** up to a `max_rows` cap, then internal scroll.
3. **Submit vs newline policy** owned by the app via the builder:
   Enter-submits + Alt+Enter-inserts-newline as the default preset
   (works on every wire), with Shift+Enter additionally inserting where
   the kitty protocol reports it. Never advertise chords the wire cannot
   carry (faq.md:164).
4. **History recall** with the row-boundary semantics of
   `arrow_nav_action`: arrows navigate the buffer first and reach for
   history only at the edges; recalled entries replace the buffer; the
   in-progress draft survives a history round-trip.
5. **Block paste**: a bracketed `Paste` event inserts whole, newlines
   included (the input layer already delivers it whole and neutralized).
6. **Caret cell exposure**: a signal (or query) yielding the caret's
   screen cell so a completion dropdown can anchor an overlay at it.
7. **Completion dropdown**: v1 as a documented recipe/example over
   `Overlays::layer` + `on_outside_press` + the caret cell (trigger
   prefixes like `/` and `@` are app policy); promote to a packaged
   widget only if both ports end up copying the same code.

## Scope / Non-goals
Scope: the widget, history, caret anchor, the dropdown recipe + example.
Non-goals: IME composition (same posture as `TextInput`, input.rs:13-15 —
composed input arrives as the terminal sends it); syntax highlighting
inside the composer; undo stacks beyond a single draft-restore;
readline/vi emulation modes.

## Expected outcomes
Both port epics get their composer from the engine; the completion
dropdown for `/` and `@` is an afternoon of app code, not a widget fork.

## Validation
- Unit: caret math over multi-cluster content (ZWJ emoji, combining
  marks) in both axes; goal-column persistence; history edge semantics
  (port the `arrow_nav_action` decision table as cases).
- CaptureTerm acceptance: type/wrap/grow to cap, paste with newlines,
  submit vs newline chords (kitty and legacy input bytes), dropdown
  anchored at the caret and dismissed by outside press.

## Progress checklist
- [ ] Multiline buffer + caret/goal-column model over ClusterMap
- [ ] Grow-to-cap + internal scroll
- [ ] Submit/newline policy presets (legacy-honest)
- [ ] History recall (edge-triggered, draft-preserving)
- [ ] Caret cell exposure + dropdown recipe/example
- [ ] Acceptance + cluster-math tests

## Field evidence (2026-07-21, first app)
`abstractcode-tui`'s composer is a single-line `TextInput`; multi-line task
prompts (the norm for coding agents) must be written elsewhere and pasted —
the paste path folds newlines to spaces (src/widgets/input.rs paste arm), so
structure is lost. A real composer is the app's top missing input feature.

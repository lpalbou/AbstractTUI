# Proposed: typed uppercase inserts lowercase on kitty-spelling wires — `convert_event` drops the kitty `text` field

## Metadata
- Created: 2026-07-23 (found during the 0286/0288 verification)
- Status: Proposed

## ADR status
- Governing ADRs: None. Routing-vocabulary seam (`ui::KeyEvent` has no
  text slot); a fix may touch the documented `convert_event` drop
  list.

## Context
0286 claimed "Text INPUT is immune (the kitty `text` field carries
\"A\")". Source verification during the choice-fix wave found that
claim FALSE for this engine:

- The kitty parser DOES decode associated text into
  `input::KeyEvent::text` (input/kitty.rs — but note the engine's
  standard enter flags are `DISAMBIGUATE | REPORT_EVENT_TYPES`; the
  associated-text progressive enhancement (0b16) is NOT pushed, so
  most terminals will not even send the text field).
- `convert_event` (app/events.rs) DROPS `text` — the routing
  `ui::KeyEvent` has no text slot (a documented seam drop).
- `TextInput` inserts the base char for `Key::Char(ch)` when
  `!ctrl && !alt` (widgets/input.rs) — SHIFT-carrying chars insert
  the char AS-IS.

Consequence: on any wire that spells Shift+A as `CSI 97;2u`
(`Char('a')` + SHIFT — the exact spelling the first app's maintainer
terminal produced, per 0286's live P0 and its regression-test bytes),
a focused `TextInput` inserts **'a'** where the user typed **'A'**.
Legacy wires are unaffected (shift arrives baked into the char).

The 0286/0288 fix (the shifted-letter MATCHING fold,
`KeyEvent::normalized`) deliberately does not touch this: text input
is a different contract than shortcut matching (the fold is
match-site-local by design; folding inside `convert_event` was the
option 0286 rejected pending a consumer audit).

## Repro sketch
Open any TextInput (e.g. ChoicePrompt's Other editor), feed
`\x1b[97;2u` through the Driver (CaptureTerm), read the value: 'a',
expected 'A'. Same class for shifted symbols on layouts where the
terminal reports base + SHIFT (`CSI 47;2u` for '?' would insert '/').

## Candidate directions (engine's call)
1. Thread the kitty `text` field through the seam: an optional text
   slot on `ui::KeyEvent` (or a separate routed event) that text
   widgets prefer over the key identity when present. Honest general
   fix (covers shifted symbols and non-trivial layouts); needs the
   associated-text enter flag (0b16) pushed, plus the seam-drop
   documentation updated — and a decision about repeat/release echo.
2. Case-fold at the INPUT site: `TextInput` (and Composer etc.)
   uppercases `Char(cased lowercase)` when SHIFT is reported —
   `fold_shifted_letter` reused at the insertion boundary. Cheap,
   covers letters only (symbols stay wrong), no wire change.
3. Both: (2) now as the letter stopgap, (1) as the real fix.

## Notes
- Whether mainstream terminals send `CSI u` for shift+letter under
  DISAMBIGUATE alone varies by implementation; the maintainer's
  terminal demonstrably does (0286's live P0). Any terminal that does
  hits this defect today.
- CAPS LOCK typing has an adjacent variant (locks are stripped for
  chord matching; a caps-locked 'a' arrives as `Char('a')` + CAPS_LOCK
  and inserts 'a') — same seam, worth deciding together.

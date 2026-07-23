# Proposed: ChoicePrompt aux-key vocabulary — no non-option key surface, and the hint row is closed to callers

## Metadata
- Created: 2026-07-23 (split out of 0271 at its completion — the
  0271 wave shipped gaps 1 and 3 (`body_width`, `dismiss_label`,
  `handle.retire`); THIS gap was recorded there and is deliberately
  not shipped yet. The ask below is 0271's gap-2 text, verbatim.)
- Status: Proposed
- ID note: filed into the 0271-0279 gap beside its parent
  (0290-0299 used, 0300+ is control-plane's); chronology reads from
  the Created date, not the id.

## ADR status
- Governing ADRs: None. ChoicePrompt key routing
  (choice_prompt_interact.rs `root_key_handler`) + hint row
  (`hint_segments`, choice_prompt_parts.rs).

## The ask (0271 gap 2, verbatim)

`root_key_handler` (choice_prompt_interact.rs) consumes movement,
Enter, non-dismissable Esc, declared option letters, Space (multiple)
and digit jumps; an UNMATCHED letter returns unconsumed — and inside
the focus-trapped Modal there is no caller-reachable node that could
host a tree shortcut for it (the body sits OFF the focus path;
content root and panel are engine-built). The approval consumer binds
`f` = toggle cards↔full-JSON on its hand-rolled modal. `KeyState::
pressed_chord` could observe `f` (the pre-routing tap works inside
modals), but `hint_segments` is hardcoded ("a/A/d pick · Enter
confirms · Esc cancels") so the key cannot be ADVERTISED — a hidden
key on a consent surface fails the discoverability bar the engine
itself set (0250's "movement is not activation" was about legible
vocabulary). Ask: `.aux_key(chord, label, handler)` — matched after
option letters, consumed, rendered into the hint row.

## Notes at split time (0271 wave, 2026-07-23)

- The 0271 completion shipped the hint row's FIRST caller-fed segment
  (`dismiss_label` feeds `esc_segment` → `hint_segments` takes the
  ready segment text) — an `aux_key` hint segment would ride the same
  shape: segments degrade whole-segment from the front, so an aux
  segment should sit before the Enter/Esc tail.
- Design questions an implementation must answer (recorded, not
  decided): where aux chords sit in the match order relative to
  declared option letters and digit jumps (the filing says after
  letters); whether an aux handler may resolve the gate or only
  mutate caller state (the consumer's `f` toggle only flips a
  signal); how aux keys interact with a focused Other editor (the
  shield rule says typing wins).
- Consumer workaround until then: `KeyState::pressed_chord` observes
  the key inside the modal, minus advertisement.

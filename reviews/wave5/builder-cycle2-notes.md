# Wave 5 — BUILDER cycle-2 notes (findings folded, skeleton readiness)

BUILDER document, 2026-07-23. Companion to the cycle-2 fold on
`ChoicePrompt` (see the amended completion report in
docs/backlog/completed/app-kits/0515_choice_prompt_decision_gate.md).
Your file (`tests/wave_choice_review.rs`) was NOT touched — this note
is the activation map for cycle 3.

## Findings disposition (F1–F7)

- **F1 (a11y) — FOLDED.** The tree now carries: `heading "<prompt>"`
  (full text — pixels wrap/ellipsize, the label does not), `menu
  "options" = "<current label>"` (one tab stop), per-option rows as
  `menuitem "<label>"` (+ `= "selected"` on the highlighted row) in
  single mode / `checkbox "<label>" = "on"|"off"` in multiple mode,
  and the revealed Other editor as `input … [focused]` (TextInput's
  own role/value; absent before engagement — no phantom). Buttons are
  `button`. No `Generic` on any interactive part. Pinned:
  `a11y_tree_names_question_options_and_selection_state`,
  `a11y_multiple_mode_reports_checkbox_state`.
  A5: the region's focus is VISIBLE — focused = the audited selection
  pair on the highlighted row; unfocused = accent ink (the RadioGroup
  focus precedent). `focus_affordance_visible` asserted on the region
  AND on a focused button
  (`region_focus_affordance_visible_and_unfocused_highlight_distinct`).
- **F2 (shortcut letters) — FOLDED.** `ChoiceOption::key(char)` +
  `ChoicePrompt::option_key(id, label, char)` (+ `option_with(opt)`
  for detail+key+danger combinations). CASE-SENSITIVE ('a' ≠ 'A' —
  your consumer's vocabulary; SHIFT-carrying uppercase accepted for
  kitty-class reporters). Letter = EXPLICIT activation:
  select+commit in single mode, jump-toggle in multiple. Rendered as
  a dim `(a)` after the label (reserved before label truncation);
  named in the hint ("a/A/d pick", the first segment to degrade).
  Collision rules: a focused Other editor consumes printables first
  (letters type, never activate — your O2 shield); a declared key
  outranks the digit-jump lane (pinned: '2' as a declared key commits
  instead of moving). Duplicate declarations are a debug_assert.
- **F3 (must-choose) — FOLDED.** `ChoicePrompt::dismissable(false)`:
  no Cancel button, no advertised Esc (a dead key in the hint or
  KeymapHelp would lie), Esc REFUSES visibly — the hint row shows
  "an answer is required" (accent ink), cleared by the next
  movement/toggle. `handle.cancel()` and the degenerate-open paths
  still resolve `Cancelled` (dismissability governs the USER's
  endings, never the flow's guarantee of an outcome — your G2).
  Documented for destructive gates; demoed in examples/decide.rs
  gate 1.
- **F4 (layered Esc) — FOLDED, position conceded.** Your Combobox-
  precedent + reflex-protection argument wins over my cycle-1 "one
  predictable ending": Esc while the Other editor is focused now
  RETREATS to the list (editor blurs, engagement + draft kept, in
  both modes); the second Esc cancels (dismissable) or refuses
  visibly (must-choose). The hint tells the truth while editing:
  "Enter confirms · Esc back to the list". Pinned:
  `esc_in_other_retreats_first_keeps_draft_then_second_esc_cancels`,
  `esc_in_other_retreats_then_refuses_on_a_must_choose_gate`.
- **F5 (draft persistence) — was already true, now SPEC'D.** The
  draft signal lives in the MODAL scope (outside the row views):
  survives excursions and retreats within one gate lifetime; a fresh
  instance starts empty (state dies with the modal scope — G6).
  Pinned: `other_retreat_hides_input_and_keeps_typed_text` (cycle 1).
- **F6 (key discoverability) — FOLDED.** The hint row names the
  actual keys: "[letters pick ·] [Space toggles ·] Enter confirms
  [· Esc cancels]" — segments drop whole from the FRONT under width
  pressure (the tail survives longest); the editing state swaps to
  the layered-Esc truth; must-choose lists no Esc.
- **F7 (danger tint) — FOLDED.** `ChoiceOption::danger(true)` /
  `ChoicePrompt::danger(id)`: glyph+label ink rides the `Error`
  token. One deliberate exception, on the record: while the row is
  highlighted AND the list focused it wears the audited SELECTION
  PAIR — error-on-selection-ground is not a contrast-audited
  combination, the pair is; the row was visibly dangerous before the
  highlight arrived. Unfocused highlight keeps the error ink (glyph
  carries the accent). Pinned:
  `danger_option_wears_error_ink_except_under_the_selection_pair`.

## Per-skeleton readiness (all 16 — none blocked)

API anchors: `ChoicePrompt::new(prompt)` builder →
`.open(cx) -> ChoicePromptHandle`; outcomes
`ChoiceOutcome::{Answered(ChoiceAnswer), Cancelled}`;
`ChoiceAnswer { selected: Vec<id>, other: Option<String> }`. Esc wire
byte in wave tests: `\x1b[27u`.

| Skeleton | Ready | Notes for activation |
| --- | --- | --- |
| g1 click+Enter one batch | YES | click-on-selected commits; the resolve fn is exactly-once (`Cell<bool>`); my wave twin pins stray-Enter-after-close |
| g2 Esc explicit outcome | YES | default (dismissable) gates only — `Cancelled` through `on_resolve` |
| g3 must-choose refusal | YES | `.dismissable(false)`; refusal note text: "an answer is required"; clears on next movement |
| g4 resolve disposes scope | YES | pinned cycle 1 (unit + wave); all commit paths funnel through one resolve; modal closes BEFORE the callback |
| g6 reopen + chain | YES | pinned cycle 1 (`on_resolve_may_open_the_next_prompt…`, `ChoiceSequence` wave test) |
| g7 stable identity | YES | ids only, canonicalized to option order |
| o1 no phantom input | YES | a11y has no `input` entry pre-engagement; `input … [focused]` after |
| o2 typing routes | YES | chars/Backspace/Left/Right/Home/End consumed by the editor; Up/Down bubble to the list |
| o3 draft survives | YES | excursion AND Esc-retreat variants pinned |
| o4 empty-Other refusal | YES | note: "«label» needs text — type your answer" (accent) |
| a3 roles honest | YES | vocabulary above; multiple mode uses `checkbox` rows, not `menuitem` |
| k2 keys in hint | YES | assert at panel width ≥ the full hint (segments degrade by design at narrow width — see below) |
| k3 first key works | YES | focus_init lands on the region; initial `●` rendered from frame one (the fabricated-selection law: an honest visible default) |
| s1 30 options | YES | windowing follows the highlight; `i/N` position note right-aligned |
| s2 prompt wraps | YES | `text::wrap` + ellipsis cap at viewport/3 rows |
| s3 fixed rows survive | YES | region carries explicit `min_h(1)` + `shrink(1)`; fixed rows keep the 0240 floor |

## Caveats your assertions should know

1. **Hint degradation is by design**: segments drop whole from the
   front when the panel is narrow ("a/A/d pick" goes first, "Esc
   cancels" last). K2 assertions should use a viewport wide enough
   for the full string, or assert the tail.
2. **The `1-9` asymmetry is a decision (your F9)**: single mode
   digits MOVE only (movement vocabulary); multiple mode digits
   jump+toggle (the mark is the selection act); declared LETTERS are
   the activation vocabulary and outrank a digit declared as a key.
   Documented in the api.md section.
3. **Value vocabulary**: `= "selected"` (single, highlighted row),
   `= "on"|"off"` (multiple), region `menu "options" = "<label>"`.
4. **No conflicts remain**: G3 is satisfied (knob shipped); K4 is
   folded (retreat) — the cycle-1 "Esc always cancels" position is
   withdrawn on the record.

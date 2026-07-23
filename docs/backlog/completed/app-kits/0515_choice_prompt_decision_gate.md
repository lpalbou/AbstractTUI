# 0515 — ChoicePrompt: the modal decision gate (question + options + Other)

## Metadata
- Created: 2026-07-23
- Status: Completed 2026-07-23 (full scope incl. the ChoiceSequence
  stretch goal; see the completion report at the end)
- Track: app-kits
- Completed: 2026-07-23
- Depends on: nothing new — 0500 shipped every ingredient (Modal
  focus-trap + `focus_init`, the 0250 movement-vs-activation
  vocabulary, the 0297 disposal-safety law, TextInput
  `placeholder_while_focused`, Button, theme tokens). Engine deltas:
  NONE (pure composition, app-side).
- Validator: `examples/decide.rs` (headless exit-0) + REVIEWER's
  acceptance suite `tests/wave_choice_review.rs` (wave 5, adversarial
  lane) against this item's contract.
- Promotion trigger: filed at build start per the wave-5 brief; moves
  to completed/ with the completion report when the gates pass.

## ADR status
- Governing ADRs: none in this repo. ADR impact: none expected — new
  component family member, zero engine deltas, all additions
  semver-additive vs 0.2.7.

## Context — the maintainer's brief (verbatim intent)
"we need a nice widget/component (if not already here) to ask
questions with multiple choices + other category. think when we need
to gate a decision behind that component."

I.e. an app must be able to BLOCK a flow on a structured question — a
prompt, N options, optionally multiple answers, optionally an "Other"
free-text choice — and get the answer(s) back through a callback.
Agent-approval dialogs, setup choices, destructive-action
confirmations with alternatives. `abstractcode-tui` already hand-rolls
approval prompts; every agent console will need this on day one.

## Current code reality (survey)
- **No question/decision component exists.** The select family (0500)
  solves "pick a value for a field" — anchored to a trigger the user
  already focused. A decision gate has no anchor: it interrupts.
  `Modal` (src/app/popups.rs) is the interruption primitive
  (focus-trapped, input-owning, centered; `layer_tree(modal=true)`
  runs `focus_init` so keys are live from frame one — the 0230 fix).
- **`KeymapHelp::open` is the action-component precedent** (a modal
  opened by a verb, closed from inside via a shortcut — synchronous
  close-from-inside-dispatch is engine-supported; `Overlays::dispatch`
  snapshots layer handles before user handlers run, so close + open
  next from inside a handler is safe).
- **The 0250/0297 laws bind here hardest**: movement is not
  activation; commit fires once; ALL widget bookkeeping lands before
  the user callback so `on_resolve` may dispose everything including
  the prompt itself.

## Specification (v1)

### Model
- `ChoiceOption { id, label, detail: Option<String> }` — `detail`
  renders as its own muted row under the label (a decision's detail
  must not vanish on narrow widths like a right-aligned hint would).
- `ChoiceQuestion { prompt, options, allow_multiple, other:
  Option<String> }` — a plain public data struct: approval questions
  arrive as DATA (from an agent/server); the builder is sugar over it.
- `ChoiceAnswer { selected: Vec<String> /* option ids, canonicalized
  to option order — the MultiSelect precedent */, other:
  Option<String> /* trimmed, non-empty when present */ }`.
- `ChoiceOutcome::{ Answered(ChoiceAnswer), Cancelled }`.
- Naming: `Choice` prefix throughout (`SelectOption`/
  `CompletionCandidate` precedent); the bare word `Question` stays out
  of the prelude (downstream collision hazard).

### The gate
`ChoicePrompt::new(prompt).option(id, label).option_detail(..)
.allow_multiple(..).allow_other(label).initial(id).checked(ids)
.max_visible(rows).overlays(&o).on_resolve(f).open(cx)
-> ChoicePromptHandle`.

- Opens a `Modal` over everything immediately (overlays from reactive
  context, explicit `.overlays(..)` for tests — the select family's
  resolution rule). Viewport via `use_viewport`.
- **Exactly-once, never silent**: every ending — Enter-commit,
  click-commit, Confirm/Cancel buttons, Esc, programmatic
  `handle.cancel()` — funnels through one resolve fn gated by a
  `Cell<bool>`; the modal closes BEFORE the callback runs (0297), so
  `on_resolve` may dispose the opener's scope or open the next prompt.
- **Outside-press does NOT dismiss** (decision-gate policy: Modal
  swallows it; a gate has explicit endings only — Esc and Cancel
  always exist).
- Degenerate opens resolve `Cancelled` immediately rather than hang
  the flow (no overlay store; zero options with no Other), with a
  debug_assert naming the mistake.
- Re-openable by construction: each open is a fresh modal scope.

### Interaction contract
Single mode (`allow_multiple = false`): the highlight IS the candidate
(`●` rides it); Up/Down/Home/End/wheel move it; `1-9` jump; Enter (or
click on the already-selected row — click-on-selected commits, the
0250 mouse ruling; first click only selects) commits
`Answered{selected: [id]}`. A Cancel button is the mouse escape hatch.

Multiple mode: highlight and checked set (`☑`/`☐`) are separate; Space
or click toggles; `1-9` jump+toggle; Enter or the Confirm button
commits the canonicalized set (an EMPTY set is a legal answer — the
component is a gate, not a validator; callers wanting an explicit
"none" add the option). Confirm + Cancel buttons.

Other: last row (`allow_other(label)`); engaging it (highlight in
single mode, checked in multiple) reveals an inline TextInput —
autofocused, `placeholder_while_focused` — whose own key consumption
shields the list keys (digits/space type text; Up/Down/Esc bubble
through; Enter submits = commit). Commit with Other engaged and
empty-trimmed text REFUSES (visible note in the hint row) — a gate
never resolves a hollow "other". The input row's space is reserved at
open so revealing it never resizes the modal.

Esc ALWAYS cancels the whole prompt (also from inside the Other input
— one predictable ending, documented).

### Aesthetics
Modal `overlay` ground; prompt wraps (`text::wrap`, bold `text`);
option rows: glyph (`●`/`○` single, `☑`/`☐` multiple) + label,
selection pair on the highlighted row, muted detail row, faint
truncation ellipses; long lists window around the highlight (the
select family's proven pattern) with an `i/N` position note in the
muted hint row; buttons right-aligned; tokens only (RT1-9b).

### Sequential flow (stretch, shipped)
`ChoiceSequence::new(vec![q1, q2]).on_resolve(f).open(cx)` — each
question opens as the previous resolves (recursive open-in-resolve;
safe because resolve closes before the callback);
`ChoiceSequenceOutcome::{ Completed(Vec<ChoiceAnswer>), Cancelled {
index, answers } }`. Empty question list completes immediately
(synchronous callback, documented).

### Non-goals (v1, recorded)
- Disabled options (the select family has them; a gate's alternatives
  are normally all live — additive later if a consumer demands it).
- Custom Confirm/Cancel button labels; per-option shortcuts rendered
  in rows; type-ahead. All additive.
- A required-minimum-selection validator (callers gate on the answer).

## Validation plan
Unit (`src/app/choice_prompt_tests.rs`, real UiTree + overlay store,
select-rig pattern) + wave acceptance (`tests/wave_choice_prompt.rs`,
real Driver/CaptureTerm wire-in modeled-VT-out): full keyboard
round-trip single/multiple/other/cancel; disposal-in-resolve;
exactly-once under every ending; outside-press non-dismissal; 20-option
windowing; narrow-width honesty; sequence order + cancel index; demo
`examples/decide.rs` headless exit-0.

## Completion report (2026-07-23, BUILDER — wave 5 cycle 1)

**Shipped** (all semver-ADDITIVE vs 0.2.7; `cargo semver-checks
--baseline-version 0.2.7`: 196 pass, "no semver update required"):

- `src/app/choice_prompt.rs` (468 lines) — model types
  (`ChoiceOption`/`ChoiceQuestion`/`ChoiceAnswer`/`ChoiceOutcome`),
  the `ChoicePrompt` builder + `open(cx) -> ChoicePromptHandle`, the
  exactly-once resolve machinery, and `ChoiceSequence` (+
  `ChoiceSequenceOutcome`). Exports appended to `app::mod` and the
  prelude.
- `src/app/choice_prompt_view.rs` (599) — the modal content tree +
  interaction contract (`#[path]` sibling).
- `src/app/choice_prompt_parts.rs` (233) — geometry (`measure`),
  variable-height windowing (`window_start`), the row renderer
  (`choice_row`), hint segments (sibling split for the file budget).
- `examples/decide.rs` (136) — three gate flavors behind keys 1/2/3;
  headless exit-0 (tty guard).
- docs/api.md `## app::ChoicePrompt — the modal decision gate`;
  CHANGELOG under `## [Unreleased]`.

**Design decisions on the record**:
1. **Modal, not anchored popup**: a gate interrupts (no anchor);
   `ChoicePrompt::open` is an ACTION like `KeymapHelp::open`.
2. **Exactly-once via `Cell<bool>` + FnOnce take**: every ending
   (Enter/click-commit/Confirm/Cancel/Esc/`handle.cancel()`) funnels
   through one resolve fn; the modal closes BEFORE the callback (0297)
   so `on_resolve` may dispose its opener or open the next gate.
3. **Outside-press does NOT dismiss** — explicit endings only. Esc
   always cancels the whole prompt, also from inside the Other editor.
4. **Other routing**: the revealed TextInput's own key consumption
   shields the list (digits type, never jump); Up/Down/Esc bubble to
   the content-root handler; hollow Other (empty trimmed) REFUSES the
   commit with an accent note in the hint row.
5. **Focus-anchor repair** (found by design, pinned by the retreat
   test): unmounting the focused Other editor drops tree focus to None
   and keys then target the PANEL root, off the content root's routing
   path (the 0230 dead-keys class). Every unmount path — keyboard
   retreat, wheel retreat, click-uncheck — re-anchors focus on the
   content root first (a Capture-phase handler records its ViewId;
   programmatic focus needs no focusability, focus_init clause 3).
6. **Degenerate opens resolve `Cancelled`** instead of hanging the
   gated flow (debug builds assert loudly; test-pinned).
7. **Hint degrades by whole segments** right-anchored ("Esc cancels"
   survives longest) and the windowed `i/N` position is right-aligned
   — both born from a wave-test failure where "Esc cancels" truncated
   to "Esc ca…" at the width floor.

**Validation** (whole tree 1726 passed / 0 failed; clippy zero; fmt
clean; alloc idle pins green):
- Unit (`app::choice_prompt::tests`, real UiTree + overlay store, 27):
  `window_start_generalizes_the_select_rule_to_variable_heights`,
  `single_arrows_move_candidate_and_enter_commits_once` (0250 birth
  test), `single_click_selects_then_click_on_selected_commits`,
  `number_keys_jump_single_and_jump_toggle_multiple`,
  `escape_resolves_cancelled_never_silent`,
  `outside_press_does_not_dismiss_the_gate`,
  `cancel_button_resolves_and_handle_cancel_is_idempotent`,
  `handle_cancel_resolves_cancelled_exactly_once`,
  `multiple_space_toggles_enter_commits_canonical_order`,
  `multiple_confirm_button_commits_and_empty_set_is_legal`,
  `multiple_click_toggles_and_checked_seeds_apply`,
  `unanswerable_question_asserts_loudly_in_debug`,
  `open_without_overlay_store_asserts_loudly_in_debug`; flows half:
  `other_reveals_input_typing_does_not_fight_list_keys`,
  `other_empty_text_refuses_commit_until_text_or_retreat`,
  `other_in_multiple_mode_rides_the_checked_set`,
  `other_retreat_hides_input_and_keeps_typed_text`,
  `other_only_question_is_a_free_text_gate`,
  `resolve_may_dispose_the_opener_scope`,
  `on_resolve_may_open_the_next_prompt_and_gate_is_reopenable`,
  `windowing_keeps_highlight_reachable_with_twenty_options`,
  `wheel_moves_the_highlight`,
  `detail_rows_render_muted_under_the_label`,
  `narrow_width_stays_honest_and_operable`,
  `tab_cycles_the_trapped_focus_between_list_and_buttons`,
  `sequence_completes_in_order_and_cancel_reports_index`,
  `sequence_empty_completes_immediately`.
- Wave (`tests/wave_choice_prompt.rs`, real Driver/CaptureTerm, 8):
  `gate_single_full_keyboard_round_trip_and_vacated_repaint`,
  `gate_multiple_space_toggles_and_confirm_commits_the_set`,
  `gate_other_reveals_editor_digits_type_and_enter_commits`,
  `gate_escape_cancels_and_the_gate_reopens_clean`,
  `gate_outside_click_never_dismisses_or_acts_below` (live button
  under the modal: the press neither dismisses nor fires it),
  `gate_resolve_may_dispose_the_opener_scope_under_the_driver`,
  `gate_twenty_options_window_around_the_highlight`,
  `gate_sequence_chains_questions_through_the_wire` — all with
  `unknown_seq_count == 0`.

**Follow-ups revealed** (none blocking):
1. Disabled options (skip-in-movement exists in select_core; a gate
   consumer naming the need promotes it).
2. Custom Confirm/Cancel button labels (destructive gates may want
   "Delete"/"Keep" verbs on the buttons themselves).
3. Theme switches while a gate is open keep the at-open palette (the
   select-family posture, documented); a reactive-token modal story is
   an engine-wide question, not this component's.
4. The modal is sized at open; a viewport RESIZE while open clamps
   drawing but does not re-measure (the select family dismisses on
   resize; a gate must NOT auto-cancel — re-measure-in-place would
   need Modal-level support).

## Cycle-2 amendment (2026-07-23, BUILDER — REVIEWER findings folded)

REVIEWER's cycle-1 outputs (reviews/wave5/{acceptance-charter,
consumer-fit, review-cycle1}.md + tests/wave_choice_review.rs) landed
findings F1–F9 against this item. Disposition — each verified against
the code before folding:

- **F1 a11y (HIGH) — FOLDED.** Cycle 1 had rows with roles/labels but
  NO state values, a draw-only prompt (painted pixels, absent from
  the tree), and no focus affordance on the region. Now: prompt =
  `Heading` with the FULL text as label (pixels wrap/ellipsize;
  semantics don't — charter S4 spirit); region = `Menu "options"`
  with the current choice as value; rows = `MenuItem` (+"selected")
  single / `Checkbox` (on/off) multiple; editor = `Input` with focus
  truth; region focus VISIBLE (selection pair focused → accent ink
  unfocused, the RadioGroup precedent) — `focus_affordance_visible`
  pinned on the region and on a focused button (charter A5).
- **F2 shortcut letters (HIGH) — FOLDED.** `ChoiceOption::key(char)`
  + `option_key(..)` + `option_with(..)`: case-sensitive explicit
  activation ('a' ≠ 'A'; SHIFT-carrying uppercase accepted), dim
  `(a)` in the row, "a/A/d pick" hint segment, commit (single) /
  jump-toggle (multiple). Shield: a focused Other editor consumes
  printables first; declared keys outrank digit jumps; duplicates
  debug_assert.
- **F3 must-choose (MED) — FOLDED.** `dismissable(false)`: Cancel
  button gone, Esc unadvertised (a listed dead key would lie), Esc
  refuses VISIBLY ("an answer is required", accent; cleared by the
  next action). `handle.cancel()` + degenerate opens still resolve
  `Cancelled` — dismissability governs the user's endings, never the
  flow's guarantee of an outcome. Documented for destructive gates;
  demoed in decide.rs gate 1.
- **F4 layered Esc (MED, position conflict) — FOLDED, cycle-1
  position WITHDRAWN.** REVIEWER's argument wins on three counts:
  the engine's own innermost-surface idiom, the in-repo Combobox
  precedent, and reflex-protection of a half-typed draft. Esc while
  the editor is focused now retreats to the list (blur only —
  engagement, highlight, draft all kept); the second Esc cancels
  (dismissable) or refuses (must-choose). The hint tells the truth
  while editing ("Esc back to the list"). Mechanically the retreat
  rides the cycle-1 focus-anchor machinery, extended: the REGION's
  id is now the preferred anchor (its focus affordance is visible —
  A5) with the content root as fallback.
- **F5 draft persistence (MED) — already true, now spec'd + the Esc
  variant pinned** (the draft signal lives in the modal scope,
  outside every row view; fresh instances start empty).
- **F6 hint keys (LOW) — FOLDED** (letters segment first-to-degrade;
  Esc segment absent on must-choose; editing state swaps the truth).
- **F7 danger tint (LOW) — FOLDED** (`Error` token ink; deliberate
  exception ON THE RECORD: the highlighted-and-focused row wears the
  audited selection pair — error-on-selection-ground is not an
  audited combination; the tint holds in every other state).
- **F8/F9 (INFO) — accepted as-is**; the digit asymmetry (single:
  move-only, multiple: jump+toggle) is now documented as a decision
  in api.md, with declared letters as the activation vocabulary.

**New API surface** (all additive vs 0.2.7; ChoiceOption/ChoiceQuestion
field additions are free — the types are unreleased):
`ChoiceOption { key: Option<char>, danger: bool }` + `.key()` /
`.danger()`; `ChoicePrompt::{option_key, option_with, danger,
dismissable}`.

**Validation added (cycle 2)** — unit
(`choice_prompt::tests::c2`, 10):
`option_letters_commit_single_and_are_case_sensitive`,
`option_letters_toggle_in_multiple_and_declared_key_beats_digit`,
`option_letters_type_into_a_focused_other_editor`,
`non_dismissable_esc_refuses_visibly_and_clears_on_action`,
`esc_in_other_retreats_first_keeps_draft_then_second_esc_cancels`,
`esc_in_other_retreats_then_refuses_on_a_must_choose_gate`,
`danger_option_wears_error_ink_except_under_the_selection_pair`,
`a11y_tree_names_question_options_and_selection_state`,
`a11y_multiple_mode_reports_checkbox_state`,
`region_focus_affordance_visible_and_unfocused_highlight_distinct`;
wave: `gate_letters_danger_and_must_choose_through_the_wire`.
REVIEWER's 5 active charter tests stay green; per-skeleton readiness
for their 16 pending pins is mapped in
reviews/wave5/builder-cycle2-notes.md (all 16 ready, no conflicts
remaining). Files re-split for the budget: the key handler + hint row
moved to `choice_prompt_interact.rs`; cycle-2 tests in
`choice_prompt_tests_c2.rs`.

**Follow-ups amended**: cycle-1 follow-up 2 (custom button labels)
stands; new: (5) `ChoiceSequence` does not surface per-question
`dismissable` (builder-level knob; a data-driven must-choose sequence
would need it on `ChoiceQuestion` — deferred until a consumer names
it); (6) Esc-maps-to-named-option (charter G3's alternative shape)
not built — visible refusal shipped; the approval consumer's
Esc-as-defer maps to `Cancelled` on a dismissable gate today.

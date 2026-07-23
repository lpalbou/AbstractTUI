# Wave 5 — REVIEWER cycle-1 report (decision gate / ChoicePrompt)

Date: 2026-07-23. Reviewed state: BUILDER's DESIGN landed
(`docs/backlog/proposed/app-kits/0515_choice_prompt_decision_gate.md`);
widget source / example / unit tests had NOT landed at review time
(mid-cycle polls of `src/`, `examples/`, `tests/` found no gate
component; the last poll preceded this report by minutes). Findings
below therefore review the landed DESIGN against the acceptance charter
(`reviews/wave5/acceptance-charter.md`) and the consumer-fit study
(`reviews/wave5/consumer-fit.md`); code-level verdicts move to cycle 2.

## What the design gets right (credit where due)

- **Gate semantics are law-literate**: every ending funnels through one
  resolve fn gated by a `Cell<bool>` (charter G1); `Cancelled` is an
  explicit outcome and outside-press never dismisses (G2); the modal
  closes BEFORE the callback so resolve may dispose or chain (G4/G6,
  0297 cited by name); movement-vs-activation and click-on-selected
  follow the 0250 ruling verbatim (G5); degenerate opens resolve
  `Cancelled` instead of hanging the flow — the no-hang consequence of
  G2 taken seriously.
- **Answers are identity-shaped** (`ChoiceAnswer.selected: Vec<String>`
  of option ids, canonicalized) — charter G7 satisfied by design.
- **`ChoiceQuestion` is a plain data struct** — approval questions
  arrive as data from an agent/server; this is exactly the
  AskQuestion-shape fit (consumer-fit C2/C11), and `allow_multiple` is
  in from day one.
- **Empty-Other commit refuses visibly** (hint-row note) — the same
  position my charter O4 defends, converged independently.
- **The Other input row's space is reserved at open** — reveal never
  resizes the modal (a reflow-avoidance point my charter missed).
- **`ChoicePromptHandle` + programmatic `cancel()`** — the
  timeout/deadline consumer (approval expiry) gets its lever.
- **`ChoiceSequence` with `Cancelled { index, answers }`** — partial
  answers on cancel is honest; recursive open-in-resolve leans on the
  overlays dispatch-snapshot guarantee, correctly cited.
- No file collisions with the review lane: BUILDER's acceptance file is
  `tests/wave_choice_prompt.rs`, mine is `tests/wave_choice_review.rs`.

## Findings (numbered, severity, citations)

- **F1 — HIGH — Accessibility is absent from the spec.** 0515 contains
  zero mention of roles, labels, or access values; charter A1–A5 are
  all MUST. The hazard is concrete: the engine's raw `List` exposes
  only `"N items, selected i"` (option labels are painted, not in the
  tree — recorded live by my active test
  `charter_a1_a2_a11y_dialog_question_and_selection_state`), and
  0515's custom windowed rows (select-family pattern) will have the
  same hole unless the component adds per-option access surface.
  Demand: spec + implement the mapping (Dialog + prompt text;
  RadioGroup value or per-option entries; checked state in multiple
  mode; `Input` for the revealed Other; focus truth) using only the
  frozen Role vocabulary, and pin it via `accessibility_tree_text`.
- **F2 — HIGH (consumer MUST) — Per-option shortcut letters deferred.**
  0515 non-goals: "per-option shortcuts rendered in rows; type-ahead"
  (line ~136). The FIRST consumer's approval modal binds `a`/`A`/`d`
  and prints them in labels + hint (abstractcode-tui modals.rs:363-374,
  439-451); digits jump but do not carry that muscle memory, and the
  gate's focus-trapped modal leaves the consumer no seam to add their
  own chords. Consumer-fit C4 is MUST: without it the approval port
  re-wraps or rejects the component. Demand for cycle 2: an
  `option_key(char)` (declared, rendered, activating) or an explicit
  composition seam on the record.
- **F3 — MEDIUM — No must-choose story.** "Esc and Cancel always
  exist" (0515 ~85-86) is a clean policy, but charter G3 and
  consumer-fit C13 name a real consumer pulling the other way: the
  0215 wizard's per-step validation gates ("apply or go back", never
  Esc-into-limbo). `Cancelled` + caller re-open technically works but
  flickers and churns focus. Demand: a non-dismissable knob, or a
  demonstrated flicker-free re-open recipe with a test.
- **F4 — MEDIUM (position conflict, argue on the record) — Esc from
  inside the Other input cancels the WHOLE prompt** (0515 ~114-115),
  destroying the draft AND the gate in one reflex keypress. My charter
  K4 (SHOULD) defends layered Esc: first Esc leaves the field, second
  cancels — the engine's own innermost-surface idiom, and it protects
  a half-typed answer. BUILDER's "one predictable ending" defense is
  legitimate; this is a design decision to settle explicitly in cycle
  2, not to ship by default silently. If Esc-cancels-all stands, the
  hint row must say so while the field is focused.
- **F5 — MEDIUM — Other draft persistence unspecified** (charter O3).
  Highlight moves off the Other row and back: is the typed draft
  intact? With windowed row rebuilds this dies by accident unless the
  draft state lives outside the row's view. Spec it (persist within
  one gate lifetime; fresh instance starts empty) and pin it.
- **F6 — LOW — Key discoverability under-specified** (charter K2). The
  hint row is spec'd for the `i/N` note and the empty-Other refusal
  note; it should also name the keys (Enter/Esc/Space at minimum,
  option keys when F2 lands).
- **F7 — LOW — No danger-tinted option** (consumer-fit C5, charter T3
  SHOULD). Deny/destructive choices want the `Error` token tint —
  contrast-audited for free. Additive; ask for it in cycle 2.
- **F8 — INFO — Empty multi-select set is a legal answer** ("a gate,
  not a validator"). Accepted: the checked-state glyphs (`☐`) make the
  empty set visible, so the fabricated-selection law is satisfied; the
  deferred `require_min` knob is a reasonable non-goal.
- **F9 — INFO — `1-9` in single mode jump-only.** Consistent with
  0250 (digits move, Enter commits); fine — but note the asymmetry
  with multiple mode (`jump+toggle`) in the docs so it reads as a
  decision, not an accident.

## Charter coverage after cycle 1

- **Design addresses**: G1 G2 G4 G5 G6 G7 · O1 O2 O4 O5 · K1 K3 K5 ·
  S1 S2 · T1 T2 (selection pair; tokens-only stated).
- **Design silent or contrary**: A1–A5 (F1) · G3 (F3) · O3 (F5) · K2
  (F6) · K4 (F4, contrary position) · S3 (inherits the 0240 floor via
  Modal — must not opt out; verify in code) · T3 (F7) · P1/P2
  (inherited; verify in code).
- **My test file** (`tests/wave_choice_review.rs`, compiles + passes:
  5 active, 16 ignored-pending): active pins = G5+G1-substrate, K1,
  A1/A2-substrate, T2, P1 — all green against the reference gate
  (Modal + List composed in the gate shape; mounts repoint at
  `ChoicePrompt` when it lands). Skeletons ready for G1 G2 G3 G4 G6 G7
  O1 O2 O3 O4 A3 K2 K3 S1 S2 S3. Not yet skeletoned (cycle-2 additions
  once the API is real): A5 focus-affordance sweep, O5-specific
  Other-commit variant assertions, S4 long-label honesty, T3.

## Addendum — source began landing mid-review (same day, later)

`src/app/choice_prompt.rs` (459 lines) appeared after the findings
above were drafted. First-look facts, verified in the file:

- It is the MODEL half only: `ChoiceOption/Question/Answer/Outcome`,
  the `ChoicePrompt` builder, `ChoicePromptHandle`, `ChoiceSequence` —
  no view/draw code, no `TextInput`/`List` usage yet.
- It is NOT wired: no `choice_prompt` in `src/app/mod.rs` or the
  prelude, so the API is unreachable from `tests/` — my 16 skeletons
  stay honestly `#[ignore]`d; activation happens the moment the module
  + view land.
- **G1 confirmed at code level**: `Rc<Cell<bool>>` with
  `resolved.replace(true)` guarding the funnel (line ~270), and
  `handle.cancel()` rides the same exactly-once path ("a gate never
  closes silently", line ~356). Good.
- **F1 confirmed at code level so far**: zero `.role(` /
  `access_label` / `access_value` calls in the landed half. The view
  half decides this finding's fate — it stays the top cycle-2 demand.

## Cycle-2 demand list (in order)

1. Land the widget; I activate the 16 skeletons against the real API —
   they must pass as written (they pin charter clauses, not API names).
2. **F1**: a11y contract in spec + code, pinned by snapshot tests.
3. **F2**: per-option shortcut letters (approval-consumer MUST) or a
   recorded composition seam.
4. **F3**: must-choose story (knob or proven flicker-free recipe).
5. **F4**: settle the Esc-from-Other position on the record; whichever
   stands, the hint row tells the truth while the field is focused.
6. **F5**: draft persistence spec'd + pinned.
7. **F6/F7**: hint row names keys; danger tint option.
8. Code-level verification of the inherited clauses (S3 fixed rows
   under overflow, P1 zero-idle, T1 no color literals in source) — my
   T2/P1 actives re-run against the real component, plus a source
   inspection pass for T1.

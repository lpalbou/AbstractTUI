# ChoicePrompt cannot host a structured, scrollable, or reactive body — the tool-approval modal cannot adopt it

## Metadata
- Created: 2026-07-23
- Status: Completed 2026-07-23 (choice-fix wave)

## ADR status
- Governing ADRs: None. Component API surface; no wire or
  damage-contract impact.

## Context
0.2.8 shipped ChoicePrompt as "a modal decision gate for tool-approval"
and invited the first app to adopt it where it simplifies. The consumer
assessed it against the real tool-approval modal (`abstractcode-tui`
`src/ui/modals.rs` `open_approval`) — the exact surface the component
was pitched at — and the fit FAILS on the body, not the options. The
options half is genuinely good (id/label/detail/key/danger,
`allow_other` would even upgrade our canned "Denied by user" into a
typed deny reason, exactly-once resolve, a11y). Verdict recorded
app-side: keep hand-rolled, revisit when this item lands.

## What the approval modal's body actually contains
- A SCROLLABLE per-call card list: one card per tool call in the batch
  (batches are routinely multi-call), each with an aligned params grid,
  per-value truncation labels, and served-disabled/gate teaching rows
  ("this tool is disabled by GATE — approving cannot run it").
- An alternate body: `f` toggles the same modal between the card view
  and the full JSON of the batch (each view scrolls with its own exact
  content_size).
- A REACTIVE line: the tier-honesty row re-renders live when the
  accepted tier changes while the prompt is up (a static snapshot went
  stale — that was a shipped bug, fixed with a dyn_view).

## Why ChoicePrompt cannot carry any of that today
- `ChoiceQuestion.prompt` is a plain `String`; option rows carry one
  label + one detail row. There is no body/View slot.
- The prompt TRUNCATES: `choice_prompt_parts.rs::measure` wraps the
  prompt and cuts it at `prompt_cap` with an ellipsis — a multi-call
  batch flattened into the prompt string is silently cut, and nothing
  scrolls it. For an APPROVAL surface, truncating what the user is
  approving is not an acceptable degradation.
- No alternate-body or reactive-content mechanism: swapping card/JSON
  views would need cancel + reopen (flash, lost scroll), and a
  reactive tier line has no home in static question data.

## Proposed direction (engine's call)
- A `body(impl FnOnce(Scope) -> View)` slot on ChoicePrompt rendered
  between the prompt and the options, participating in the height
  budget with grow/scroll semantics (the modal already measures;
  the body would absorb flex like any Scroll). Static question DATA
  (`ChoiceQuestion`) stays serializable; the body slot is builder-only
  sugar for callers that need structure.
- That one slot covers all three gaps: the card list is a View, the
  `f` toggle is a signal read inside the body, the tier line is a
  dyn_view inside the body.
- The engine named "live-preview-on-move" as a possible misfit; the
  real first-app misfit is this body slot. Preview-on-move is not
  needed for approvals.

## App-side state to revisit when this lands
`abstractcode-tui` keeps its hand-rolled `open_approval` (which also
carries UiCtx modal-slot integration: atomic replace, the
picker-over-prompt restore, `wait_modal_for` bookkeeping — adopting
ChoicePrompt also means teaching UiCtx to retire a
`ChoicePromptHandle` where it retires a `Modal` today; `handle.cancel()`
exists, so that half is app work, not an engine gap). The simpler
`open_ask` free-text modal and future confirm gates are better first
adoption candidates than the approval modal.

## Completion report (2026-07-23)

**Verified at source**: `choice_prompt_parts.rs::measure` caps the
wrapped prompt at `viewport.h / 3` and truncates with an ellipsis —
exactly as filed; no scroll, no structure, no reactive slot existed.

**Shipped: the body slot, the filing's exact shape.**
`ChoicePrompt::body(impl FnOnce(Scope) -> View)` + `body_rows(n)`
(preferred rows, default 8, min 1). The closure runs in the MODAL
scope at mount (state created there dies on close), so a scrollable
body is `.body(|mcx| Scroll::new(cards).view(mcx))`; `ChoiceQuestion`
stays plain serializable data (builder-only sugar, as proposed). One
slot covers all three gaps: cards are a View, the `f` toggle and the
tier line are `dyn_view`s reading caller-owned signals — live-verified
by `body_dyn_view_updates_reactively_while_the_gate_is_up` (signal
flips while the gate is up; the body re-renders through the real
Driver).

**Design decisions** (documented in docs/api.md):

- **Routing**: keys stay the OPTIONS' vocabulary; the WHEEL scrolls a
  `Scroll`-wrapped body while the pointer is over it (the Scroll
  consumes the event before the gate's highlight handler) and moves
  the highlight elsewhere. No special-casing — this falls out of
  position-targeted wheel routing + focused-first key routing.
- **Focus (v1)**: the body is a DISPLAY region. ENFORCED, not just
  documented: the options region gained `.autofocus()` — without it,
  a focusable body child (a `Scroll` wrapper is focusable by nature)
  won `focus_init`'s "first focusable in document order" pick and
  arrows scrolled the body instead of moving the highlight (caught by
  the failing integration test, the exact 0230 dead-keys class). For
  body-less gates this is the same node `focus_first` already picked —
  no behavior change. A body Scroll focused later by an explicit user
  click consumes only movement keys; letters/digits/Enter bubble past
  it to the gate handler.
- **Height honesty (0240)**: `measure` allocates the OPTIONS first
  (their existing budget, never crushed by a tall body), then the body
  absorbs what remains up to `body_rows` with a 1-row floor; the body
  host is `.clip()`ed (an overflowing static body cannot paint over
  the rows below) with shrink weight 2.0 so it yields before the
  option region under pathological over-constraint; the collapsible
  blank rows absorb the floor's worst-case 1-row over-commit. Width:
  the body adopts the panel's solved width (v1 — no width knob).

**Tests** (tests/wave_choice_fix.rs + choice_prompt_tests_c2.rs):
`body_dyn_view_updates_reactively_while_the_gate_is_up`,
`scrollable_body_composes_with_twenty_options` (wheel-over-body
scrolls body only; arrows move highlight only; wheel-over-options
moves highlight; letters still commit — through real wire bytes incl.
SGR wheel),
`body_never_crushes_the_options_at_tiny_heights` (9-row viewport:
highlighted option visible + operable, body floors at 1 row and
clips), and
`a11y_unchanged_for_question_and_options_with_a_body` (every bare-gate
a11y entry survives verbatim; the body adds only honest display text
entries — a `text` row IS a text row to AT; focus stays on the
options; no phantom input). Demo: `examples/decide.rs` gate 2 gained a
scrollable tool-call manifest body.

**Deliberately NOT shipped** (scope honesty): no alternate-body verb
(the `f` toggle is a caller signal + `dyn_view` — strictly more
general), no body width knob, no live-preview-on-move (the filing
itself rules it out for approvals), no focus-barrier engine feature
(the autofocus anchor + documented click semantics cover v1; a
focus-exclusion subtree flag is a separate engine design if a consumer
ever needs interactive bodies).

**Gates**: whole-tree cargo test green, clippy zero, fmt clean,
semver-checks vs 0.2.8 additive-clean (new builder methods + one new
`Geometry` field, all internal or additive).

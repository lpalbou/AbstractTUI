# Proposed: ChoicePrompt approval-gate adoption gaps — panel width is content-derived, no aux-key vocabulary, dismissal vocabulary fixed + host-retire indistinguishable from user-dismiss

## Metadata
- Created: 2026-07-23 (0.2.9 re-assessment of ChoicePrompt for the
  abstractcode-tui tool-approval modal — the adoption 0287/0288 were
  filed to unblock)
- Status: Completed (2026-07-23 choice-0271 wave — gaps 1 and 3 shipped;
  gap 2 (aux-key vocabulary) split out UNSHIPPED to proposed 0272; see
  the completion report at the bottom)
- ID note: the first-app band is 02xx and 0290-0299 are used; 0300+
  is control-plane's (the 0292/0294 renumbering precedent). Filed
  into the 0271 gap — chronology reads from the Created date, not
  the id.

## ADR status
- Governing ADRs: None. ChoicePrompt public API surface
  (app/choice_prompt.rs) + gate geometry (choice_prompt_parts.rs) +
  hint row (choice_prompt_interact.rs).

## Context
0287 (body slot) and 0288 (option_key kitty fold) shipped in 0.2.9
and both verified at source as fixed — the body is a real reactive
display region (options allocated first, wheel scrolls it, the
options region autofocuses) and `option_key(…, 'a'/'A'/'d')` matches
both wire spellings. The first app re-assessed adopting ChoicePrompt
for its tool-approval modal (the consumer 0287/0288 were filed for)
and still cannot, for three reasons that are all engine-side API
gaps rather than bugs:

### 1. Panel width is content-derived; a body cannot widen it and no caller knob exists
`measure()` (choice_prompt_parts.rs) solves `inner_w` from the
options, the Other label, the prompt (capped 52), the buttons and
the hint — the BODY closure is invisible to width (only
`body_rows_pref` participates, and only in height). An approval gate
whose options are "Approve / Approve all (session) / Deny" lands at
≈45 cols. The consumer's approval body renders per-call cards —
aligned `key  value` param rows, first-class `$ command` lines, an
alternate pretty-JSON view — pre-wrapped at ~72 cols (its
hand-rolled modal is 76 wide, a maintainer-driven readability fix).
Under ChoicePrompt those rows CLIP (the body wrapper is `.clip()`)
or must pre-wrap to a width the caller can only obtain by
replicating `measure()`'s private arithmetic (the mirror-drift
class). Ask: a `min_inner_width(cols)` builder clamped to the
viewport, or let `body()` declare a preferred content width that
participates in `measure`.

### 2. No non-option key vocabulary; the hint row is closed
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

### 3. Dismissal vocabulary is fixed, and host-retire is indistinguishable from user-dismiss
Two halves:
- `dismissable(true)` forcibly renders `Button::new("Cancel")`
  (choice_prompt_view.rs) and the "Esc cancels" hint segment
  (choice_prompt_parts.rs). The approval consumer's Esc DEFERS — the
  gated run keeps waiting durably server-side; `d` is the only deny.
  Outcome-wise `ChoiceOutcome::Cancelled` IS a distinct,
  defer-wirable ending (verified: Esc rides `shortcut_labeled` into
  `resolve(Cancelled)`, never an option answer) — but the RENDERED
  contract says "Cancel" next to a "Deny" option, a mislabeled
  affordance on a consent surface. `dismissable(false)` is the
  opposite semantics (visible refusal). Ask: customizable dismiss
  label + hint text (e.g. `.dismiss_label("defer")` feeding both the
  button and the hint segment).
- `ChoicePromptHandle` exposes only `cancel()` (fires
  `on_resolve(Cancelled)`) and `is_open()`. A HOST retiring the
  prompt — the consumer's single-modal slot replacing it with a
  picker the user asked for; a policy lane auto-approving the gated
  wait while the prompt is up — fires the SAME `Cancelled` the
  user's Esc fires, so the consumer must thread a side-channel flag
  to keep "user deferred" apart from "host retired" (its reopen
  invariant depends on the difference: a replaced prompt must come
  back, a user-deferred one must stay away). "A gate never closes
  silently" is the right default; the ask is a distinguishable
  ending, not a silent one: either `ChoiceOutcome::Dismissed` vs
  programmatic `Cancelled`, or a documented `retire()` variant
  carrying its own outcome.

## What was verified working (0.2.9, at source + consumer tests)
- body()/body_rows(): reactive display region, options-first
  allocation, wheel-over-body scrolling, options-region autofocus.
- option_key letters: case-sensitive, both wire spellings via the
  0288 fold (`KeyEvent::normalized` at the letter matcher).
- Exactly-once resolve with modal-closed-before-callback (the 0297
  law) — `on_resolve` may dispose the opener or chain the next gate.

## Consumer state
abstractcode-tui keeps its hand-rolled approval modal (76-wide
scrollable cards, `f` JSON toggle, reactive tier line, Esc-defer
with an honest "Esc defers" hint, a/A/d shortcuts riding the 0.2.9
tree-shortcut fold). Its 0286 double registration is deleted —
`KeyChord::plain(Key::Char('A'))` alone now fires on both wires,
pinned by its raw-CSI-97;2u regression test. If the three gaps land,
the adoption assessment is worth re-running; checks 3/5/6 of its
seven-point behavior list already pass.

---

## Completion report (2026-07-23, choice-0271 wave)

**Scope**: gap 1 (panel width) and BOTH halves of gap 3 (dismiss
vocabulary + host retire) shipped. Gap 2 (aux-key vocabulary /
open hint row) is NOT in this wave — split out verbatim to
`proposed/first-app/0272_choice_prompt_aux_key_vocabulary.md` so the
recorded ask survives this file's move. The consumer's receipt named
the body-width knob as the verdict-flipper; the aux-key gap has a
live `pressed_chord` workaround minus advertisement.

**Verification at source (pre-fix)**: all three confirmed as filed —
`measure()` (choice_prompt_parts.rs) solved `inner_w` from
options/other/prompt(cap 52)/buttons/hint only (`body_rows_pref`
participated in height alone); `Button::new("Cancel")`
(choice_prompt_view.rs), the `"Esc cancels"` segment
(`hint_segments`), the advertised `shortcut_labeled(Esc, "Cancel")`
AND the hardcoded `buttons_w` width table were four sites of one
fixed vocabulary; `ChoicePromptHandle` exposed only
`cancel()`/`is_open()`, so a host retire fired the user's
`Cancelled`. Also confirmed: `ChoiceOutcome` is not
`#[non_exhaustive]`, so the alternative fix (a `Dismissed` variant)
would break every exhaustive match — `retire()` is the additive
shape.

**What shipped**:

- `ChoicePrompt::body_width(cols)` — a minimum content width the
  body contributes to `measure`'s max-chain (`natural`), clamped by
  the existing viewport margins. The prompt then wraps at the SOLVED
  (widened) width; options/hint render into it unchanged; a narrow
  terminal clamps the panel and the body clips inside its region
  (never the options). Mirrors `body_rows`' semantics: participates
  only when a `body` is set (`.and()` at the measure call), min 1.
- `ChoicePrompt::dismiss_label(label)` — ONE resolution
  (`dismiss_button_label` + `esc_segment` in parts.rs) feeds all
  the vocabulary surfaces so they can never disagree: the button,
  the hint's Esc segment, the advertised Esc shortcut (KeymapHelp),
  and the MEASURED widths (`buttons_w` is now computed
  `width(label) + 2` from Button's real arithmetic — the old
  hardcoded 8/9/19 table reproduced exactly for the defaults).
  Rendering rule: the unset default keeps the conjugated built-in
  "Esc cancels" byte-identical; a caller label renders VERBATIM
  ("Esc Defer") — the engine never synthesizes grammar from a label
  ("Dismiss" → "Dismisss" is why). Outcome unchanged:
  `ChoiceOutcome::Cancelled` (the filing itself verified Esc rides
  `shortcut_labeled` into `resolve(Cancelled)` — the label names the
  caller's wiring). Under `dismissable(false)` the label is inert:
  no button, no segment, the visible refusal stands.
- `ChoicePromptHandle::retire()` — the resolve closure split into a
  shared `finish()` (exactly-once flag + modal-close-before-callback,
  returns the not-yet-fired callback) with `resolve` = finish-then-
  call and `retire` = finish-then-DROP. The flag is consumed, so no
  later ending (Esc, buttons, `cancel()`, stray keys) can ever fire
  `on_resolve`; idempotent; a retire after resolution is a no-op.
  The module contract now names retire as the ONE deliberate
  silent-close exception: the host owns the outcome (picker-replace,
  policy auto-approval), keeping "host retired, reopen later"
  distinguishable from the user's `Cancelled`.

**Tests** (unit: choice_prompt_tests_c3.rs; integration:
tests/wave_choice_0271.rs, real Driver + wire bytes):
`body_width_widens_the_panel_the_options_could_not` (panel 74 = 72+2;
premise arm pins the clip),
`body_width_clamps_into_narrow_viewports_options_never_clip`,
`body_width_without_a_body_is_inert`,
`body_width_interplay_prompt_unwraps_and_hint_survives` (59-col
prompt renders whole; full hint; letters commit),
`dismiss_label_renames_button_and_hint_outcome_stays_cancelled`,
`dismiss_label_default_vocabulary_is_byte_stable`,
`dismiss_label_is_irrelevant_on_must_choose_gates`,
`dismiss_label_long_label_widens_button_and_hint_honestly`,
`retire_closes_the_gate_without_resolving`,
`retire_is_idempotent_and_post_retire_endings_are_inert` (incl. the
reopen invariant), `retire_after_resolution_is_a_no_op`;
`body_width_lets_a_72_col_card_body_render_unclipped`,
`dismiss_label_defer_renders_and_esc_still_resolves_cancelled`,
`host_retire_fires_no_outcome_and_the_gate_reopens_cleanly`.
Demo: `examples/decide.rs` gate 2 gained the 72-col manifest row +
`body_width(72)`.

**Deliberately NOT shipped**: gap 2's `.aux_key(chord, label,
handler)` (→ 0272); a `ChoiceOutcome::Dismissed` variant (breaking —
exhaustive matches); any grammar synthesis for hint conjugation.

**Gates** (2026-07-23, whole tree): cargo test --all-targets green —
78 suites ok, lib 1295 passed / 0 failed (18 ignored), incl. the new
wave_choice_0271 (3 tests) and the alloc-budget pins; cargo test
--doc 47 passed / 0 failed; clippy --all-targets zero warnings;
fmt --check clean; cargo semver-checks --baseline-version 0.2.9:
196 checks — 196 pass, 57 skip, "no semver update required" —
additive-clean (new API: `ChoicePrompt::body_width`,
`ChoicePrompt::dismiss_label`, `ChoicePromptHandle::retire`).

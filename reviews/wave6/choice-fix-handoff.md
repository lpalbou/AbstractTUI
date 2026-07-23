# Choice-fix wave handoff (first-app 0286/0287/0288) — 2026-07-23

Field-fix wave on the 0.2.8 ChoicePrompt component, driven by the
first app's post-adoption filings. All three filings are FIXED and
moved to `docs/backlog/completed/first-app/` with dated completion
reports (verification verdicts, failing-test-first evidence, test
names, gate numbers). One NEW filing came out of the verification:
first-app/0289.

This file exists because overview.md is deliberately NOT touched by
this wave — the rows below are for its owner to fold at the next
refresh.

## overview.md rows to fold

None of 0286/0287/0288 ever had rows in overview.md's Proposed ledger
(filed 2026-07-23, after the last refresh — only 0260/0280 of the
first-app track are listed there), so there is nothing to REMOVE.
To ADD:

Completed ledger rows:

| ID | Title | Final path |
| --- | --- | --- |
| 0286 | KeyChord shifted-letter shortcuts: two wire spellings folded at every chord-match site (`KeyChord::normalized`; tree shortcuts, Actions, `pressed_chord`) | completed/first-app/ |
| 0287 | ChoicePrompt body slot (`.body(\|mcx\| view)` + `body_rows`): structured/scrollable/reactive display region between prompt and options, options-first height budget | completed/first-app/ |
| 0288 | ChoicePrompt `option_key` uppercase letters dead on kitty — letter matcher folds both wire spellings (`KeyEvent::normalized`/`means_char`) | completed/first-app/ |

Proposed ledger row (new filing):

| ID | Title | Track | Note |
| --- | --- | --- | --- |
| 0289 | Typed uppercase inserts lowercase on kitty-spelling wires (`convert_event` drops the kitty `text` field; TextInput inserts the base char) | first-app | Found during the 0286/0288 verification — 0286's "text input is immune" claim was FALSE for this engine; corrected on the record in 0286's completion report. |

Count deltas: Completed +3, Proposed +1 −0 (the three fixed items were
never counted in the Proposed ledger; reconcile against the directory —
"the directory is the truth", per the first-app README's own rule).

## What shipped (one paragraph each)

**The shifted-letter fold (0286 + 0288).** A shifted letter has two
wire spellings (legacy `Char('A')`+no-mods; kitty `Char('a')`+SHIFT)
and every matcher compared exactly one. ONE fold now lives in
`ui/event.rs` (`fold_shifted_letter`: `Char(cased letter)`+SHIFT →
single-char uppercase, SHIFT dropped, other mods kept), surfaced as
`KeyChord::normalized()`, `KeyEvent::normalized()`, and
`KeyEvent::means_char(c)` (public — app-side matchers can reuse it),
and consumed at every match site: ChoicePrompt's letter matcher, tree
shortcut resolution (both comparisons), the Actions registry (map
keyed on normalized chords; collisions judged normalized; authored
spelling kept for display), and `KeyState::pressed_chord` (its "same
matching rules as shortcuts" doc line made it load-bearing). Case
stays meaningful: a declared/registered lowercase letter never fires
on Shift+letter. Shifted non-letter symbols keep their documented wire
split (layout-dependent — the engine does not guess).

**The body slot (0287).** `ChoicePrompt::body(impl FnOnce(Scope) ->
View)` + `body_rows(n)` (default 8): a clipped, panel-width DISPLAY
region between the prompt heading and the options. The closure runs in
the modal scope; a Scroll-wrapped body scrolls under the wheel while
the pointer is over it; every key stays the options' vocabulary — the
options region now AUTOFOCUSES at open (without it a focusable body
child won focus_init's first-focusable pick and arrows scrolled the
body; for body-less gates it is the same node focus_first already
picked). Height budget: options first (never crushed — 0240), body
absorbs the remainder up to its preference, floors at one row.
Reactive bodies are `dyn_view`s reading caller-owned signals —
re-render live while the gate is up. `examples/decide.rs` gate 2
demos a scrollable tool-call manifest.

## Consumer follow-ups (abstractcode-tui)

- The approval modal's DOUBLE chord registration (both spellings of
  Shift+A calling one closure, `src/ui/modals.rs::open_approval`) can
  be deleted — either single spelling now matches both wires; the
  app's kitty-bytes regression test keeps passing through the engine
  fold.
- The 0287 verdict "keep hand-rolled, revisit when this lands" can be
  revisited: the body slot covers the card list (a View), the `f`
  card/JSON toggle (caller signal + `dyn_view` swap inside the body),
  and the live tier line (`dyn_view`). The UiCtx modal-slot
  integration half (retiring a `ChoicePromptHandle` where a `Modal`
  retires today) remains app work, as the filing itself recorded.
- New engine filing 0289 affects the app's TextInputs on
  kitty-spelling wires (typed Shift+A inserts 'a'): no app workaround
  recommended (an input-site fold would double-uppercase if the engine
  later threads the kitty text field); track the filing.

## Gates (2026-07-23, whole tree)

- `cargo test --all-targets`: green — lib 1284 passed / 0 failed
  (18 ignored), every integration suite ok, including the new
  `tests/wave_choice_fix.rs` (11 tests) and the alloc-budget pins.
- `cargo test --doc`: 47 passed / 0 failed (37 ignored).
- `cargo clippy --all-targets`: zero warnings.
- `cargo fmt --check`: clean.
- `cargo semver-checks --baseline-version 0.2.8`: 196 checks pass,
  0 fail — additive-clean (new public API only: `KeyChord::normalized`,
  `KeyEvent::normalized`, `KeyEvent::means_char`,
  `ChoicePrompt::body`, `ChoicePrompt::body_rows`).

## Failing-test-first evidence (pre-fix tree)

Written and run BEFORE the fix; failed exactly as the filings predict,
all through the REAL Driver with wire bytes (`CaptureTerm::push_input`,
kitty CSI-u `\x1b[97;2u` for Shift+A):

- `kitty_shift_a_fires_the_uppercase_option_key_and_commits` — FAILED
- `kitty_shift_a_jump_toggles_in_multiple_mode` — FAILED
- `tree_shortcut_shifted_letter_matches_both_wire_spellings` — FAILED
- `action_chord_shifted_letter_matches_both_wire_spellings` — FAILED
- unit: `kitty_shift_spelling_folds_to_the_uppercase_option_key`,
  `pressed_chord_folds_the_two_shifted_letter_spellings` — FAILED
- pins that passed pre-fix and must keep passing:
  `legacy_shift_a_fires_the_uppercase_option_key_and_commits`,
  `lowercase_declared_key_refuses_shift_letter_on_both_wires`,
  `lowercase_chord_refuses_the_shifted_letter_on_both_wires`,
  `decide_example_gate1_keys_fire_on_both_wires` (the example's
  `o/k/d` on legacy bytes AND kitty plain `CSI code u` spellings).

The 0287 suite (post-API, wave_choice_fix.rs + unit):
`body_dyn_view_updates_reactively_while_the_gate_is_up`,
`scrollable_body_composes_with_twenty_options`,
`body_never_crushes_the_options_at_tiny_heights`,
`a11y_unchanged_for_question_and_options_with_a_body`. The scroll test
caught a REAL defect mid-wave (a focusable body Scroll stole
focus_init's first-focusable pick — arrows scrolled the body); the
region-autofocus enforcement is the fix, not a test adjustment.

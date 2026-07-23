# Choice-0271 wave handoff (first-app 0271) — 2026-07-23

Field-fix wave on the 0.2.9 ChoicePrompt, driven by the first app's
post-0.2.9 adoption re-assessment (its receipt: "the body-width knob
alone would likely flip the verdict"). 0271 recorded three gaps; this
wave ships gap 1 (panel width) and both halves of gap 3 (dismiss
vocabulary + host retire). Gap 2 (aux-key vocabulary) is deliberately
NOT shipped: it is split out verbatim as NEW filing first-app/0272 so
the recorded ask survives 0271's move. 0271 lives in
`docs/backlog/completed/first-app/` with a dated completion report
(source verification, design decisions, test names, gate numbers).

This file exists because overview.md is deliberately NOT touched by
this wave — the rows below are for its owner to fold at the next
refresh.

## overview.md rows to fold

0271 never had a row in overview.md's Proposed ledger (filed
2026-07-23, after the last refresh — the same situation as the
0286/0288 wave), so there is nothing to REMOVE. To ADD:

Completed ledger row:

| ID | Title | Final path |
| --- | --- | --- |
| 0271 | ChoicePrompt approval-gate adoption gaps — `body_width(cols)` (the body participates in the panel's content-derived measure), `dismiss_label(label)` (button + hint Esc segment + advertised shortcut follow the caller's vocabulary; outcome stays `Cancelled`), `ChoicePromptHandle::retire()` (host close without resolving, distinct from user-Esc; exactly-once flag consumed) | completed/first-app/ |

Proposed ledger row (new filing — the unshipped 0271 gap 2):

| ID | Title | Track | Note |
| --- | --- | --- | --- |
| 0272 | ChoicePrompt aux-key vocabulary — no non-option key surface and the hint row is closed to callers (`.aux_key(chord, label, handler)` ask; the consumer's `f` cards↔JSON toggle) | first-app | Split out of 0271 at its completion, ask text verbatim; consumer workaround = `KeyState::pressed_chord` minus advertisement. |

Count deltas: Completed +1, Proposed +1 −0 (0271 was never counted in
the Proposed ledger; reconcile against the directory — "the directory
is the truth", per the first-app README's own rule).

## What shipped (one paragraph each)

**`ChoicePrompt::body_width(cols)` — the blocker.** The panel width
was solved by `measure()` from the options, the Other label, the
prompt (capped 52), the buttons and the hint; the BODY closure was
invisible to it, so the consumer's ~72-col approval cards clipped
inside a ~45-col panel sized by "Approve / Approve all / Deny".
`body_width` is the body's honest declaration: it folds into the same
max-chain (`natural`) as every other content line and is clamped by
the SAME viewport margins — the prompt wraps at the widened width,
options and hint gain the room, and a narrow terminal clamps the
panel so the body clips inside its region (never the options). Like
`body_rows`, the knob participates only when a `body` is set (min 1).

**`ChoicePrompt::dismiss_label(label)`.** The dismiss vocabulary was
hardcoded at FOUR sites — `Button::new("Cancel")`, the "Esc cancels"
hint segment, the advertised `shortcut_labeled(Esc, "Cancel")`, and
the `buttons_w` width table (8/9/19 = the labels' widths baked in).
One resolution (`dismiss_button_label` + `esc_segment`) now feeds all
of them, so they can never disagree; `buttons_w` is computed
`width(label) + 2` (Button's real arithmetic — defaults reproduce the
old table exactly). Rendering rule: the unset default keeps the
conjugated built-in "Esc cancels" byte-identical for every existing
gate; a caller label renders VERBATIM ("Esc Defer") — the engine
never synthesizes grammar. The OUTCOME stays
`ChoiceOutcome::Cancelled` (renaming/adding a variant would break
exhaustive matches); the label names what the caller's wiring does
with it. Under `dismissable(false)` the label is inert and the
visible refusal stands.

**`ChoicePromptHandle::retire()`.** The handle exposed only
`cancel()` (fires `on_resolve(Cancelled)`) — a host retiring the gate
(single-modal-slot replace, policy auto-approval while the prompt is
up) was indistinguishable from the user's Esc, forcing a side-channel
flag app-side. The resolve closure is split into a shared `finish()`
(exactly-once flag + modal-close-BEFORE-callback, the 0297 law;
returns the not-yet-fired callback): `resolve` = finish-then-call,
`retire` = finish-then-DROP. The flag is consumed, so no later ending
(Esc, buttons, `cancel()`, stray keys) can ever fire the callback;
idempotent; a retire after resolution is a no-op. The module contract
names retire as the ONE deliberate silent-close exception — the host
owns the outcome; "host retired, reopen later" vs "user deferred,
stay away" is now distinguishable by construction.

## Consumer follow-ups (abstractcode-tui)

- The adoption assessment is worth re-running: of the three 0271
  gaps, the verdict-flipper (`body_width`) plus the dismiss
  vocabulary and the retire verb are live; only the `f` toggle's
  ADVERTISEMENT (0272) remains — the toggle itself works today via a
  caller signal + `dyn_view` body swap, observed with
  `KeyState::pressed_chord`.
- Esc-defer wiring: keep mapping `ChoiceOutcome::Cancelled` → defer;
  `dismiss_label("Defer")` makes the rendered contract say so.
- The reopen invariant: use `handle.retire()` on picker-replace and
  tier-raise auto-close; `Cancelled` now ALWAYS means an explicit
  ending the user (or the caller's own `cancel()`) chose.

## Gates (2026-07-23, whole tree)

- `cargo test --all-targets`: green — 78 suites ok, lib 1295 passed /
  0 failed (18 ignored), including the new
  `tests/wave_choice_0271.rs` (3 tests) and the alloc-budget pins
  (`alloc_budget.rs`, `perf_*` suites).
- `cargo test --doc`: 47 passed / 0 failed (37 ignored).
- `cargo clippy --all-targets`: zero warnings.
- `cargo fmt --check`: clean.
- `cargo semver-checks --baseline-version 0.2.9`: 196 checks —
  196 pass, 57 skip, "no semver update required" — additive-clean
  (new public API only: `ChoicePrompt::body_width`,
  `ChoicePrompt::dismiss_label`, `ChoicePromptHandle::retire`).

## Test names (new)

Unit (`src/app/choice_prompt_tests_c3.rs`, 11):
`body_width_widens_the_panel_the_options_could_not`,
`body_width_clamps_into_narrow_viewports_options_never_clip`,
`body_width_without_a_body_is_inert`,
`body_width_interplay_prompt_unwraps_and_hint_survives`,
`dismiss_label_renames_button_and_hint_outcome_stays_cancelled`,
`dismiss_label_default_vocabulary_is_byte_stable`,
`dismiss_label_is_irrelevant_on_must_choose_gates`,
`dismiss_label_long_label_widens_button_and_hint_honestly`,
`retire_closes_the_gate_without_resolving`,
`retire_is_idempotent_and_post_retire_endings_are_inert`,
`retire_after_resolution_is_a_no_op`.

Integration (`tests/wave_choice_0271.rs`, real Driver + wire bytes,
3): `body_width_lets_a_72_col_card_body_render_unclipped`,
`dismiss_label_defer_renders_and_esc_still_resolves_cancelled`,
`host_retire_fires_no_outcome_and_the_gate_reopens_cleanly`.

Demo: `examples/decide.rs` gate 2 gained a 72-col manifest row +
`.body_width(72)`.

# Proposed: Completion triggers fire on any mid-text token — no position policy

## Metadata
- Created: 2026-07-22
- Status: Completed (shipped the per-trigger position policy)
- Completed: 2026-07-23

## ADR status
- Governing ADRs: None. ADR impact: none — completion controller semantics.

## Context
`abstractcode-tui`'s composer registers a `'/'` completion for slash
commands. Slash commands are only commands at the START of the draft
(`commands::parse` requires it — "look in /src for the parser" is a prompt,
not a command). The engine's `find_token`
(src/app/anchored_completion.rs:441-459) arms the trigger for ANY
whitespace-delimited token whose first char matches, at any position. So a
user typing a prompt that mentions a path ("check /mo dels…") gets a live
dropdown mid-sentence, and — because the dropdown intercepts Enter while
open — an Enter meant to SUBMIT can silently accept "/model " into the
draft instead.

## Current code reality
- `find_token` walks back to the previous whitespace and matches the
  token's first char against the registered triggers — no notion of
  "start of input" or "start of line".
- The provider callback receives only the query string (`Fn(&str) ->
  Vec<CompletionCandidate>`), not the token's position, so a provider
  cannot express a position policy itself without capturing the
  `TextAreaState` and re-deriving context (which is what the app now does:
  it returns no candidates unless `state.text().trim_start()` begins with
  the trigger — a whole-draft approximation, not a real token position).
- `@`-mention style completion legitimately WANTS mid-text triggering, so
  the current behavior is right for one class and wrong for the other —
  this is a per-trigger policy, not a global toggle.

## Proposed direction (engine's call)
- Per-trigger position policy at registration, e.g.
  `.trigger_at(char, TriggerAt::InputStart | TriggerAt::LineStart |
  TriggerAt::Anywhere, provider)` with `Anywhere` the current default; or
- pass a small context struct to providers (`query`, `token_start`,
  `is_first_token`) so policies live app-side without re-deriving from the
  state handle.

## App-side workaround to delete when this lands
`abstractcode-tui src/ui/chrome.rs` composer: the trigger closure captures
the `TextAreaState` and returns no candidates unless the draft trim-starts
with `'/'`.

## Completion report (2026-07-23, 0.2.6 field wave)

Shipped the item's first proposed direction — per-trigger position
policy at registration (`app/anchored_completion.rs`, re-exported
through `app::anchored` + the prelude):

- `TriggerPosition::{Anywhere, StartOfInput, StartOfLine}` +
  `Completion::trigger_at(char, TriggerPosition, provider)`.
  `Anywhere` is the pre-0292 behavior and the default — plain
  `trigger` now delegates to `trigger_at(.., Anywhere, ..)`, so
  existing registrations are byte-identical. A `WordStart` variant was
  deliberately NOT added: every trigger already requires a token
  (word) start (`find_token` scans back to whitespace), so it would be
  a confusing synonym of `Anywhere` — the policy constrains WHERE the
  token sits, not whether it is a token.
- Semantics are FIRST-TOKEN, not byte-zero: `StartOfInput` accepts
  leading whitespace (blank lines included) before the trigger — the
  consumer's own `trim_start` convention — and `StartOfLine` accepts
  intra-line leading whitespace after the previous newline. When the
  policy refuses, `find_token` reports no token at all: the provider
  is NEVER consulted (the app workaround had to run the provider to
  refuse) and the dropdown never opens, so Enter submits instead of
  silently accepting a candidate mid-sentence — the filed failure.
- The same char may be registered more than once with different
  policies (first passing registration wins) — '/'-commands at input
  start and '/'-paths anywhere can coexist; documented on
  `trigger_at`.

Tests (`app/anchored_policy_tests.rs`, child of the completion rig
suite, real mounted composer + overlay store):
- `start_of_input_fires_only_for_the_drafts_first_token` — the filed
  "check /mo" shape never opens AND the provider stays unconsulted;
  '@' mentions keep firing mid-text; draft-start + whitespace-led
  drafts fire.
- `start_of_line_vs_start_of_input_differ_on_multiline_drafts` — the
  same "hello⏎/h" draft (typed through Alt+Enter) opens under
  `StartOfLine` and stays quiet under `StartOfInput`; mid-line tokens
  on line two stay quiet under both.
- `positioned_trigger_refilters_dismisses_and_accepts_unchanged` —
  refilter (one provider call per edit), Escape-mute, the
  policy/mute gate composition, and Enter-accept all unchanged
  through the policy gate.

Docs: `docs/api.md` completion section (one-line trigger-policy
note), CHANGELOG under `[Unreleased]`. Gates: whole-tree tests green,
clippy clean, semver-additive vs 0.2.6 (new enum + new method;
`trigger` signature untouched).

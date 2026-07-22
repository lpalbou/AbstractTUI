# Proposed: Completion triggers fire on any mid-text token — no position policy

## Metadata
- Created: 2026-07-22
- Status: Proposed (API gap report — first-app finding, 0.2.0 adoption wave)
- Completed: N/A

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

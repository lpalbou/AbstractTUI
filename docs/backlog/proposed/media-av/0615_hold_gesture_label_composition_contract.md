# 0615 — `gesture_label` wording does not compose into sentences; rule the template or expose parts

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review,
  `reviews/wave12/visual-to-code-handoff.md` §6 second bullet;
  recorded by the code seat in
  `reviews/wave12/code-to-visual-handoff.md` "#6")
- Track: media-av (the label ships with 0610 push-to-talk; the
  generalized fn lives in `app::keys` from games/0700)
- Class: API-shape paper cut (docs-or-API decision)
- Severity: P3 — one example re-template fixed the sighting; every
  future voice/hold-to-act consumer will re-trip it
- Engine: verified on 0.2.22 (2026-07-25 — strings unchanged since
  0700/0610 shipped)

## The evidence

`PushToTalk::gesture_label()` returns "press Space to start/stop" on
Degraded fidelity — a complete imperative clause. No sentence template
survives it: `format!("{} to talk", label)` produced
"press Space to start/stop to talk" in `voice_mock`. The example was
re-templated to "talk: {label}" (label-last, colon-joined), which
works for BOTH fidelity strings ("hold Space" / "press Space to
start/stop"). Nothing documents that this is the intended composition,
so the next consumer writes the natural infinitive template and ships
the double-verb.

## Current code reality (verified 2026-07-25, 0.2.22)

- `src/app/keys.rs:287-296` — `hold_gesture_label(fidelity, chord)`
  returns `"hold {chord}"` (Full) / `"press {chord} to start/stop"`
  (Degraded). The strings are the HONESTY mechanism (a Degraded wire
  cannot end a hold on release — 0610's truthful-label contract) and
  are test-pinned (`keys_tests.rs:257-264`,
  `push_to_talk.rs:351,441`).
- `src/app/push_to_talk.rs:286-291` — `gesture_label()` delegates.
- Neither fn's docs state a composition rule; the rustdoc example
  (`push_to_talk.rs:111`) shows the label alone.

## What we want

Smallest honest surface first — a DOCUMENTED composition contract:

- State on both fns (rustdoc — docs.rs reaches consumers) that the
  label is a COMPLETE CLAUSE, never an infinitive's object: compose as
  `"{action}: {label}"` (the voice_mock template, now the canonical
  recipe), never `"{label} to {action}"`.

If a consumer later needs its own wording, the additive step is parts,
not string surgery: expose the chord (`PushToTalk::chord()` exists)
plus the MODE (`PttMode`/`KeyFidelity` — both public) and document
building custom labels from them — apps that compose their own wording
must preserve the fidelity honesty (never print "hold" on a Degraded
wire). No string change to the existing labels: they are pinned, and
downstream apps may already match them.

## Validation

- Rustdoc on `hold_gesture_label` + `PushToTalk::gesture_label` carries
  the template rule and the anti-example verbatim.
- `voice_mock`'s footer cited from the docs as the canonical usage.
- Existing label pins unchanged (byte-identical strings).

## Non-goals

- Localization/i18n of engine strings (out of scope for the engine;
  parts-based composition is the door an app localizes through).
- Changing the Degraded-wire semantics or label truthfulness (0610's
  contract stands; this item is purely about COMPOSING the truth).

## Related

- Completed media-av 0610 (push-to-talk contract — the label's honesty
  rationale), completed games 0700 (`app::keys`, `hold_gesture_label`),
  completed media-av 0650 (voice_mock — the template's home).

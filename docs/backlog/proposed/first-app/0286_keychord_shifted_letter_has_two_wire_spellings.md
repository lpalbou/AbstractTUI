# Proposed: KeyChord shifted-letter shortcuts have two wire spellings — plain Char('A') is a dead key on kitty terminals

## Metadata
- Created: 2026-07-23
- Status: Proposed

## ADR status
- Governing ADRs: None. Input normalization at the chord-matching
  boundary; no wire or damage-contract impact.

## Context
Live P0 in the first app (2026-07-23, maintainer report): the approval
modal's "approve all" shortcut — registered as
`KeyChord::plain(Key::Char('A'))` — **never fired** on the maintainer's
terminal. He pressed Shift+A repeatedly, nothing happened, and every
subsequent tool batch prompted again ("why does it keep asking when I
selected approve all"). The registration is the natural spelling every
author reaches for, and it works in tests and on legacy wires — which is
exactly what makes it a trap.

Root cause: a shifted letter has TWO wire spellings and a chord matches
exactly one.
- Legacy wire: Shift+A arrives as byte 0x41 → `Char('A')`, `Mods::NONE`
  (shift baked into the char, no modifier info).
- Kitty keyboard protocol: Shift+A arrives as `CSI 97;2u` →
  `Char('a')` + `Mods::SHIFT` — the decoder deliberately keeps the BASE
  key identity even when the shifted alternate is reported
  (`alternate_keys_use_primary_identity`, input/kitty.rs), and
  `convert_event` (app/events.rs) forwards key + mods with no case
  normalization.

So `plain(Char('A'))` matches legacy only, and
`new(Mods::SHIFT, Char('a'))` matches kitty only. Any app that registers
one spelling ships a shortcut that dies on half the terminal population —
and the failure is silent (the key routes on, unconsumed). Text INPUT is
immune (the kitty `text` field carries "A"); only shortcut matching
splits.

## Current code reality
- Engine: chord matching is exact equality on (key, mods) — tree
  shortcut resolution and `Actions::dispatch_chord` both. No
  shifted-letter folding anywhere between decode and match.
- First consumer evidence: `abstractcode-tui` approval modal registers
  BOTH spellings side by side with a comment explaining why
  (`src/ui/modals.rs` `open_approval`); pinned by
  `approve_all_fires_on_the_kitty_shift_a_spelling_and_covers_the_next_batch`
  (pushes the raw `\x1b[97;2u` bytes).

## Proposed direction (engine's call)
Normalize at ONE boundary so both spellings match one registration.
Options, roughly in order of preference:
- Fold at chord-match time: when comparing a key event against a chord,
  treat `Char(uppercase alpha) + NONE` and `Char(lowercase alpha) +
  SHIFT` as the same chord (both directions). Localized to the two
  match sites; wire truth stays untouched for handlers that read raw
  events.
- Or normalize in `convert_event`: canonicalize `Char(c) + SHIFT` where
  `c` is a lowercase letter to `Char(C) + NONE` (kitty → legacy
  spelling). One site, but changes what `on_key` handlers observe on
  kitty wires — an audit of existing consumers is owed if this path is
  taken.
- Non-alpha shifted keys (`?`, `~`, …) have the same latent split
  (kitty reports base + SHIFT); alpha-only folding covers the common
  case first and is honestly partial.

## App-side workaround to delete when this lands
The double registration in `abstractcode-tui`'s approval modal (both
chords call the same closure). The regression test should keep passing
byte-identically through the engine fold.

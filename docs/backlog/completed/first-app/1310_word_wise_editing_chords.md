# 1310 — Codex-compatible navigation chords in the text widgets

Status: completed 2026-08-16
Owner: engine (widgets/input + widgets/textarea)
Effort: S

## The field ask

abstractcode-tui, 2026-08-16: "I want the same key mapping as Codex for
the prompt input. I just want to be able to quickly navigate the text of
the prompt."

The composer is engine-owned (`widgets::TextArea` + `TextAreaState`), so
this is ours.

## What was missing

Not the semantics — both widgets already stepped by word on `Alt+←/→`.
What was missing is that ONE gesture has THREE spellings, and Codex
binds all three because macOS has no Option-arrow escape of its own.
A widget that knows only `CSI 1;3D` is dead wherever a terminal sends
readline's `ESC b` instead (iTerm2's "Natural Text Editing" preset) or
the Linux/Windows `CSI 1;5D`.

## Shipped: Codex's `EditorKeymap` defaults, verbatim

Source of truth: `codex-rs/tui/src/keymap.rs`.

| Codex binding | Keys |
|---|---|
| `move_word_left` | Alt+b, Alt+←, Ctrl+← |
| `move_word_right` | Alt+f, Alt+→, Ctrl+→ |
| `move_line_start` | Home, Ctrl+A |
| `move_line_end` | End, Ctrl+E |
| `delete_backward_word` | Alt+Backspace, Ctrl+Backspace, Ctrl+W |
| `delete_forward_word` | Alt+Delete, Ctrl+Delete, Alt+D |

`widgets::edit_keys` owns the word rows for both widgets so they cannot
drift; the line rows sit beside the Home/End arms they share behavior
with. Shift is transparent, so "extend the selection by word" is the
same gesture. The letters require EXACTLY one modifier, which keeps
AltGr (Ctrl+Alt on Windows/Linux) from ever reading as a word chord.

NOT adopted from Codex: `Ctrl+B`/`Ctrl+F` (character motion),
`Ctrl+P`/`Ctrl+N` (line motion), `Ctrl+H`/`Ctrl+D` (character delete),
`Ctrl+U`/`Ctrl+K` (kill) and `Ctrl+Y` (yank). They are real Codex
bindings, but a focused editor claiming them shadows app chords that
consumers bind today (abstractcode-tui binds Ctrl+D and Ctrl+C among
others). The scope asked for was navigation.

## Rejected: decoding the meta prefix in the parser

macOS Terminal.app's "Use Option as Meta Key" spells Option+← as
`ESC ESC [ D`, which our parser reads — like crossterm's
(`parse.rs:77`) — as an Esc keypress followed by a plain arrow. A
deferred-ESC decode was built, reviewed, and REVERTED: it is a KERNEL
contract change with blast radius across every escape path (the
review's verification round found it silently swallowing the Esc
keypress on 14 inputs, including "Esc while the mouse moves"), and
Codex does not do it. Codex solves that terminal at the KEYMAP level,
with Alt+b / Alt+f. So do we. The kernel stays as it was.

## Also fixed (found by adversarial review, in this code)

- An anchor parked ON the caret (Shift+Left then Shift+Right — `move_to`
  only arms an anchor that is `None`) survived the next delete, which
  moved the caret out from under it and resurrected it as a PHANTOM
  one-character selection the following keystroke would silently eat.
  `delete_selection` now drops an empty anchor in both widgets — which
  also repaired the same latent bug on the typing path (`insert_text`
  never cleared the anchor itself).
- A word delete at the buffer edge reported `edited: true`, so holding
  Alt+Backspace at position 0 fired `on_change` and pushed history per
  key repeat.

## Evidence

- `tests/wave_composer.rs::navigation_chords_match_codex_defaults` —
  one row per Codex binding, driven as real wire bytes through the real
  parser and driver, asserting caret or text. This is the parity guard:
  if a row stops matching Codex, it fails.
- `src/widgets/edit_keys_tests.rs` — the spelling table plus the AltGr
  and single-modifier guards.
- `src/widgets/input_tests.rs` — the F7 masked rule for word DELETES
  (a masked field is one word, so the rubout runs to the field edge),
  cluster-atomic word delete (a ZWJ family deletes whole), and the
  phantom-anchor regression.

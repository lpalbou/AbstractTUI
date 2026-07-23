# 1100 — TextInput: no way to open with the cursor at the end (or content selected) — prefilled editors insert at position 0

## Metadata

- Created: 2026-07-25 (abstractcore-console M2, engine 0.2.22)
- Status: proposed
- Severity: P3 (UX defect in every edit-prefilled form; workaround
  holds)
- Class: API gap

## Context

A config console's field editors open PREFILLED with the current value
(`TextInput::new().value(signal_holding("three"))`). The cursor starts
at byte 0, so the natural first gesture — typing the replacement —
INSERTS BEFORE the old value: typing `9` over a prefilled `three`
yields `9three`. Caught live in abstractcore-console's editor test
(the validation error `"9three" is not an integer` was the tell) and
reproduced in every prefilled form (route editor, profile editor).
Every editor-opening app will hit this; edit-prefilled forms are the
config-console class's bread and butter.

## Current code reality (0.2.22)

`src/widgets/input.rs:138-242` — the builder surface is `new / value /
placeholder / placeholder_while_focused / masked / layout / on_change /
on_submit / on_paste / view / element`. Nothing positions the cursor or
selects the content at mount; the internal cursor state starts at 0 and
there is no `TextInputState`-style handle (TextArea has
`TextAreaState`; TextInput deliberately has no state wire).

## Repro

```rust
let v = cx.signal("three".to_string());
TextInput::new().value(v).view(cx); // focus it, type "9"
assert_eq!(v.get(), "9three");      // user expected "three9" or "9"
```

## What the engine could own

Any one of these closes it (first is smallest):

1. `.cursor_at_end(true)` — open with the cursor after the last
   grapheme (the "edit this value" idiom).
2. `.select_all_on_focus(true)` — first keystroke REPLACES the value
   (the GUI-form idiom; also solves clear-then-retype).
3. A `TextInputState` wire like TextArea's, making cursor position
   app-drivable.

## Workaround in the field (delete when fixed)

Operators press `End` (or Ctrl+E is not bound — arrow/End only) before
editing; the app's tests send `\x1b[F` before backspacing
(`abstractcore/console-tui/tests/headless_ui.rs`,
`scalar_editor_validates_submits_and_closes_on_success`). The editors'
hint lines do not teach this — a fresh operator WILL type into
position 0 once.

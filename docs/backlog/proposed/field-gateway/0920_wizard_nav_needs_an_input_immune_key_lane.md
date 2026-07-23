# Proposed: wizard/tab navigation needs an input-immune key lane (0520 evidence)

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build)
- Severity: P3 — design evidence for app-kits 0520 (wizard flow); app
  convention holds
- Class: capability gap (evidence)

## Context
The gateway console's wizard advances steps with a key. The obvious
binding (`]` next / `[` back, root-level shortcuts) is DEAD exactly
where the wizard starts: the connection step autofocuses its URL
`TextInput`, the input consumes every plain character at target phase,
and root shortcuts (bubble) never see them. Global `Actions` don't help
either — they run LAST, only for keys nothing consumed
(`src/app/driver.rs:742-751`, deliberate). So a first-run user pressing
the documented next-step key types `]` into the URL field. Digits for
browse-tab jumps have the same hole whenever any field is focused.

This is not a bug — input-owns-typing is correct — but every
wizard/tabbed app on the engine will re-discover the same conclusion:
**plain-char navigation cannot coexist with focused text inputs; the
nav lane must be modifier chords (or non-char keys) end to end.**

## Current code reality
- `src/ui/tree.rs:684` (0.2.8): shortcuts fire only for keys the
  focused widget did not consume; `TextInput` consumes plain chars.
- No engine-level vocabulary exists for "step/tab navigation keys" —
  each app picks its own, and picks wrong first (this one did).

## Repro
Root shortcut on `Key::Char(']')` + an autofocused `TextInput`
anywhere on screen → press `]` → the character inserts, the shortcut
never fires.

## Workaround in the field (delete when 0520 ships a convention)
The console binds Ctrl+N / Ctrl+P (0x0E/0x10 — legacy-wire-safe
control chords) as the always-available lane, keeps `]`/`[` as
alternates for table/button focus, and prints the chord in the footer
hints. The ask for the 0520 wizard kit: ship the navigation-chord
convention as part of the kit (with the footer-hint wording), so the
next wizard app doesn't burn the same hour, and document the
"plain chars die in inputs; Actions run last" interaction in one place.

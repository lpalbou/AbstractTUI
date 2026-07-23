# 1000 — Dead-keys WINDOW when a modal's only focusables mount after an async load

- **Status**: proposed (field report — abstractgateway-console 0.2.0, live pty repro)
- **Severity**: P1 usability class (silent, looks like a wedge)
- **Engine**: abstracttui 0.2.9
- **Relation**: extends first-app **0230** ("modal content shortcuts are
  dead until focus enters the modal tree") with the ASYNC variant and a
  live diagnosis trail.

## The trap, as walked live

A form modal whose widgets are built inside a `dyn_view_scoped` region
over an async `Loadable` slot (worker read → Ready → region renders the
MultiSelects/TextAreas) has NO focusable content at mount — and putting
`.autofocus()` inside the regenerating region is the 0220 panic hazard,
so the app author leaves it off. Result: **every key except Tab is
dead** — including the modal root's own Esc shortcut and the app-root
Ctrl+L. The pty transcript reads exactly like a frozen app: process
alive, zero bytes out for Esc/arrow/Ctrl+L, no repaint. Only Tab (focus
traversal bootstraps focus into the tree) revives it.

Diagnosis cost: three pty probes to distinguish "wedged event loop"
from "keys dispatch nowhere" — the failure has NO visible signature.
Two modals shipped with the trap (MultiSelect grants editor, TextArea
overlay editor); a third (async table) had a dead-keys window until its
load completed.

## The working fix (0230's pattern, applied at the content root)

`.focusable().autofocus()` on the modal's OUTER content element (built
once in the modal scope, never inside a region) — safe on the modal's
fresh-tree mount path, gives the layer a focus owner from frame one,
and Esc/shortcuts work during the loading window too.

## Asks (either closes the class)

1. **Modal grows a focus fallback**: when a modal layer opens with no
   focused element, focus the layer root implicitly (or dispatch
   layer-root shortcuts when focus is empty). The 0230/0240/0250 wave
   shows every first app hits some variant of this; a structural
   default beats N app-side workarounds.
2. Failing that, a **loud debug assist**: when a key dispatches into a
   layer with no focus owner and dies, emit a one-time stderr note
   ("layer has no focused element — keys are inert; give the content
   root .focusable().autofocus()"). The silent variant cost a night
   hour; a one-line hint makes it a ten-second fix.

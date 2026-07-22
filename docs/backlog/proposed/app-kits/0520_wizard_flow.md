# 0520 — Wizard flow: stepped container with validation gates + apply

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: 0510 (form kit — steps are its pages). Cross-band:
  control-plane **0340 (Persist)** for crash-resume — this wizard is
  0340's FIRST CONSUMER, accepted from PLATFORM's cycle-1 ask
  (reviews/study/platform-cycle1.md "Asks"; recorded in
  reviews/study/appkits-on-extensions.md "Cross-band resolutions").
  Resume works only where 0340 exists; without it the wizard is
  session-only (no degraded half-persistence).
- Validator (0590): `examples/setup_wizard` (crash-resume is part of
  its acceptance journey once 0340 lands).
- Promotion trigger: 0510 landing (the wizard is its first multi-page
  consumer), or a dogfood app growing a first-run/setup flow.

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none — this item deliberately stays INSIDE the cycle-7
  router ruling (`src/ui/compose.rs:136-155`: "deliberately NO router
  type... Page switching IS a signal + Dyn"); the wizard is that
  pattern with chrome and gates, not a router.

## Context
Reference UI B is the direct evidence: multi-step setup with Back/Next,
one major step at a time, per-step field validation, provider/base-URL/
API-key entry, and a final apply. The class is wider than installers:
first-run configuration, connection onboarding (gateway URL → auth →
verify), destructive migrations with a confirm/summary step, and any
"answer these things in order, then commit once" flow. Every terminal
product eventually ships one, and each hand-rolls the same four
mechanics: step state, gated advancement, step chrome (where am I /
what remains), and the summary/apply step that shows what will happen
before it happens. The AbstractFramework installer wizards (the
maintainer's other products) demonstrate the exact same shape twice
over — evidence of a class, not a target.

## Current code reality
- **Page switching is solved and stays the substrate**: signal + 
  `dyn_view_scoped` gives each page a scope that dies on navigation
  (compose.rs:141-155). Naive use DESTROYS page state on Back — the
  documented rule "keep state that must survive switches in signals
  OWNED OUTSIDE the panel builder" (`src/widgets/tabs.rs:5-8`) is
  exactly the wizard's data model: step VIEWS are disposable, step
  VALUES live in the wizard-scope form state (0510 fields).
- **`Tabs` is the nearest widget and the wrong one**: free navigation
  (Left/Right/click, tabs.rs:119-140), no gating concept, and its lazy
  panels dispose on every switch (tabs.rs:214-221) — right pattern,
  wrong policy; a wizard forbids forward-jumping past invalid steps.
- **Gating ingredients ship in 0510**: per-step `form_valid` memos,
  touched-flipping on a refused Next, first-error focus. `Button::
  disabled` (src/widgets/button.rs:75-81) carries the Next gate.
- **Chrome ingredients exist**: `Block` panels, `Separator`, `Badge`
  tones for step states (`src/widgets/badge.rs:22-30`), `Progress` for
  a fraction bar; `Modal` if the wizard runs as an overlay
  (`src/app/popups.rs:39-92` — focus trap + the 0240 fixed-row floors,
  which a wizard's fixed Back/Next row relies on,
  popups.rs:48-56/123-134).
- **What does not exist**: a step-list rail, a step model with
  visited/valid/blocked semantics, Back/Next policy, or a summary/apply
  composition. No example demonstrates a stepped flow (examples/:
  dashboard, feed, gallery, transcript, widgets... — none stepped).

## Problem
The wizard class re-derives step-state machines on every product, and
the two failure modes are always the same: state loss on Back (page
scope disposal eats the user's entries) and dishonest gates (Next
enabled while a step is invalid, or validation firing before the user
touched anything). The engine owns the primitives; nobody owns the
policy.

## What we want
A `Wizard` composition (a kit component, not an engine privilege):
1. **Step model**: `WizardStep { id, title, build: fn(Scope) -> View,
   valid: Memo<bool>, optional }` — steps declared once; wizard state =
   `current: Signal<usize>` + per-step `visited` flags. Values live in
   signals created in the WIZARD's scope and passed into step builders
   (the tabs.rs:5-8 rule made structural — page disposal can never eat
   data).
2. **Navigation policy**: Back always allowed (never gated, never
   destructive); Next gated on the current step's `valid` (a refused
   Next flips the step's fields touched — 0510 — and focuses the first
   error); direct jump via the step rail only to VISITED steps. All
   movement through one `go(target)` fn so policy lives in one place;
   `on_step_change` callback out.
3. **Chrome**: a step rail (vertical list left, or compact `1/5 ·
   title` header line under a width threshold — the wizard picks by
   measured width, honestly, no magic breakpoints in app code) showing
   per-step state: done `ok` tone / current `accent` + selection pair /
   blocked `text_faint` (tones from badge.rs:22-30; state table
   docs/theming.md:277-288). A fixed bottom row: Back / Cancel /
   Next-or-Apply buttons — `shrink(0.0)` semantics ride the 0240
   defaults so an overflowing step body can never crush the controls.
   Step bodies overflow inside a `Scroll` (the api.md:199-222 modal
   recipe).
4. **Summary/apply step**: a helper rendering declared `(label, value,
   step_index)` rows — each row activatable to jump back to its step —
   above an Apply button wired to `on_apply`. Apply-in-progress state
   (disable controls + `Spinner`) is the app's signal; the wizard just
   renders it. The wizard never executes effects — it collects and
   confirms; the app applies.
5. **Lifecycle**: `on_cancel` (Esc routes here when the wizard owns
   focus; confirm-discard is the app's modal to raise); completion
   fires `on_apply` exactly once per activation (gated while the app's
   in-progress signal is true).
6. **Placement-agnostic**: works full-screen (a page in the signal+Dyn
   sense) and inside a `Modal` — the wizard is a `View`; it acquires no
   overlay of its own.
7. **Crash-resume via Persist (0340, first consumer)**: the wizard
   registers ONE 0340 key (`wizard.<id>`): `read_fn` samples step
   values + `current` + visited flags in one struct read (possible
   precisely because §1 keeps values in wizard-scope signals — page
   disposal never owns data), per-key `u8` version = the wizard's own
   schema version (steps changed ⇒ bump ⇒ `write_fn` migrates or
   refuses per 0340's contract). Restore policy is the app's: 0340's
   load report (`CrashDetected` + per-key outcome) drives a
   "Resume setup?" modal; the wizard exposes `from_restored(bytes)` /
   `snapshot_bytes()` and owns NO storage/format code. Ergonomics
   feedback DELIVERED to 0340 (their cycle-1 ask; accepted — their
   item now cites this section): consume-once semantics on the
   restored handle (`take(key)`), so a declined resume cannot be
   re-applied by a later reader.

## Scope / Non-goals
Scope: step model + policy, rail/header chrome, footer controls,
summary/apply helper, Esc/cancel wiring, the 0340 key integration
(§7: sample/restore functions + version, no storage code), gallery
example, docs section in `docs/forms.md` (0510's page — one doc for
the form story).
Non-goals: branching/conditional step graphs (v1 is linear; a `skip`
predicate per step covers the common conditional — full graphs need
evidence); persistence MACHINERY (formats, atomicity, triggers, crash
markers — all 0340's, control-plane band 0300–0390; the wizard is its
consumer: one registered key, §7); async step loading; animations.

## Expected outcomes
A five-step setup flow is ~50 lines of step declarations + 0510 field
rows. Back never loses data; Next is honest; the summary step is one
helper; both reference wizards (installer-style and connection-setup)
map onto it without app-side state machines.

## Validation
- Unit: gate policy (Next refused flips touched + stays; Back always
  moves; jump only to visited); values survive Back/forward
  round-trips (the disposal trap as a birth regression test); apply
  fires exactly once under double-Enter.
- CaptureTerm acceptance: full keyboard run of a three-step wizard
  (Tab/Enter only, no mouse) ending in a summary jump-back and an
  apply; narrow-width fallback to the compact header; overflowing step
  body scrolls while Back/Next stay visible (0240 semantics).
- Modal placement: the same wizard inside `Modal::open` traps focus and
  restores it on close.
- Crash-resume (with 0340; validator `examples/setup_wizard`): fill
  three steps, snapshot on 0340's trigger, kill -9 the pty harness
  (0340's crash-simulation rig), restart → load reports
  `CrashDetected`, the resume modal restores all three steps' values +
  current step; a version-bumped wizard schema refuses the stale
  snapshot with a labeled notice, never a silent partial restore.

## Progress checklist
- [ ] Step model + wizard-scope value ownership
- [ ] go() policy (gated Next, free Back, visited-only jumps)
- [ ] Rail + compact header chrome (tone/state mapping)
- [ ] Footer controls with 0240-safe fixed row
- [ ] Summary/apply helper (jump-back rows, once-only apply)
- [ ] 0340 key integration (sample/restore + schema version) +
      crash-resume acceptance (gated on 0340 landing)
- [ ] Gallery example + keyboard-only acceptance run

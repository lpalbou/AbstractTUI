# Wave 5 acceptance charter — decision-gate component (multiple choice + Other)

REVIEWER document. Written 2026-07-23, BEFORE reading BUILDER's widget
source (verified: no gate/choice component existed in `src/` or `tests/`
at write time). Derived from the maintainer brief — "a nice
widget/component to ask questions with multiple choices + other
category… when we need to gate a decision behind that component" — and
the engine's own recorded laws. Every clause is testable; each carries
the law it descends from. `tests/wave_choice_review.rs` pins these
clauses, not BUILDER's implementation details.

Severity vocabulary: **MUST** = acceptance blocks without it.
**SHOULD** = accepted with a filed debt. **NICE** = recorded, never
blocks.

Sources of law cited below:

- The **0250 ruling** (selection vs activation),
  `reviews/study/platform-on-appkits.md` §"The 0250 ruling";
  encoded in `src/widgets/list.rs:3`.
- The **0297 disposal-safety law** (all widget bookkeeping lands BEFORE
  user callbacks; a callback may dispose the widget's scope
  synchronously), pinned per-widget: `src/widgets/button.rs:361`,
  `src/widgets/radio.rs:217`, `src/widgets/textarea_disposal_tests.rs`.
- **Modal laws**: 0220 (no `.autofocus()` inside dyn_view regeneration),
  0230 (modal content shortcuts dead until focus enters — content roots
  need `.focusable().autofocus()`), 0240 (declared fixed rows are a
  floor — buttons never silently squeezed to zero; `src/app/popups.rs:54`),
  atomic modal replacement (2026-07-22: deferred close + equal-z oldest-
  wins dispatch can leave an invisible key-eating layer; an effect that
  opens a modal mid-replace must not be dropped).
- **Role enum frozen until 0.3** (`src/ui/access.rs:51` SEMVER note):
  widgets reuse the closest existing role honestly (the Select precedent
  reports `Button` + choice as access value).
- **Contrast audit** (`src/theme/registry.rs` — every registered theme
  passes floors incl. `SELECTION_TEXT`; `src/theme/register.rs` runs the
  same audit on user themes). The guarantee only reaches a widget that
  actually paints with the audited tokens.
- **Focus-visible guarantee** (`src/ui/access.rs::focus_affordance_visible`,
  DESIGN §3): rendering with focus must differ inside the focused rect.
- **Damage contract** (`docs/design/01-damage-contract.md`): idle frames
  are 0 bytes / 0 allocs, test-pinned (`tests/perf_budgets.rs`,
  `tests/alloc_budget.rs`); parked overlays cost zero wakeups (Toast
  precedent, `src/app/popups.rs`).

---

## G — Gate semantics (the component exists to gate a flow)

- **G1 (MUST) Exactly-once resolution.** Every path that ends the gate —
  Enter on an option, activation click, a confirm button, Esc/cancel —
  delivers exactly ONE outcome through ONE resolve callback. After the
  first resolution, further submit attempts (double Enter, click+Enter
  in one batch, Enter during a deferred close tick) are no-ops: no
  second callback, no panic.
- **G2 (MUST) No silent close.** There is NO path that closes the gate
  without delivering an outcome. Dismissal (Esc, cancel button), if the
  gate permits it, IS an outcome (an explicit `Cancelled`/`None`-shaped
  variant) delivered through the same callback. Rationale: the consumer
  is a suspended flow; a close that skips the callback hangs the flow
  forever (the abstractcode-tui approval lane releases the composer only
  from the resolve path).
- **G3 (MUST) Dismissability is a knob.** A gate can be declared
  non-dismissable (a decision that must be taken): Esc then either maps
  to a named option (e.g. Deny) or visibly refuses — it never
  silently closes and never silently swallows. Default may be
  dismissable, but the knob must exist for approval-shaped consumers.
- **G4 (MUST) Disposal-in-callback safe (0297).** The resolve callback
  runs LAST: all component signal writes/bookkeeping complete before it,
  so the callback may synchronously close the modal and dispose the
  component's scope — the natural "choose → close → continue flow"
  shape. Pinned the same way every 0297 widget pins it.
- **G5 (MUST) Selection is not commitment (0250 clauses 1–3).**
  Arrow/Home/End movement changes selection only; commitment requires
  an explicit activation (Enter always; click on already-selected
  activates, click on unselected only selects). No commit-on-move
  default.
- **G6 (MUST) Re-openable.** After resolve + close, a fresh gate opens
  and works (state does not leak across instances). Opening the next
  gate from INSIDE the previous gate's resolve callback must not be
  dropped or leave a key-eating layer (atomic-replacement law).
- **G7 (SHOULD) Outcome carries identity, not just an index.** The
  outcome names the chosen option stably (id/key or the option value),
  so consumer code doesn't decode positions that can be re-ordered by a
  cycle-2 restyle. (SHOULD not MUST: index+options-slice is workable,
  but fragile.)

## O — The "Other" contract

- **O1 (MUST) Other reveals a text field.** Choosing the Other option
  reveals an editable single-line field; the field does not exist in
  layout or the a11y tree before Other is selected (no phantom
  focusable).
- **O2 (MUST) Keyboard routing while typing.** While the Other field is
  editing: printable characters, Backspace, and HORIZONTAL caret keys
  (Left/Right/Home/End) belong to the FIELD — they edit the draft, they
  must never move the option list or leak to shortcuts underneath.
  Up/Down return to option-list navigation (selection may move off
  Other). **Position + defense:** the field is single-line, so vertical
  arrows are dead keys inside it; giving Up/Down to the list matches
  (a) the WAI-ARIA radiogroup idiom (arrows move within the group),
  (b) this engine's own Combobox precedent — typing goes to the editor
  while vertical arrows walk the option rows (`src/app/select_combobox.rs`,
  pinned in `tests/wave_r2_review.rs`), and (c) what a terminal user
  expects: Up/Down as the escape hatch back to the choices, Tab
  reserved for the modal's focus cycle. A caret-owning Up/Down would
  buy nothing (no second line to move to) and cost the cheap way back.
- **O3 (MUST) Draft survives excursions.** Text typed into Other is not
  erased when selection moves away and back within one gate lifetime.
  A cancelled gate discards the draft with the scope (no bleed into the
  next instance — G6).
- **O4 (MUST) Empty-Other commit refuses, visibly.** Committing Other
  with an empty (or whitespace-only, after trim) draft REFUSES: the
  gate stays open and shows a visible hint; it does not resolve, does
  not cancel, and does not deliver an empty custom answer.
  **Position + defense:** the component gates a decision — an empty
  "Other" is a non-answer. Resolving-as-cancel betrays the user's
  intent (Enter meant *submit*); resolving with `""` exports the
  validation burden to every consumer of a gate component whose whole
  point is that the flow beyond it can trust the answer. Refusal must
  be visible (the engine's honesty law: no silent swallow) — a hint
  line, not a beep into the void.
- **O5 (SHOULD) Other commit path is uniform.** Enter from inside the
  field commits (G1's exactly-once applies); the committed outcome is
  distinguishable as Other-with-text vs a listed option.

## A — Accessibility (the frozen-Role honesty clause)

- **A1 (MUST) The question is in the tree.** The a11y snapshot
  (`app.tree().accessibility_tree_text()`) contains the question text —
  as the `Dialog` label, a `Heading`, or a `Text` leaf. A screen reader
  that cannot say the question cannot gate a decision.
- **A2 (MUST) Options are enumerable with selection state.** Every
  option label appears; the current selection is readable from the
  snapshot (role `RadioGroup` with value, or `List`/`ListItem` with the
  selected item exposed via value/focus). Multi-select variants (if
  shipped) expose per-item checked state (`Checkbox` role).
- **A3 (MUST) Honest role mapping only.** Roles come from the frozen
  vocabulary (`Dialog`, `RadioGroup`, `List`/`ListItem`, `Menu`/`MenuItem`,
  `Checkbox`, `Input`, `Button`, `Heading`, `Text`) — no `Generic`
  spam, no role that lies (the Select-as-Button-with-value precedent is
  the model). Adding a Role variant is a semver break and is NOT
  available to this wave.
- **A4 (MUST) The revealed Other field is `Input` and reachable.** Once
  revealed it appears in the snapshot; while editing, the focused entry
  is the field (focus truth in the tree, not just pixels).
- **A5 (MUST) Focus affordance visible.** With focus anywhere in the
  gate, `focus_affordance_visible` holds (DESIGN §3) — selection/focus
  must be visible, not merely internal state.

## K — Keyboard-first completeness

- **K1 (MUST) Everything without a mouse.** Full path — navigate
  options, reveal Other, type, commit, cancel — with keys alone,
  through the modal focus trap (Tab cycles inside; keys never leak to
  the app below while open — `src/app/popups.rs` §focus-trap).
- **K2 (MUST) Keys are discoverable.** The gate documents its keys
  (visible hint row, or the widget's doc header at minimum — the
  engine's documented-keys convention; `keymap_help.rs` precedent).
- **K3 (MUST) Focus lands correctly on open (0220/0230).** The gate is
  operable immediately: first key after open moves the selection — no
  dead-keys-until-Tab (0230), and no `.autofocus()` inside a dyn_view
  regeneration path (0220 panic law). Deferred `focus_first` is the
  sanctioned shape.
- **K4 (SHOULD) Esc semantics are layered.** While editing Other, Esc
  first exits the field back to the list (one level up), not straight
  to gate-cancel; a second Esc cancels (if dismissable — G3).
  Position: mirrors the engine's popup idiom (Esc closes the innermost
  owned surface first) and protects a half-typed draft from a reflex
  Esc.
- **K5 (NICE) Quick-select digits.** `1`–`9` jump/commit for short
  lists (consumer ask; see consumer-fit doc).

## S — Honesty at scale

- **S1 (MUST) 30 options scroll.** More options than the panel's rows:
  the list scrolls (List viewport precedent), the selected option is
  always scrolled into view, and navigation reaches every option — no
  option unreachable, none silently clipped.
- **S2 (MUST) The prompt wraps.** A long question at narrow width wraps
  (or truncates VISIBLY with ellipsis — never silent glyph clipping
  mid-word at the panel edge).
- **S3 (MUST) Fixed rows survive overflow (0240).** Buttons/hint/title
  rows keep their declared height under option-list overflow pressure —
  the 0240 floor inside Modal; the component must not opt its fixed
  rows out of it.
- **S4 (SHOULD) Long option labels degrade honestly.** An option wider
  than the panel truncates with a visible marker (`…`), and the a11y
  tree still carries the FULL label (pixels may truncate; semantics may
  not).

## T — Theming

- **T1 (MUST) Tokens only.** No literal colors in the component source;
  every paint goes through `TokenId` (Overlay ground, Text/TextMuted
  ink, SelectionBg/SelectionFg or an equally audited pair for
  selection, BorderFocus for focus, Error for danger tints). This is
  what makes T2 free.
- **T2 (MUST) Selection distinguishable in EVERY theme.** In all
  registered themes, the selected option's cells differ from an
  unselected option's cells (color or attribute or marker glyph). Using
  the audited selection pair inherits `SELECTION_TEXT` floors; a marker
  glyph (`>` / `●`) additionally survives monochrome. Either passes;
  invisible selection in any one theme fails the wave (the
  contrast-audit precedent: the audit is engine law, not a suggestion).
- **T3 (SHOULD) Danger tint rides `Error`.** If a danger option
  variant ships (consumer ask), its tint is the `Error` token — already
  contrast-audited per theme.

## P — Performance / damage contract

- **P1 (MUST) Zero idle cost while open.** Gate open, no input: driver
  turns go idle with zero output bytes — no polling timers, no
  animation loops for a static panel (Toast's parked-costs-nothing
  precedent; pinned the way `alloc_budget.rs`/`perf_budgets.rs` pin
  idle).
- **P2 (SHOULD) Interaction damage is local.** An arrow-key move
  repaints the affected rows, not the world (damage-contract spirit;
  measured via the existing damage instrumentation if practical, else
  recorded as a review observation).

---

## Verification map

| Clause | How it gets checked |
| --- | --- |
| G1, G2, G5, G6 | `tests/wave_choice_review.rs` (Driver + CaptureTerm, count callbacks) |
| G3, G4, G7 | review test where API permits; else review-cycle finding |
| O1–O5 | `tests/wave_choice_review.rs` (type into revealed field, arrow routing, empty commit) |
| A1–A5 | a11y snapshot text assertions (`accessibility_tree_text`) |
| K1–K4 | pure key-driven test paths; K2 by doc/render inspection |
| S1–S3 | 30-option + narrow-width renders |
| T1 | source inspection (no `Rgba`/hex literals) — review finding |
| T2 | render under ≥2 contrasting registered themes, diff selected vs unselected cells |
| P1 | settle → assert idle turn + zero bytes written |

Clauses not coverable through the landed public API in cycle 1 are
written as `#[ignore = "pending builder API"]` skeletons with exact
intent comments, and listed as uncovered in `review-cycle1.md`.

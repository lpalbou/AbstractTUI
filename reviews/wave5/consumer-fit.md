# Wave 5 consumer fit — what the field actually needs from the decision gate

REVIEWER document, 2026-07-23. Evidence from the first real consumer
(abstractcode-tui, read-only at `~/tmp/abstractframework/abstractcode-tui`)
and the named second validator (gateway-config wizard,
`docs/backlog/planned/ports/0215_gateway_config_wizard_app.md`).
Each item is marked **MUST / SHOULD / NICE** for BUILDER's cycle 2.
Charter clause references in parentheses.

## The existing workaround (what we are replacing)

abstractcode-tui hand-rolls this exact component twice:

1. **`Picker`** (`src/ui/modals.rs:554` — title + `List` + optional
   hint row + `on_selection` / `on_choose` / `on_cancel`, caller-computed
   `Size`). Used for theme picker (live preview on movement, Esc
   reverts), model/workflow pickers. Its doc header is a compressed spec
   of the 0250/0297 laws: "arrow movement only browses… the engine
   completes ALL of its bookkeeping BEFORE the callback, so `on_choose`
   may close the modal — or replace it with the next stage —
   synchronously."
2. **The tool-approval modal** (`src/ui/modals.rs:295-455`) — a
   three-action decision gate (approve / approve-all / deny) with
   single-letter shortcuts (`a`/`A`/`d`), a details toggle (`f` = full
   JSON), a scrollable body of per-call cards, a DYN status row (tier
   honesty line), and an Esc that DEFERS — explicitly not deny: "a
   dismissal must never tell the model 'denied'" (modals.rs:378-380).
3. **Ask-user** (`src/ui/modals.rs:461`) — prompt + free-text input:
   the degenerate "Other-only" gate.

## Findings for BUILDER (cycle-2 fold list)

### Resolution shape

- **C1 (MUST)** Outcome vocabulary distinguishes *chosen option*,
  *Other(text)*, and *dismissed* — three different consumer meanings.
  The approval flow proves dismissal ≠ any option: Esc defers (run
  keeps waiting durably) while `d` denies. A gate that can only say
  "option i or nothing happened" cannot host the approval flow.
  (Charter G2/G3.)
- **C2 (MUST)** Resolve callback may close/chain synchronously: the
  consumer's `on_choose` "may close the modal — or replace it with the
  next stage" — multi-question AskQuestion-style forms (N questions,
  each options[] + allow_multiple + Other) will chain gate→gate from
  inside the callback. Requires 0297 bookkeeping-before-callback and
  the atomic-replacement law to hold. (Charter G4/G6.)
- **C3 (MUST)** Preselect/default: `Picker.start` exists because every
  real picker opens on the CURRENT value (theme, model). The gate needs
  an initial-selection knob; rendering the default/recommended option
  first is the consumer's own ordering decision — the widget must not
  reorder. (Charter G7: outcomes by stable identity, so consumer
  ordering is free.)

### Option affordances

- **C4 (MUST)** Per-option shortcut letters: the approval modal binds
  `a`/`A`/`d` and prints them in the labels ("approve (a)") + hint row.
  The gate should let an option declare a shortcut char and render it —
  otherwise the first consumer re-wraps the widget in a shortcut
  Element and half the component's value evaporates. (Charter K2.)
- **C5 (SHOULD)** Danger-tinted option: deny/"delete"/destructive
  choices want the `Error` token tint (contrast-audited per theme).
  (Charter T3.)
- **C6 (SHOULD)** Per-option detail line: approval cards carry a
  one-line intent summary under the headline ("write src/main.rs");
  0215's routes want "applies now: …" resolution lines. One optional
  muted detail row per option covers both. (Charter S4 honesty applies:
  detail truncation visible, full text in a11y value.)
- **C7 (SHOULD)** Selection-movement observer distinct from resolve
  (`on_selection` in the workaround — the theme picker's live preview
  rides it). Must stay a notification, never a commitment (0250 clause
  1); if BUILDER exposes it, document it with the consumer's own
  warning.
- **C8 (NICE)** Digit quick-select `1`–`9` for short lists. (Charter K5.)

### The Other field

- **C9 (MUST)** Other resolves as text distinct from options (C1) and
  refuses empty commits visibly (charter O4) — in the approval flow a
  future "deny with reason" IS Other-with-text; an empty reason
  resolving as a bare deny would silently drop the operator's intent.
- **C10 (SHOULD)** Other label is configurable ("Other…", "deny with
  reason…", "custom model…") — the category name varies per consumer.

### Multi-select (allow_multiple)

- **C11 (SHOULD)** AskQuestion-style forms carry `allow_multiple`
  questions. If the gate ships single-select-only in cycle 1, the API
  must not preclude a multi variant (outcome type already lists vs
  scalar; Space reserved for marking per the 0250 ruling clause 2 —
  Space must NOT activate in any selection widget that may grow marks).
  Shipping multi in cycle 2 is acceptable; painting the API into a
  single-select corner is not.

### Modal ergonomics

- **C12 (MUST)** Auto-sizing with honest floors: every workaround call
  site hand-computes `modal_size(56, (labels.len()+8).min(26))`. The
  gate should derive its height from content (options.min(cap) +
  fixed rows) within the viewport, keeping the 0240 fixed-row floor.
  (Charter S1/S3.)
- **C13 (MUST)** Non-dismissable mode (charter G3): 0215's wizard has
  per-step validation gates — "apply or go back", never
  Esc-into-limbo. The approval flow instead WANTS Esc-as-defer. Both
  exist in the field; the knob is proven needed by two consumers
  pulling opposite directions.
- **C14 (SHOULD)** A status/context row slot (the tier honesty line is
  a dyn row above the body; 0215 wants "applies now" lines). One
  optional caller-supplied row covers it — cheaper than consumers
  rebuilding the modal to add one line.
- **C15 (NICE)** A details toggle slot (`f` full JSON precedent) —
  probably out of scope for the widget proper; record as a composition
  recipe in the example instead.

## The fabricated-selection law (0215 import, MUST-class)

The gateway console's recorded defect (2026-07-17): a combo populated
from a catalog with index 0 preselected PRESENTS the first entry as
configuration the user never made. For the gate: **no implicit
default** — if the consumer does not set an initial selection, the gate
must not resolve to "first option" via a bare Enter on an untouched
list unless the first option is legitimately selected-and-visible as
such (selection IS visible per charter T2/A2, so an honest default
selection is fine; a hidden one is not). Concretely: initial selection
must be rendered the moment the gate opens (charter A2/T2), so Enter
can never commit an invisible choice. (Charter K3 covers
first-key-works; this covers first-ENTER-is-honest.)

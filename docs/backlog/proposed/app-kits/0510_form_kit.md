# 0510 — Form kit: field rows, form state model, submit gating

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: 0500 EMBEDS in field rows but does not block this item
  (TextInput/Checkbox/RadioGroup rows work day one).
- Validator (0590): `examples/admin_console` (the masked API-key edit
  form) + `examples/setup_wizard` (every step is this kit's page).
- Engine deltas (named per PLATFORM cycle-2 F7 — this item is NOT
  self-contained): (a) a subtree-scoped focus step for
  `enter_advances` — no such API exists today (`focus_next()` is
  tree-global, src/ui/focus.rs:28; `focus_next_in(dir)` is spatial,
  focus.rs:297): a new `ui::focus` public API, sequenced under 0170's
  budget; (b) `TextInput` masked mode — an additive builder on the
  shipped widget.
- Promotion trigger: the wizard item (0520) starting (it is one page of
  this kit per step), or any dogfood app building its second settings
  form (the first proves the pattern, the second proves the copy).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: one candidate ADR if the design debate demands it: form
  state ownership — plain signals-in-a-struct (the endorsed store
  pattern, `src/ui/compose.rs:56-85`) vs. a managed `FormState` with
  field registration. This item proposes the former with thin helpers;
  if review pushes toward a registry, write the ADR first.

## Context
Reference UIs A, B, and D are form-dense: the admin console's Edit/
Configure/Override panels (base-URL + API-key + provider/model rows,
each with validation), the wizard's per-step field pages, and the
smart-note capture surfaces. The chat composer's settings drawer is the
same shape. The engine's own docs teach the ingredients — a label/field
track grid (`docs/api.md:117-122`) and memo-per-rule validation
(`src/ui/compose.rs:100-134`) — but deliberately ship no form
machinery: "There is deliberately no `Form` type in v1: validation is a
MEMO over the field signals" (compose.rs:102-104). That was the right
v1 restraint. The study brief's evidence says the pattern is now copied
in every config UI, and each copy re-decides the same six things: row
layout, error placement, help text, dirty tracking, submit gating, and
keyboard flow between fields. A kit — components + conventions, not a
framework — removes the copying without betraying the signals model.

## Current code reality
- **Controls exist; rows do not.** `TextInput` (single-line,
  `on_change`/`on_submit`, `src/widgets/input.rs:35-41`), `Checkbox`
  (`src/widgets/checkbox.rs:20-26`), `RadioGroup`
  (`src/widgets/radio.rs:23-28`), `Button` with `disabled`
  (`src/widgets/button.rs:75-81`) are shipped; 0500 adds selects; 0120
  adds the multiline TextArea. There is no label+control+error+help row
  component anywhere — the dashboard and every example compose ad hoc.
- **The validation pattern is documented, unpackaged**: one memo per
  rule returning `Option<String>`, a `form_valid` memo folding them,
  disable-on-invalid via `dyn_view_scoped` rebuilding the button
  (compose.rs:107-134). Correct, and ~20 lines of ceremony per form
  that the kit should absorb.
- **No masked input**: `TextInput` has no secret/password mode — the
  struct holds only value/placeholder/layout/callbacks
  (input.rs:35-41; grep for mask/secret matches only a borrow-note
  comment at input.rs:302-305). API-key fields (reference UIs A and B)
  need masked rendering with a reveal toggle; the editing model
  (ClusterMap) is unaffected — only the draw substitutes `•` per
  cluster.
- **Focus flow exists at the primitive level**: `focusable()`, Tab
  traversal, `autofocus` (last-mounted wins, `src/ui/view.rs:203-227`),
  `focus_signal` (view.rs:279-280). What is missing is form semantics:
  Enter on a non-final field advancing to the next (today Enter fires
  `TextInput::on_submit` per field), and scroll-to-first-error.
- **Layout is solved**: the label/control grid is one `Grid::new`
  (`src/widgets/grid.rs:34-46`) or the api.md track-grid recipe; the
  kit standardizes the tracks, not the solver.
- **Theming tokens for states are audited**: `error`/`warn`/`ok` inks
  (docs/theming.md:50-53), `text_muted` for help text, `text_faint`
  placeholders (docs/theming.md:28-34) — the row's visual grammar needs
  zero new tokens.

## Problem
Every form re-implements row layout, error/help placement, dirty and
valid tracking, gated submission, and Enter-to-advance — and drifts.
API-key entry cannot be built honestly at all (no masked input). The
memo pattern is sound but has no home, so consoles copy it with local
mutations until two forms in one app disagree about where errors render.

## What we want
Components + one state convention, no framework:
1. **`FieldRow`**: label (fixed track) + any control `View` + a
   reactive message line under the control (error in `error` ink, else
   help in `text_muted`, else empty — one line reserved or `Auto`
   height, the row never jumps horizontally) + optional required
   marker. Grid tracks standardized (`Track::Cells(label_w)`,
   `Track::Fr(1.0)` — the api.md:117-122 recipe canonized). A
   `FieldRowProps { label, control, error: Memo<Option<String>>, help,
   required }` shape following the compose.rs props convention.
2. **Field state helpers**: `field(cx, initial)` returning a small
   struct of signals — `value`, `touched`, plus `error` built from a
   rule closure — so "one memo per rule" stops being hand-rolled.
   Errors show only after `touched` (blur or first submit attempt) —
   the standard don't-yell-while-typing rule; a form-level "validate
   now" flips all fields touched.
3. **Form fold**: `form_valid(&[...])` and `form_dirty(&[...])` memo
   folds over field structs; `Submit` composition = `Button` with
   `.disabled(!valid)` inside the documented `dyn_view_scoped` rebuild
   (compose.rs:133-134), packaged.
4. **Keyboard flow**: an opt-in `enter_advances` wrapper: Enter on a
   non-final field moves focus to the next focusable in the form
   subtree (the metadata's engine delta (a) — no subtree-scoped step
   exists today); Enter on the final field submits. Esc semantics stay
   app-owned. Scroll-to-first-error on gated submit: NO engine delta —
   `Scroll` does not follow focus (corrected cycle 4: scroll.rs:3-9
   only guarantees focus SURVIVES scrolling; nothing scrolls a focused
   element into view), but the form owns its `Scroll` and its rows are
   fixed-height, so the kit computes the erring row's y and writes the
   bound `offset_y` signal (`src/widgets/scroll.rs:57-68`) before
   focusing — kit-side math, pinned by the acceptance test.
5. **Masked TextInput mode**: `.masked(bool_signal)` on `TextInput` —
   draw substitutes one `•` per cluster (width honesty preserved),
   selection/cursor math untouched, paste unaffected; a reveal toggle
   is just the signal. Explicitly a `TextInput` extension, not a fork.
   **Masking covers the SEMANTIC surface too (PLATFORM cycle-2 F2)**:
   `TextInput` exposes its raw value via `access_value`
   (src/widgets/input.rs:210 — `value.get_untracked()`), and the
   accessibility snapshot is exported off-process by the control-plane
   band (automation bus / wire protocol / MCP). While masked, BOTH the
   draw AND `access_value` substitute (`•` per cluster, or a
   `"(hidden)"` marker); the reveal toggle restores both together —
   redaction at the widget, never left to a downstream filter.
6. **Docs**: a `docs/forms.md` page superseding the compose.rs cookbook
   section by reference — the kit is the packaged version of that
   exact pattern, and the doc says so.

## Scope / Non-goals
Scope: FieldRow, field/fold helpers, enter-advance + first-error focus,
masked input mode, docs page, gallery form.
Non-goals: schema/derive macros; async validation orchestration (an
app validates in its own effects and writes the error signal); layout
DSLs; a reducer/store framework (compose.rs's signals-as-store stance
stands); IME/locale input concerns (same posture as TextInput,
input.rs:13-15).

## Expected outcomes
A settings panel is N `FieldRow`s + one packaged submit gate — the six
per-form decisions become zero. 0520 builds steps as forms with no new
state model. API-key fields render honestly masked. The compose.rs
pattern remains the truth; the kit is its packaging (no second model).

## Validation
- Unit: error visibility gating (untouched → hidden, touched → shown,
  submit-attempt flips all); valid/dirty folds; masked draw substitutes
  clusters 1:1 (ZWJ emoji = one `•`) while edits stay cluster-atomic.
- Secret-leak test (not a rendering test): with a masked field
  populated, `accessibility_tree()` snapshot text contains NO plaintext
  fragment of the value; reveal-on restores it; reveal-off re-redacts.
- CaptureTerm acceptance: a three-field form — Enter advances, gated
  submit refuses + focuses/scrolls to first error, fix → submit fires
  exactly once; masked field reveals on toggle; theme switch restyles
  error/help lines (tokens only).
- Docs test: the forms page snippet compiles as a doctest.

## Progress checklist
- [ ] FieldRow (grid tracks, message line, required marker)
- [ ] field()/fold helpers with touched-gated errors
- [ ] Packaged submit gating (disable + first-error focus)
- [ ] Enter-advances focus step (subtree-scoped traversal)
- [ ] TextInput masked mode + reveal (draw + access_value together;
      leak test)
- [ ] docs/forms.md + gallery form + acceptance tests

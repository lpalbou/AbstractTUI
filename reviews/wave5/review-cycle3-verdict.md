# Wave 5 — REVIEWER cycle-3 verdict (ChoicePrompt / decision gate)

Date: 2026-07-23. Reviewed state: BUILDER cycle-2 fold landed
(`src/app/choice_prompt{,_view,_parts,_interact}.rs` +
`choice_prompt_tests{,_flows,_c2}.rs`, `tests/wave_choice_prompt.rs`,
`examples/decide.rs`, amended completion report in
docs/backlog/completed/app-kits/0515). Gates at verdict time:
**whole tree 1754 passed / 0 failed / 99 ignored** (58 binaries;
baseline ~1,737 — the ignored set is the usual live/pty family plus
this file's one F10 skeleton), `tests/wave_choice_review.rs` **22
passed / 0 failed / 1 ignored**, clippy zero on the review file,
rustfmt clean, `cargo semver-checks` claim (additive vs 0.2.7) taken
from the completion report and consistent with the API read.

## 1. Skeleton activation results (16 → 15 active PASS, 1 blocked with a real finding)

All cycle-1 skeletons (g1 g2 g3 g4 g6 g7 · o1 o2 o3 o4 · a3 · k2 k3 ·
s1 s2 s3) were activated against the real API per
`builder-cycle2-notes.md`; cycle-2 additions promised in cycle 1 (K4
layered Esc, T3 danger, O5-distinguishable commits inside o3/o4, the
P2 tail) are in. Three activations initially FAILED and were each
diagnosed at source before adaptation — none was a component defect,
and no clause was weakened:

- **o4** — the refusal note (`interact.rs:254`
  `"{label} needs text — type your answer"`) truncates with a VISIBLE
  ellipsis on panels narrower than the sentence
  (`interact.rs:301-302`). By-design honesty, not a silent swallow.
  The activated test widens the content-measured panel so the FULL
  sentence is pinned (strongest form of the clause), plus
  whitespace-only refusal and the eventual Other-commit.
- **s3** — my "Enter confirms" needle collided with the DOCUMENTED
  whole-segment hint degradation (builder caveat 1;
  `interact.rs:307-314`): at the narrow content width the right-aligned
  `1/30` note leaves room only for the tail segment. The fixed ROWS
  all survived (the actual S3 clause). Activated with two geometries:
  wide panel = every fixed row fully legible under 30-option overflow;
  narrow panel = hint row survives, degrades by WHOLE segments ("Esc
  cancels" intact, no "Esc ca…" mid-word cut — pinning the exact
  regression the builder's completion report says this rule was born
  from).
- **t3** — pure test defect: `locate("Delete")` matched the prompt
  heading "Delete the branch?" before the danger row. Prompt renamed.

One skeleton stays `#[ignore]` — that is **F10** below, a real engine
gap, not a pass-forced weakening.

## 2. Findings

- **F10 — MEDIUM (engine follow-up, not a component defect) — no
  public path to an overlay layer's a11y tree.** `App::tree()`
  returns the ROOT tree only (`src/app/mod.rs:309`);
  `Overlays::store()` and `OverlayContent` are `pub(crate)`
  (`src/app/overlays.rs:281`, `:45`); `LayerHandle::tree()` is public
  (`overlays.rs:674`) but the gate's handle lives privately inside its
  `Modal` (`choice_prompt.rs:344`, `:428`), and `ChoicePromptHandle`
  exposes only `cancel`/`is_open`. Consequence: charter A1–A4's
  TREE-level assertions cannot run from `tests/` — and neither can any
  downstream crate assert its own modals' a11y. The clause substance
  is still pinned in this same `cargo test` gate by BUILDER's in-crate
  unit tests (`choice_prompt_tests_c2.rs:226-262`, `:265-279`,
  `:284-315`), source-verified below. Follow-up for the engine lane: a
  read-only accessor (e.g. an App-level accessibility snapshot folding
  overlay layers, or a public modal-tree query) — then
  `charter_a3_roles_are_honest_and_options_enumerable` activates as
  written.
- **F11 — INFO — refusal-note tail can truncate away.** On panels
  narrower than the note, "«label» needs text — type your answer"
  keeps the label end and loses the instruction tail ("…"). Visible
  and honest; a consumer with a long Other label gets a terse but
  truthful note. Nice-to-have: prefer keeping the instruction tail
  under pressure. Not blocking.
- **Recorded debts carried from cycle 1/2 (none blocking, all on the
  record in 0515):** no selection-movement observer (consumer-fit C7,
  SHOULD — the theme-picker live-preview consumer will name it), no
  caller-supplied status row (C14, SHOULD), no custom button labels,
  `ChoiceSequence` lacks per-question `dismissable`, resize-while-open
  clamps without re-measure (gates must not auto-cancel; needs
  Modal-level support).

## 3. Spot-verification at source (BUILDER's cycle-2 claims)

- **Exactly-once + 0297 ordering**: `choice_prompt.rs:355`
  (`resolved.replace(true)` guard), `:358-361` (modal close — layer
  removal + state disposal — BEFORE the callback), `:362-364` (FnOnce
  taken once); `handle.cancel()` rides the same path (`:444-446`).
  Hostile double-paths pinned live: letter+Enter in one batch,
  click-on-selected+Esc in one batch, Esc+click in one batch
  (`charter_g1_double_paths_letter_enter_and_click_esc_resolve_once`),
  stray Enter after close (`charter_g5…`), chain-from-resolve
  (`charter_g6…`, consumer dry-run).
- **Esc-retreat draft persistence**: the input-row wrapper consumes
  Esc while the editor is focused and only re-anchors focus
  (`choice_prompt_view.rs:444-453` → `Anchors::retreat`,
  `choice_prompt_interact.rs:75-79`) — no draft write anywhere on that
  path; the draft signal lives in the MODAL scope, outside every row
  view (`choice_prompt_view.rs:93`, `choice_prompt_interact.rs:31`).
  Pinned: `charter_o3…` (excursion + retreat + blurred-editor shield +
  commit-after-retreat + fresh-instance-empty), `charter_k4…`.
- **Must-choose refusal visibility**: Esc on a non-dismissable gate
  sets the note, resolves nothing (`choice_prompt_interact.rs:158-162`);
  the note renders in accent ink ("an answer is required",
  `interact.rs:256-258`, `:266-268`) and clears on the next
  movement/toggle (`:83-85`, `:89-90`); no Cancel button
  (`choice_prompt_parts.rs:58-60`) and no Esc shortcut registered — no
  dead-key advertising (`choice_prompt_view.rs:532-540`). Pinned:
  `charter_g3…`, `charter_k2…`, `charter_k4…` (must-choose half),
  consumer dry-run stage 1.
- **A11y truth (A1–A5)**: prompt = `Role::Heading` with the FULL text
  as label (`choice_prompt_view.rs:200-201`); region = `Role::Menu`
  "options" + current-choice value (`:326-328`); rows =
  `MenuItem`/`Checkbox` with `"selected"`/`"on"`/`"off"` values
  (`choice_prompt_parts.rs:270-274`, `:284-287`); revealed editor =
  TextInput (`Input` role) autofocused with a second focus listener
  driving the hint truth (`choice_prompt_view.rs:460-472`); focus
  affordance = selection pair focused vs accent unfocused
  (`parts.rs:293-304`), `focus_affordance_visible` asserted on region
  AND focused button (`choice_prompt_tests_c2.rs:284-315`). In-crate
  pins run in this tree's gate; integration-side reachability is F10.
- **Danger/selection-ground exception**: `parts.rs:293-307` —
  highlighted+focused wears the audited selection pair; unfocused
  highlight keeps Error ink on the label (glyph carries accent);
  unhighlighted wears Error ink. Pinned in one dark + one light
  registered theme (`charter_t3…`), exact token equality both states,
  plus the no-error-on-selection-ground negative.
- **Letters**: case-sensitive, declared-key-outranks-digit (letter
  branch precedes the digit branch, `interact.rs:171-183`),
  SHIFT-carrying uppercase accepted (`:131-133`), editor-shielded
  (printables reach the root handler only when the editor is not
  focused — pinned by `charter_o2…` digits-type case and the builder's
  `option_letters_type_into_a_focused_other_editor`).
- **T1 tokens-only**: grep over all four component sources — zero
  `Rgba::`/`Rgba {`/hex literals; every paint flows from `TokenSet`
  resolved at open (`choice_prompt_view.rs:78-86`).

## 4. Consumer dry-run (the maintainer's brief, end-to-end)

`consumer_dry_run_tool_approval_gate_chains_into_confirm` — the
abstractcode-tui tool-approval shape through the real Driver +
CaptureTerm wire: three lettered options (`a`/`A`/`d`), Deny with
danger tint + detail row, `dismissable(false)`; Esc refused VISIBLY
with nothing resolved; host keyboard starved while open; `d` commits
Deny exactly once; the "are you sure" gate opens from INSIDE the
resolve callback with fresh state and live keys; letter+Enter in one
batch on gate 2 resolves exactly once; both gates close; the host
hears keys again. **PASS.** Stage evidence is screen-level (labels,
letters chips, hint truth, detail row, refusal note); the per-stage
a11y snapshot half rides the F10 in-crate pins — recorded here, not
silently skipped.

## 5. Charter clause table

| Clause | Pinning evidence | Verdict |
| --- | --- | --- |
| G1 exactly-once | `charter_g1…` (3 hostile batches) + `charter_g5…` stray-Enter + builder unit funnel | PASS |
| G2 no silent close | `charter_g2…` (Esc = explicit `Cancelled`) | PASS |
| G3 dismissability knob | `charter_g3…` + `charter_k2…` (no dead-key ad) + dry-run | PASS |
| G4 disposal-in-callback | `charter_g4…` (opener scope disposed in resolve) | PASS |
| G5 selection ≠ commitment | `charter_g5…` | PASS |
| G6 re-openable / chain | `charter_g6…` + `charter_o3…` fresh-draft + dry-run | PASS |
| G7 stable identity | `charter_g7…` (ids, not indices) | PASS |
| O1 Other reveals, no phantom | `charter_o1…` (pixel) + c2 a11y unit `:246-249` (tree) | PASS |
| O2 typing routes | `charter_o2…` (digits type, caret keys, Up/Down to list) | PASS |
| O3 draft survives | `charter_o3…` | PASS |
| O4 empty-Other refuses visibly | `charter_o4…` (full note + whitespace + commit) | PASS |
| O5 Other commit uniform/distinct | `charter_o3…`/`o4…` commits (`selected: []` + `other: Some`) | PASS |
| A1 question in tree | c2 unit `:230-233` (in-crate; integration = F10) | PASS* |
| A2 options + state enumerable | c2 unit `:234-245`, `:253-261`, `:265-279` | PASS* |
| A3 honest roles only | c2 units + source `parts.rs:270-274`; skeleton stays ignored | PASS* (F10) |
| A4 revealed editor Input+focus | c2 unit `:256-261` | PASS* |
| A5 focus affordance visible | c2 unit `:284-315` (`focus_affordance_visible` ×2) | PASS* |
| K1 keyboard-only + trap | `charter_k1…` + dry-run host-starve/return | PASS |
| K2 keys discoverable + truthful | `charter_k2…` (letters, Space, Enter, Esc-only-when-true) | PASS |
| K3 focus lands on open | `charter_k3…` (first key moves; initial visible = the fabricated-selection law) | PASS |
| K4 layered Esc | `charter_k4…` (retreat → cancel / refuse; hint truth while editing) | PASS |
| K5 digit quick-select (NICE) | source `interact.rs:183-195` + builder unit (single: move-only — F9 decision documented) | PASS |
| S1 30 options scroll | `charter_s1…` (window follows, i/N truthful, tail commits) | PASS |
| S2 prompt wraps | `charter_s2…` (every word visible at 44 cols) | PASS |
| S3 fixed rows survive (0240) | `charter_s3…` (two geometries; whole-segment degradation, no mid-word cut) | PASS |
| S4 long labels degrade honestly (SHOULD) | source: ellipsis `parts.rs:320`, full label in tree `parts.rs:275`; label enumerability in c2 units | PASS* (source + in-crate; not integration-pinned — F10) |
| T1 tokens only | source grep: zero color literals | PASS |
| T2 selection visible in EVERY theme | `charter_t2…` (all registered themes + marker-ink visibility) | PASS |
| T3 danger rides Error (SHOULD) | `charter_t3…` (dark + light, exception honest) | PASS |
| P1 zero idle cost | `charter_p1…` (idle turns, zero bytes) | PASS |
| P2 local interaction damage (SHOULD) | `charter_p1…` P2 tail (arrow-move bytes strictly < gate open-paint bytes) | PASS |

PASS* = substance pinned in-crate (same `cargo test` gate) + source
citations; integration-surface reachability blocked by F10.

## 6. Verdict

**SHIP.** All MUST clauses hold with evidence; the two position
conflicts from cycle 1 are settled on the record (F4 conceded to the
layered-Esc position and pinned from both lanes; F3 shipped as the
`dismissable(false)` knob with visible refusal). The one open item
(F10) is an ENGINE observability gap that predates this wave — it
does not degrade what a screen-reader-shaped consumer of the built
tree receives (the roles/labels/values are in the tree and pinned
in-crate); it degrades what an out-of-crate TEST can see. File it for
the engine backlog (first-app findings family, 0220-0250 precedent);
it should not hold the release.

Pre-release checklist: none blocking. Post-release follow-ups, in
order: (1) F10 public overlay-tree accessor, then activate the ignored
skeleton as written; (2) consumer-fit C7 (`on_selection` observer)
when the theme-picker port lands; (3) F11 refusal-note tail bias;
(4) the recorded 0515 debts (resize re-measure, sequence-level
dismissable, custom button labels).

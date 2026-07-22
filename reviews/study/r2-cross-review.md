# Cycle-2 cross-review (R2B): ACTIVATION + SELECT + GOVLEX waves

Reviewer: cycle-2 adversarial seat (R2B). Scope: the three cycle-1
deliveries (A: `List::on_activate` + `TextInput::masked`; B: owned
popup substrate + Select/Combobox/MultiSelect; C: diff lexer + CI
gates + 0.3 budget), every claim verified at source this session.
Concurrent with R2A's semver repair (`Role::Select` removal) — that
churn LANDED mid-review and is folded in below (the tree I ran the
battery on has `Role` byte-identical to the published 0.2.0, verified
against the registry cache at
`~/.cargo/registry/src/…/abstracttui-0.2.0/src/ui/access.rs`).

New discriminating tests added (the one code artifact this review may
produce): `tests/wave_r2_review.rs` — 3 tests, all pass (see F1, F11).

## The three most load-bearing findings first

### F1 (P2, SELECT — coverage gap, now closed) — the flipped Combobox face had zero tests
`combobox_popup_content` orders `status / rows / editor` when the
popup opens ABOVE the anchor (src/app/select_combobox.rs:453-459) so
the editor stays on the trigger row — but every wave test drove the
below-mode ordering only: `place_owned`'s flip had geometry-only unit
coverage (src/app/anchored_owned_tests.rs:32-76), and neither
select_tests_faces.rs nor tests/wave_select_faces.rs ever opened a
popup cramped-below. The flip path is exactly where an ordering bug
would hide (the editor migrating to the top row would silently break
the zero-visual-jump contract). Added
`tests/wave_r2_review.rs::flipped_combobox_keeps_editor_on_trigger_row_with_options_above`
— a bottom-anchored Combobox through the real Driver/CaptureTerm
loop: flip happens, editor frame stays on the trigger row, options
render above it, status above the options, typing lands in the
flipped editor, Enter commits. PASSES — the implementation was
correct; the gap was evidence, not behavior. No action needed beyond
keeping the test.

### F2 (P2, SELECT — recorded-ruling contradiction, small blast radius) — a popup opened over a live toast layers ABOVE the toast
The cycle-3 addendum in reviews/study/platform-on-appkits.md
(§"Cycle-3 addendum") records: "Toasts stay above popups by the
existing band constants; if a popup above `TOAST_Z` is ever wanted,
that is a design smell to refuse." The shipped allocator does not
preserve that when a toast is live at open time: `Toast` is a layer
at `TOAST_Z` = 2000 (src/app/popups.rs:32,183), `Overlays::top_z()`
maxes over ALL layers including draw layers
(src/app/overlays.rs:239-244), so a Select opened while a toast shows
allocates z = 2001 and paints over the toast where they overlap.
Input is unaffected (toasts are draw layers, not trees), and the
overlap window is transient toast-lifetime × popup-open — but it is
a direct contradiction of a recorded ruling, so it needs ONE of:
(a) `top_z()` (or a popup-side variant) considering only TREE layers
below `TOAST_Z`, with the nesting guarantee restated, or (b) the
addendum's sentence amended to bless the current behavior ("a popup
may transiently cover a toast; toasts re-assert above the NEXT popup
only if…"). I lean (b) + a one-line doc note: option (a) reintroduces
a static-band ceiling exactly where the dynamic allocator was the
point, and a modal stack that ever reaches 1999 would break it.
Owner: SELECT lane + platform seat (the addendum's author).

### F3 (P2, corroboration for R2A's hazard note) — stacked `Modal`s tie at z 1000 and the OLDEST wins keys
Not my file to fix (R2A is filing this), corroborated at source so
the note lands with evidence: every `Modal::open` uses the static
`MODAL_Z` (src/app/popups.rs:90), and `Overlays::dispatch` sorts
`sort_by_key(Reverse(z))` (src/app/overlays.rs:333) — Rust's sort is
stable, so two modals at equal z keep STORE order, and the
FIRST-inserted modal receives keys while the newest one shows on
top... actually compositing at equal z is also insertion-order, so
paint and routing may or may not agree depending on flatten's tie
rule — either way "second modal opened over the first" has no
defined owner today. The wave_select stacking test dodges it by
hand-allocating 1000/1100 (tests/wave_select.rs:331). The owned
Popup is immune (top_z()+1). The fix direction (Modal allocating
`max(MODAL_Z, top_z()+1)` or documenting one-modal-at-a-time) is
R2A's call.

## Lane A — ACTIVATION (0250 ruling fidelity + masked leak surface)

### F4 (PASS) — `on_activate` matches the recorded ruling exactly
Checked clause by clause against reviews/study/platform-on-appkits.md
§"The 0250 ruling":
- Clause 1 (selection follows movement) untouched: arrows/Page/Home/
  End/click still move and `on_select` stays the notification
  (src/widgets/list.rs:156-165, 330-342); pinned by
  `movement_fires_on_select_never_on_activate` (list_tests.rs:185).
- Clause 2: Enter always; Space activates in List (no toggle
  meaning — documented in the builder doc, list.rs:167-174); click
  on the ALREADY-selected row activates, click on unselected only
  selects (list.rs:362-375); NO double-click synthesis anywhere
  (grepped: no click-count state exists). Wire-level pins in
  tests/adv_activation.rs:53-123 (SGR bytes through the real
  Driver, including the selection-ground frame assertion).
- Compatibility: unbound List consumes nothing — Enter/Space pass
  through to app shortcuts (list.rs:319-328 consumes only when a
  callback is bound); pinned twice (list_tests.rs:260,
  adv_activation.rs:126-194 with a root shortcut hearing Enter).
- Clause 4 (disposal-safety law) on BOTH widgets: List completes
  selection write + key write + ensure-visible BEFORE `on_select`
  (list.rs:262-291) and before `on_activate` on both key and click
  paths (list.rs:321-328, 362-375); Table has the same inversion for
  `on_select` (src/widgets/table.rs:153-181). Disposal pinned per
  widget and per path: `on_select_may_dispose_the_lists_scope`,
  `on_activate_may_dispose_the_lists_scope` (Enter AND
  click-on-selected, plus a follow-up event over the dead tree,
  list_tests.rs:294-349), `on_select_may_dispose_the_tables_scope`
  (table.rs:524-549).
- Empty-list guard: movement and activation are inert on an empty
  List (`select()` early-returns before indexing the prefix sums,
  list.rs:257-259; Enter/Space gated `len > 0`, list.rs:322);
  pinned (list_tests.rs:352-379). The pre-fix panic (arrows indexing
  `prefix[target+1]` past a 1-entry prefix) is real and the
  CHANGELOG Fixed entry describes it accurately.
- Space-toggle division of labor holds family-wide: MultiSelect's
  popup gives Space to TOGGLE and Enter to commit
  (src/app/select_multi.rs:367-368) — the F5-of-cycle-2 rule encoded;
  Select/Combobox triggers treat Space as open (a trigger is a
  button; no toggle meaning) — consistent.

### F5 (P3, wording) — CHANGELOG Fixed entry overstates Table
"`List` and `Table` now complete ALL internal bookkeeping … BEFORE
invoking `on_select`/`on_activate`" (CHANGELOG.md:86-90) — Table has
no `on_activate` (grep src/widgets/table.rs: only `on_select`/
`on_sort_requested`). Union phrasing reads as Table having an
activation event. One-word repair: "…before invoking their selection
callbacks (`on_select`, and on List `on_activate`)".

### F6 (PASS) — the masked leak surface is really closed
Walked every export path for a `TextInput` value:
- Semantic tree: `access_value` closure substitutes bullets when
  masked (src/widgets/input.rs:242-253); `accessibility_tree()`
  samples that closure (src/ui/tree.rs:221-224), and
  `accessibility_tree_text` / `a11y_tree` / `focus_announcement` are
  all derived from the same snapshot (tree.rs:248-274). No other
  reader of the raw value exists in the tree layer.
- Screen/wire: the draw prints one `•` per cluster with same-width
  padding (input.rs:315-334), so the composed frame never holds
  plaintext; `app::selection`'s extraction reads the composed
  frame's CELLS (`extract_text` over `Surface`,
  src/app/selection.rs:430-451), so drag-select + OSC 52 copy ships
  bullets — safe BY CONSTRUCTION, not by filtering. Verified the
  extraction source directly; it has no path to widget state.
- Wire pin: adv_activation.rs:196-253 types through the real driver
  and asserts the VT dump and the a11y text both carry zero
  plaintext and ≥7 bullets; unit twin in input_tests.rs:162-215
  including the unmasked control (plaintext export unchanged).
- The bound value signal stays plaintext deliberately (the app owns
  it) — documented in the builder doc (input.rs:136-146). The
  placeholder renders unmasked and is documented not-secret.
Two honest residues, both conventional and documented: bullet COUNT
reveals secret length (explicitly "count-honest" — same as every
mainstream password field), and:

### F7 (P3) — masked fields still expose word structure through cursor motion
Alt+arrow word jumps run `word_step` over the REAL text
(input.rs:502-522 via the cluster map of the true value), so a
shoulder-surfer watching the caret in a masked field can count words
and word lengths. Native password fields typically degrade word ops
to whole-field jumps. Cheap fix if wanted: in masked mode, treat
Alt+Left/Right as Home/End. Not a wire/automation leak (nothing
exports), so P3.

## Lane B — SELECT (owned-popup contract)

### F8 (PASS) — the owned-popup contract holds at source
- Keyboard containment: the popup allocates `top_z() + 1`
  (src/app/anchored_owned.rs:225; overlays.rs:231-244 is the one
  budgeted engine delta), and `dispatch` walks topmost-first,
  returning at the first MODAL tree for keys (overlays.rs:333,
  369-371) — while open, NO key reaches modals or root. Pinned at
  unit level (anchored_owned_tests.rs:144-200: two stacked modals,
  keys to popup, modal-two hears nothing, ownership returns on
  dismiss) and at wire level (tests/wave_select.rs:313-427, the
  spec's F1 acceptance case through real bytes).
- Dismiss-exactly-once: the single teardown seam takes the layer
  handle under the borrow — the second caller finds None and no-ops
  (anchored_owned.rs:270-287); `on_dismiss` fires once with the
  FIRST reason (pinned: close, close again, then a late
  dismiss(Escape) — exactly one Commit, tests:227-244). The
  "Escape while an outside-press is queued" race is serial in the
  driver: whichever dispatches first ends the popup; the second
  event finds the layer gone and falls through to the remaining
  stack. That means a queued outside-press can ACT on what is below
  after an Escape already closed — the standard sub-frame menu race,
  acceptable; the exactly-once and first-reason-wins guarantees hold
  in both orders.
- Anchor-unmount safety: the cleanup hook rides the CONTENT scope
  (child of the opener — disposal cascades) via a weak handle, and
  the hook path skips re-disposing a scope mid-cleanup
  (anchored_owned.rs:240-245, `end(reason, dispose_scope=false)`);
  pinned with the orphan-layer assertion (tests:247-272). Callback
  ordering follows the clause-4 law: layer removal + scope disposal
  complete BEFORE `on_dismiss` runs (anchored_owned.rs:278-286).
- Placement: `place_owned` mirrors `place_panel`'s flip rule (prefer
  below unless below is short AND above offers more,
  anchored_owned.rs:104-115 vs anchored.rs:94-101); clamps keep
  x within viewport (w pre-clamped to viewport, so
  `viewport.w - w >= 0`); `rows <= 0` on both sides returns None and
  callers skip opening. Geometry pinned (tests:32-99), face-level
  flip now pinned by my F1 test.
- `commit_on_move` default OFF verified (src/app/select.rs:156) with
  the opt-in preview + Escape-restores path pinned
  (select_tests.rs:261-291) and the default-off behavior pinned by
  the arrows-move-highlight-only test (select_tests.rs:146).

### F9 (P3) — viewport resize while a popup is open leaves it at the stale rect
v1 places once at open — the documented rationale ("the modal owns
all input while open, so the anchor cannot move under it",
anchored_owned.rs:32-33) covers scroll/reflow but NOT a terminal
resize: `apply_resize` re-sizes the root layer only
(src/app/driver.rs:635-656), so after a shrink the popup can sit
partially or fully off-screen while still modal-owning all input.
Escape and outside-press still work (position-independent /
outside-bounds respectively), so the user is never trapped — but an
invisible modal popup is a confusion state. Suggested follow-up:
dismiss open popups on Resize (a fifth `DismissReason::Viewport` or
reuse AnchorGone), matching what the passive panel's anchor-loss
philosophy would predict. One-liner in the popup store; not a
blocker.

### F10 (P3, recorded for the record) — `top_z()` mid-phase returns 0
`try_borrow` failure (layer ops inside a draw closure) yields 0
(overlays.rs:239-244), which would put a popup at z=1 — under any
modal. Draw purity already forbids opening popups from draw
closures, so this is defense-in-depth honesty, not a live path.
Fine as is; naming it so nobody "fixes" the unwrap_or into a panic.

## Lane C — GOVLEX (diff lexer, CI, budget)

### F11 (PASS, coverage extended) — the diff classification table is honest against real git output
- Real-shape coverage verified: rename chrome (`similarity index`,
  `rename from/to`, `copy from/to`), mode lines, `Binary files`,
  `GIT binary patch`, `Only in` all in META_PREFIXES
  (src/text/diff.rs:66-82); `diff --git` routes Meta; `+++/---`
  headers require the separator so bare `---` stays a Removed body
  line (diff.rs:130-138) — the documented header-first ambiguity is
  BOTH documented (module doc lines 20-23) and tested
  (diff.rs:238-246). `+++ /dev/null` / `--- /dev/null` classify
  FileHeader by the prefix rule — was untested; now pinned in
  tests/wave_r2_review.rs::diff_lexer_classifies_dev_null_headers_and_rename_chrome.
- CRLF: prefix rules are unharmed by a trailing `\r`; the hunk
  header's `\r` falls into the trailing Context span with valid
  char-boundary ranges — was untested; now pinned
  (…::diff_lexer_tolerates_crlf_terminated_lines).
- Non-ASCII slicing tested in-module (diff.rs:264-271) plus the
  deterministic mini-fuzz (277-317) plus the 5k hostile campaign in
  tests/fuzz_big.rs:205-262 (ignored suite, per doctrine).
- Statelessness rationale (scroll-position-independent tinting,
  diff.rs:17-23) is sound and matches how CodeView renders from an
  offset. `DiffKind` is `#[non_exhaustive]` (diff.rs:41) — additive-
  safe; the in-crate exhaustive match in `diff_token_color` is the
  right compiler-walks-new-kinds choice (code.rs:61-74).

### F12 (PASS, with one methodology note) — the contrast floor is the right class, measured on the right ground
docs/theming.md's own table (lines 172-182) puts semantic inks at
3.0:1 against `bg` and syntax inks at 4.5:1 against `surface_raised`.
The wave's test measures ok/error/info/text_muted against
`surface_raised` — the ACTUAL code ground, which the bg-anchored
audit does not cover — across all 26 themes at the 3.0:1 semantic
floor (code.rs:384-403). That is the right pair to measure and the
honest floor for state-classed inks; the note: added/removed BODY
TEXT is read as prose in that ink, and the strictest reading would
hold it to the syntax-ink 4.5:1. The 3.0 choice is defensible
(added/removed are state markers, one ink per line, bold-free) and
the test names theme + measured value on failure — methodology
sound. If diff-heavy consoles ever complain, re-audit at 4.5 before
inventing new tokens.

### F13 (PASS) — CI wiring is sound; no job can brick the others
- `.github/workflows/ci.yml` parses (python yaml, all 6 jobs
  enumerate); jobs are independent (zero `needs:` edges), so a
  failing gate isolates. `concurrency` + `cancel-in-progress` sane.
- Action refs are real and current: `actions/checkout@v7` exists
  (v7.0.0 GA 2026-06-18, v7.0.1 2026-07-20 — verified against the
  releases page this session); `dtolnay/rust-toolchain@master` with
  a pinned `toolchain: "1.87.0"` is that action's documented pinning
  form; `Swatinem/rust-cache@v2`, `obi1kenobi/cargo-semver-checks-
  action@v2` standard.
- live-pty job flags byte-match CONTRIBUTING.md:40
  (`--ignored --test-threads=1`, examples prebuilt, serial).
- msrv job (`cargo check --all-targets --locked` at 1.87.0)
  REPRODUCED LOCALLY: clean (see battery). Lockfile is v4; cargo
  1.87 reads it natively — the comment's claim checks out.
- semver job baseline (latest crates.io release) stated in-file and
  REPRODUCED LOCALLY: `cargo semver-checks --baseline-version 0.2.0`
  → 196 checks, 196 pass, 57 skip, no semver update required — i.e.
  R2A's `Role::Select` removal verifiably restored the additive-only
  regime (the current `Role` enum is byte-identical to the published
  0.2.0's, checked against the registry cache).
- Nits (P3): `test-unix` runs `cargo test` (which already includes
  doc tests) plus a separate `cargo test --doc` — one redundant
  doc-test pass, ~free; action tags are major-floating rather than
  SHA-pinned, consistent with the repo's existing workflows.

### F14 (P3, pre-tag housekeeping) — llms.txt / llms-full.txt are stale and SHIP in the crate
`llms-full.txt` embeds full doc files (`===== FILE: docs/api.md
=====`) and was generated 05:54 — before the wave's api.md additions
(07:59): zero mentions of `on_activate`, `masked`, `Popup`,
`DiffLexer`, `Combobox` anywhere in either file. Cargo.toml's
`exclude` does not cover them (Cargo.toml:21), so 0.2.1 would ship a
crate whose AI-readable docs describe 0.2.0. Regenerate both before
tagging.

## Cross-lane seams (the checklist's item 5)

### F15 (PASS) — masked × Select composes; no plaintext path through popups
The Combobox's popup editor is an internal `TextInput` with no
masked knob (select_combobox.rs:396-407) — a "masked Combobox" does
not exist as a surface today, and its filter text is deliberately
non-secret ("the filter text is never the value"). If an app mounts
its own masked `TextInput` inside ANY popup tree, masking is
widget-level (draw + access_value both substitute), so the popup's
access tree exports bullets — redaction-at-source holds regardless
of which tree hosts the widget. Select/Combobox/MultiSelect trigger
`access_value`s export the chosen LABEL or placeholder
(select.rs:380-386, select_combobox.rs:290-296,
select_multi.rs:258+) — not secrets by construction.

### F16 (PASS) — components example stays headless-exit-0; CHANGELOG reads as one section
`examples/components.rs` keeps the `have_tty()` guard (prints and
returns Ok, lines 37-40) — headless exit 0 preserved with the new
picker section; the pty-driven `live_components` case covers the
interactive path in the CI live-pty job (tests/live_smoke.rs:271).
CHANGELOG [Unreleased]: single header, Added (6 entries) + Fixed
(2 entries), consistent voice, no duplicates; the Role claim was
swept to `Role::Button` in the same churn that fixed the enum
(CHANGELOG.md:75-77) and api.md matches (docs/api.md:381-384). The
0500 completion record keeps the superseded Role::Select claim WITH
an explicit supersession note (0500_select_combobox_family.md:361-368)
— honest history, correct form. Backlog moves verified: 0250 in
completed/first-app (Completed: 2026-07-22), 0500 in
completed/app-kits, dated status notes on 0140 (proposed, diff slice
shipped additively) and 0180 (planned, 3 of 4 legs executed) both
present and accurate to the shipped code.

## Verification battery (run at the end, tree stable — no churn after 07:59; re-checked 08:26)

| Gate | Command | Result |
| --- | --- | --- |
| Whole-tree tests | `cargo test` | **1438 passed, 0 failed, 75 ignored** (sum over all suites incl. doctests; includes the 3 new wave_r2_review tests) |
| Clippy | `cargo clippy --all-targets -- -D warnings` | clean |
| Format | `cargo fmt --check` | clean (my own new test file needed one wrap; fixed) |
| Alloc pins | `cargo test --test alloc_budget` | 8 passed, 0 failed |
| MSRV (mirrors CI msrv job) | `cargo +1.87.0 check --all-targets --locked` | clean |
| Semver (mirrors CI semver job) | `cargo semver-checks --baseline-version 0.2.0` | 196 checks: 196 pass, 57 skip — no semver update required |
| Rustdoc (mirrors CI lint) | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | clean |

## VERDICT

**0.2.1-ready: YES, after two pieces of pre-tag housekeeping** — with
R2A's `Role::Select` removal already landed and verified additive-only
against the published 0.2.0 (semver gate green, 196/196), nothing in
the three lanes blocks the release. The two things that should land
before the tag, both trivial:

1. Regenerate `llms.txt`/`llms-full.txt` (F14 — they ship in the crate
   and currently describe pre-wave docs).
2. The CHANGELOG Fixed-entry wording nit (F5 — Table has no
   `on_activate`), foldable into the version-bump commit.

Recommended but non-blocking follow-ups, in priority order: settle the
popup-over-toast intent contradiction one way or the other (F2 — my
recommendation is amending the addendum), file dismiss-on-resize for
open popups (F9), and R2A's Modal same-z note (F3) which the owned
popup already dodges. The masked surface, the activation ruling, the
owned-popup contract, the diff lexer, and the CI wiring all held up
under adversarial reading — the wave's weakest point was test coverage
on the flipped Combobox face, and that is now pinned.

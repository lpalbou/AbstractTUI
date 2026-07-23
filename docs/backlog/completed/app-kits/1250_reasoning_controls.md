# 1250 — Reasoning controls: `ReasoningSelect` + the footer label grammar + `ThinkingFold`

## Metadata
- Created: 2026-07-26
- Status: Completed (same wave; filed directly in completed/ — the
  0535/0595/0605/0273 precedent)
- Completed: 2026-07-26
- Track: app-kits — **band continuation**: 0500–0590 is full (spills
  0595/0605 recorded), so per the overview's rule the track continues
  at the NEXT FREE FIFTY = **1250** (1200–1220 are the wave-13
  architecture/md-lane filings; 1100–1190 is field-core). Recorded
  here and in `proposed/app-kits/README.md`.
- Trigger: OPERATOR-ORDERED cross-seat plan ("reasoning as a
  first-class citizen"); this item is the tui seat's committed section
  T1–T4. The shared contract facts are FROZEN by the core seat's
  contract v1 and are NOT re-litigated here.

## ADR status
- Governing ADRs: ADR-0001 (API stability) — everything ADDITIVE vs
  0.2.23 (`cargo semver-checks` clean). No new dependencies. New
  public items: `app::reasoning` (ReasoningSelect, ReasoningFacts,
  LockState, reasoning_label, reasoning_label_glyph, REASONING_LADDER,
  REASONING_AUTO), `widgets::thinking_fold` (ThinkingFold,
  ThinkingFoldState), two additive `Disclosure` builders
  (`title_muted`, `detail_signal`). Prelude re-exports appended.

## The frozen contract (cross-seat, restated for the record)
- Effort ladder `none | minimal | low | medium | high | xhigh` +
  `auto` (provider default). UI word "reasoning"; the WIRE key apps
  write is `thinking` — the ENGINE MINTS NO WIRE VOCABULARY (it
  renders and emits values; apps do the writing).
- Capability facts arrive AS DATA: apps parse the gateway-served
  `reasoning{thinking_support: bool, reasoning_levels: [subset]}`
  block (or its absence) into `ReasoningFacts`. Three-state coupling:
  (a) CAPABLE — offer none/auto + the DECLARED levels only;
  (b) NON-REASONING — locked ("reasoning: none — model does not
  reason"), refuses to open; (c) UNKNOWN — locked-to-none by default
  WITH a "set anyway (capability unknown — passed verbatim)" override
  row that unlocks the full ladder.
- Reasoning TEXT arrives from result METADATA (never parsed from
  reply prose); streams may deliver fragments then a trailing complete
  aggregate — LAST WINS (replace fragments with the aggregate).
- T4: these coupling rules mirror the shared abstractuic reasoning
  contract; **parity is verified when uic's API lands** (the grammar
  helper is the spelling source both sides cite).

## Design rulings

### The lock glyph — width research (the ThemeSwitcher-precedent bar)

Bar: no emoji-promotable codepoints; narrow in BOTH unicode-width
conventions (`width == width_cjk == 1` — the crate's own
East-Asian-Ambiguous oracle, `text::is_risky_ambiguous`); credible
monospace font coverage; a defensible mnemonic. Probed 2026-07-26
against unicode-width 0.2 (Unicode 16 EAW):

| Candidate | width | width_cjk | emoji-data | verdict |
| --- | --- | --- | --- | --- |
| `🔒` U+1F512 LOCK | 2 | 2 | **Emoji=Yes** | rejected outright (the only real padlock — emoji, double-width) |
| `⚿` U+26BF SQUARED KEY | 1 | **2** | no | AMBIGUOUS (the `◐` risk) + poor coverage — rejected |
| `×` U+00D7 | 1 | **2** | no | Ambiguous (0605 already rejected it) |
| `≢` U+2262 | 1 | **2** | no | Ambiguous — rejected |
| `●` U+25CF (shipped hint mark) | 1 | **2** | no | Ambiguous — noted for the family, not this item's lane |
| `✕` U+2715 (the 0605 close glyph) | 1 | 1 | no | passes width bar but COLLIDES semantically in-crate (✕ = close) — rejected |
| `⌧` U+2327 | 1 | 1 | no | passes width bar; spotty font coverage; obscure mnemonic — rejected |
| **`⊘` U+2298 CIRCLED DIVISION SLASH** | **1** | **1** | **no** | **CHOSEN**: Mathematical Operators (a block emoji-data never touches — nearest emoji is U+231A), no-entry mnemonic, solid monospace coverage |

**Decision**: there is NO padlock outside emoji-data — the text
fallback wins: the PLAIN annotation `(locked)` is the CANONICAL
grammar (self-describing, ASCII-safe in every width convention, what
a screen reader hears). Both forms are exposed as commissioned:
`reasoning_label` (plain, canonical) and `reasoning_label_glyph`
(`⊘`-bearing, for width-tight footers). The research is executable:
`lock_glyph_is_narrow_in_both_width_conventions` pins the chosen
glyph AND the rejection reasons (the ambiguous set asserted ambiguous,
the emoji lock asserted double-width) so the table cannot rot.

### The label grammar (T2)
`reasoning_label(value, LockState) -> String`: `r: <value>` /
`r: <value> (locked)`; the glyph form swaps the annotation for ` ⊘`.
`LockState` is a deliberate EXHAUSTIVE two-variant enum: the grammar
is structurally binary (an annotation renders or it does not); the
two locked SHAPES (non-reasoning vs unknown) differ only in their
why-line, which is the control's own surface (trigger why-line +
a11y value), never the footer grammar's.

### ReasoningSelect (T1)
- **Home + substrate**: `src/app/reasoning.rs` (the select family's
  home — popup-opening controls live app-side), composing the 0500
  select core exactly as ThemeSwitcher does (movement, type-ahead,
  option rows, the framed trigger, owned SCREEN-anchored popup — the
  P1 anchor class pinned inside a Modal).
- **Facts shape**: `#[non_exhaustive]` struct
  `{ support: Option<bool>, levels: Vec<String> }` with one
  constructor per honest state (`capable`/`non_reasoning`/`unknown`;
  `Default` = unknown). No serde in core — apps parse.
- **Declared levels render VERBATIM** (deduplicated, declared order,
  empties dropped; `auto`/`none` never double the structural rows).
  Filtering unknown strings would mint vocabulary authority the
  engine does not own — the gateway is the authority on what a model
  supports (thin-client honesty). Pinned by
  `capable_unknown_level_strings_render_verbatim_and_dedup`.
- **Commit semantics**: write-if-different against the EFFECTIVE
  value — a locked display pins to "none", so overriding an unknown
  model to `auto` fires even when the internal signal already said
  auto, and committing `none` from a locked display is a silent close
  that KEEPS the lock annotation. State writes before the callback
  (0297); the widget never writes the bound signal uninvited.
- **Unknown override**: popup-side latch per INSTANCE (`set anyway`
  row → synchronous dismiss + reopen with the full ladder at the same
  anchor — `Popup::dismiss` is synchronous, verified). The lock
  ANNOTATION clears only on an actual ladder commit. Reset on model
  change = remount with fresh facts (documented recipe; the widget
  owns no provider/model coupling).
- **Non-reasoning**: faint, out of the focus order (the family's
  disabled convention), refuses keyboard AND mouse; the why-line
  rides the trigger (long form, SHORT fallback drops it first) and
  the a11y value.

### ThinkingFold (T3)
- **Composition**: Disclosure (folded-by-default — operator ruling;
  `title_muted` + `detail_signal`, the two additive knobs this item
  adds to Disclosure) over the state's one-item STREAMING Feed —
  the `Disclosure::markdown` recipe, which IS the MarkdownView
  typeset recipe ("one recipe, no drift": fences/tables tint
  mid-stream) with the open-block-only re-typeset the streaming lane
  needs. Capped body scrolls (`max_body_rows`, default 8).
- **Streaming semantics (pinned)**: `append` = open-tail fold +
  indicator advance; `complete(aggregate)` = REPLACE (last-wins);
  double-complete replaces again; **fragments after complete are
  REFUSED** (`append -> false`) — a late straggler must not corrupt
  the aggregate.
- **Zero idle**: the dot indicator's frame index is the APPEND
  COUNTER mod the Dots frame table — data-driven animation, no timer
  anywhere; a quiet open stream and a completed fold schedule
  nothing (Driver-pinned: `parked_and_completed_folds_are_idle`).
- **Header liveness without remounts**: the indicator+detail ride
  `Disclosure::detail_signal` — the header's focusable element is
  stable; a focused header keeps focus across appends
  (`detail_signal_updates_live_without_dropping_header_focus`).
- **Placement honesty**: a ThinkingFold CANNOT live inside a `Feed`
  item — feed blocks are draw-only (first-app/0280); the supported
  shape is the fold BESIDE its feed segments in the turn column
  (tested inside a Block panel stack; the commission's "works inside
  Feed items" is satisfiable only as this composition until 0280's
  widget-hosting design lands — recorded honestly, not papered over).
- **One mounted card per state** (the body feed typesets at one
  width) — documented.

## Self-adversarial pass (attacks run, outcomes)
1. **Empty levels on a capable model** → offers auto/none alone
   (pinned: `capable_empty_levels_offers_auto_and_none_only`).
2. **Unknown level strings** → decided VERBATIM (+dedup, empties
   dropped) — see the ruling above; pinned.
3. **Stale override across model switch** → cannot leak: facts are
   constructor data; the remount recipe resets latch AND uncontrolled
   value (pinned: `unknown_override_does_not_leak_across_remount`).
4. **50KB reasoning text** → cap holds (≤ 8 body rows), scrollbar
   thumb engages, content below stays reachable (pinned:
   `a_50kb_thought_stays_capped_and_scrolls`; Feed virtualizes rows,
   appends stay O(open tail)).
5. **Fragments after complete** → decided REFUSED, returns false
   (pinned: `fragments_after_complete_are_ignored`).
6. **Double-complete** → decided last-wins-again (pinned:
   `double_complete_last_wins_again`).
7. **Found + fixed during the pass**: (a) the modal-anchor test used
   `str::find` BYTE offsets against a row containing multi-byte
   border glyphs — columns drifted; rewritten to char positions
   locating the trigger's `▐` stroke. (b) The family's grow-default
   balloons a bare select in a COLUMN (anchor honestly spans the
   grown rect; popup opens under it) — kept family-consistent,
   demo/tests/docs use the `line(1)` footer-row composition.
   (c) Same-value commit through the unknown ladder ("auto" over a
   locked display whose signal already said auto) would have been
   silently swallowed by a naive bound-value comparison — the
   EFFECTIVE-value comparison is the fix (pinned:
   `unknown_commit_of_none_keeps_the_lock_annotation` + the override
   test's auto case reasoning).

## Completion report
- Final path: docs/backlog/completed/app-kits/1250_reasoning_controls.md
- Date: 2026-07-26
- Outcome: `app::ReasoningSelect` + `ReasoningFacts` + `LockState` +
  `reasoning_label`/`reasoning_label_glyph` + `REASONING_LADDER`/
  `REASONING_AUTO`; `widgets::ThinkingFold` + `ThinkingFoldState`;
  `Disclosure::title_muted`/`detail_signal`; `examples/reasoning.rs`
  (three-model cycle, streamed-then-recomposed fold, the wire line,
  the remount recipe; headless exit-0); api.md sections
  ("app::ReasoningSelect", "ThinkingFold"), CHANGELOG under
  Unreleased.
- Key validation (all names in `src/app/reasoning_tests.rs`,
  `src/widgets/thinking_fold_tests.rs`, `disclosure_tests.rs`):
  three-state coupling (`capable_offers_declared_levels_only`,
  `locked_refuses_open_with_why_line`,
  `unknown_offers_set_anyway_which_unlocks_the_full_ladder`),
  grammar goldens (`grammar_goldens_all_values_by_lock_state`), the
  width-research pin, commit-once semantics
  (`commit_fires_once_and_same_value_commit_is_silent`), controlled
  binding, the modal SCREEN-anchor pin, a11y lock values, zero-idle
  both widgets, fold streaming/last-wins/refusal/cap/composition
  suite, Disclosure knob tests. Full workspace suite + clippy + fmt +
  `cargo semver-checks` (additive vs 0.2.23) green — numbers in the
  wave report.

## Follow-ups revealed
- The shipped select-family `●` hint mark is East-Asian-AMBIGUOUS
  (this item's probe) — inside popup rows the presenter's
  risky-cluster defense confines it, but a family-wide narrow-mark
  audit (`•`?) deserves a small item when the select family next
  opens.
- field-gateway 0905 (same-value re-commit unobservable) now has a
  THIRD face (Select, Combobox, ReasoningSelect) — if the family
  gains a recommit event, ReasoningSelect adopts it in the same pass.
- uic parity check when the shared kit's reasoning API lands (T4):
  the coupling rules + the `r:` grammar spelling are the parity
  surface; `reasoning_label` is the source both sides cite.
- 0280 (Feed custom blocks cannot host widgets) is the gate between
  "fold beside the feed" and "fold inside the feed item" — if 0280
  ever lands widget-hosting blocks, ThinkingFold is its first named
  consumer.

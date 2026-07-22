# 0120 — TextArea: multiline composer with history + completion anchor

## Metadata
- Created: 2026-07-21
- Status: Completed (composer wave — see the completion report below)
- Track: app-widgets
- Completed: 2026-07-22

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: None expected (new widget; `TextInput` unchanged).

## Context
The bluntest line of the cycle-11 critique: the crate "ships only a
single-line text input". A multiline composer is the input half of every
interactive app class — chat and feeds (message bodies are paragraphs;
one-line messages are a non-starter), consoles and REPLs (multi-row
prompts, history recall, `/command` and `@file` completion), and any
form- or note-taking surface. Both reviews list it P0/P1 (completeness
§2b P0-3; robustness Part 2 P1-1); the two port composers — the coding
console's prompt box (`abstractcode/fullscreen_ui.py`) and the chat
client's markdown composer (agora bodies up to 64 KB) — are the first
validators and supply the reference semantics below.

## Current code reality
- `src/widgets/input.rs:1-16` — `TextInput` is single-line **by design**
  ("TextInput: single-line editable text field"). What it already solved
  and this item must reuse, not re-derive: cluster-atomic editing
  (`ClusterMap`, input.rs:44-96, over `text::segments` — one cursor stop
  per grapheme cluster, widths from the same authority as rendering),
  selection via anchor+cursor, word jumps, whole-`Paste` insertion (never
  per-char synthesis), horizontal scroll keeping the caret visible.
- `src/app/overlays.rs:158,212` — `Overlays::layer` + `on_outside_press`
  exist: a completion dropdown is buildable as an overlay **if** the
  composer exposes where the caret is on screen. Nothing exposes a caret
  cell today.
- `docs/faq.md:164-166` — Ctrl+Enter and Shift+Enter do not exist on the
  classic terminal wire; the kitty keyboard protocol disambiguates where
  supported (the engine already decodes it). Any newline-vs-submit design
  must be honest about legacy terminals.
- Target semantics worth matching, read from the Python console:
  `abstractcode/fullscreen_ui.py:144-163` (`arrow_nav_action`) — Up moves
  within the buffer first, jumps to text start, and only then recalls
  history; Down mirrors it; an empty buffer goes straight to history.
  Completion: fullscreen_ui.py:56-120 curates a `/`-command list whose
  first completion screen is deliberately the app's face; `@` file
  mentions complete from workspace files.

## Problem
There is no multiline editing surface at all: no vertical caret movement,
no logical-line model, no grow-to-content, no history recall, no caret
anchor for a dropdown. Building a serious composer app-side means
re-deriving cluster math the engine already owns.

## What we want
A `TextArea` widget:
1. **Multiline model**: logical lines with soft wrap at the widget width;
   caret moves by cluster horizontally (reuse `ClusterMap`) and by visual
   row vertically with a remembered goal column; Home/End per visual row,
   Ctrl+Home/End for the document.
2. **Grow-to-content** up to a `max_rows` cap, then internal scroll.
3. **Submit vs newline policy** owned by the app via the builder:
   Enter-submits + Alt+Enter-inserts-newline as the default preset
   (works on every wire), with Shift+Enter additionally inserting where
   the kitty protocol reports it. Never advertise chords the wire cannot
   carry (faq.md:164).
4. **History recall** with the row-boundary semantics of
   `arrow_nav_action`: arrows navigate the buffer first and reach for
   history only at the edges; recalled entries replace the buffer; the
   in-progress draft survives a history round-trip.
5. **Block paste**: a bracketed `Paste` event inserts whole, newlines
   included (the input layer already delivers it whole and neutralized).
6. **Caret cell exposure**: a signal (or query) yielding the caret's
   screen cell so a completion dropdown can anchor an overlay at it.
7. **Completion dropdown**: v1 as a documented recipe/example over
   `Overlays::layer` + `on_outside_press` + the caret cell (trigger
   prefixes like `/` and `@` are app policy); promote to a packaged
   widget only if both ports end up copying the same code.

## Scope / Non-goals
Scope: the widget, history, caret anchor, the dropdown recipe + example.
Non-goals: IME composition (same posture as `TextInput`, input.rs:13-15 —
composed input arrives as the terminal sends it); syntax highlighting
inside the composer; undo stacks beyond a single draft-restore;
readline/vi emulation modes.

## Expected outcomes
Both port epics get their composer from the engine; the completion
dropdown for `/` and `@` is an afternoon of app code, not a widget fork.

## Validation
- Unit: caret math over multi-cluster content (ZWJ emoji, combining
  marks) in both axes; goal-column persistence; history edge semantics
  (port the `arrow_nav_action` decision table as cases).
- CaptureTerm acceptance: type/wrap/grow to cap, paste with newlines,
  submit vs newline chords (kitty and legacy input bytes), dropdown
  anchored at the caret and dismissed by outside press.

## Progress checklist
- [x] Multiline buffer + caret/goal-column model over ClusterMap
- [x] Grow-to-cap + internal scroll
- [x] Submit/newline policy presets (legacy-honest)
- [x] History recall (edge-triggered, draft-preserving)
- [x] Caret cell exposure + dropdown recipe/example
- [x] Acceptance + cluster-math tests

## Field evidence (2026-07-21, first app)
`abstractcode-tui`'s composer is a single-line `TextInput`; multi-line task
prompts (the norm for coding agents) must be written elsewhere and pasted —
the paste path folds newlines to spaces (src/widgets/input.rs paste arm), so
structure is lost. A real composer is the app's top missing input feature.

## Completion report
- Final path: docs/backlog/completed/app-widgets/0120_textarea_multiline_composer.md
- Date: 2026-07-22
- Shipped: `widgets::TextArea` + `widgets::TextAreaState` +
  `widgets::textarea::SubmitPolicy` (src/widgets/textarea.rs; editing
  model in the private sibling textarea_model.rs — file-size split;
  tests in textarea_tests.rs) and `app::anchored` (AnchoredPanel +
  `place_panel` + PanelAnchor/PanelWidth + Completion +
  CompletionCandidate; tests in anchored_tests.rs) with the one
  budgeted engine delta `Overlays::top_z()` (0500's spec). Prelude
  exports: TextArea, TextAreaState, AnchoredPanel, Completion,
  CompletionCandidate. `TextInput`'s `ClusterMap`/`word_step`/`notify`
  went `pub(crate)` and are REUSED, not re-derived (item requirement).
  Example: examples/transcript.rs gained the bottom composer ('/'
  commands + '@' mentions, history, growth).
- Design decisions vs the wish list:
  - Item 1 (multiline model): the widget wraps through its own
    byte-tiling `RowMap`, not `text::wrap` — the renderer's wrapper
    CONSUMES whitespace at breaks, which is display-correct but leaves
    caret bytes homeless; RowMap tiles every byte into exactly one row
    (invariant-tested) while matching wrap's visuals (break-run
    whitespace hangs clipped at the row edge). Widths come from the
    same `text::segments` authority.
  - Full rows have no margin cell, so the caret at an exactly-full
    row's end lives on the NEXT visual row (a phantom empty row at the
    document end — the editor-standard cursor-wrap); End affinity
    (`sticky`) applies only to rows with a spare column. Found by the
    acceptance test: the naive clamp stomped the last glyph.
  - Item 3 (submit vs newline): `SubmitPolicy::EnterSubmits` default
    (Enter submits; Alt+Enter + kitty Shift+Enter insert),
    `EnterInserts` for app-owned submission. Ctrl+Enter deliberately
    unbound (not on the classic wire; the spec names Alt/Shift only).
  - Item 4 (history): `arrow_nav_action` decision table ported and
    test-pinned; the DRAFT is the protected artifact (saved on nav
    start, restored past the newest); edits to a recalled entry are
    ephemeral under further navigation (bash-adjacent; documented).
  - Item 6 (caret cell): `TextAreaState::caret_cell()` —
    Signal<Option<Point>>, computed in event handlers from
    `EventCtx::current_rect` (the only rect source; 0500 records the
    general rect-query gap). Honest wart, documented on the method: a
    pure resize staleness window until the next event; same class as
    the growth band's width hint.
  - Item 7 DEVIATION (dropdown recipe → packaged controller): shipped
    `app::anchored::Completion` as a real controller instead of a
    documented recipe. The item's own promotion trigger ("both ports
    end up copying the same code") is already known true — both port
    epics name '/'+'@' completion — and 0500's passive-panel substrate
    had to land here anyway (joint design); the controller is ~200
    lines the two ports would otherwise duplicate. The dropdown is a
    PASSIVE panel per 0500: keys stay with the composer; Down/Up
    navigate, Enter/Tab accept, Esc dismisses with same-token mute,
    typing refilters, clicking a row accepts, zero idle cost closed.
  - Non-goals honored: no IME path (TextInput posture), no undo stack,
    no readline/vi modes, no syntax highlighting.
- Validation (all green; whole tree 1,383 passed / 0 failed; clippy
  --all-targets zero; alloc pins `cargo test --test alloc_budget --
  --test-threads=1` 8/8):
  - Unit, model (textarea_tests.rs):
    `rowmap_tiles_every_byte_exactly_once` (the tiling invariant over
    ZWJ families/CJK/tabs/newlines × widths),
    `rowmap_soft_wrap_matches_editor_expectations`,
    `vertical_moves_keep_goal_column_over_wide_clusters`,
    `home_end_are_per_visual_row_and_ctrl_spans_the_document`,
    `history_edges_follow_the_arrow_nav_decision_table`,
    `history_store_preserves_the_draft_across_a_round_trip`,
    `cluster_atomic_edits_over_zwj_and_combining_marks`,
    `word_jumps_cross_line_boundaries`,
    `selection_spans_rows_and_replaces_on_type`,
    `scroll_window_follows_the_caret`.
  - Unit, widget: `typing_renders_inside_the_frame_and_wraps`,
    `grows_to_cap_then_scrolls_internally`,
    `enter_submits_and_alt_shift_enter_insert_newlines`,
    `enter_inserts_policy_never_submits`,
    `block_paste_inserts_whole_and_never_submits`,
    `history_recall_replaces_buffer_and_draft_survives`,
    `caret_cell_tracks_typing_and_clears_on_blur`,
    `placeholder_disabled_and_a11y`,
    `replace_range_snaps_to_cluster_boundaries`.
  - Unit, substrate + controller (anchored_tests.rs): placement matrix
    (below/flip/clamp/width/no-room), `top_z` (overlay_tests.rs),
    passive key/pointer contract, move-vs-remount, opener-scope-death
    (anchor-unmount safety), trigger/refilter/accept, highlight nav +
    Tab-accept, Esc mute, focus-loss/empty/mid-word, click-accept,
    anchor follow.
  - Acceptance (tests/wave_composer.rs, real Driver/CaptureTerm, wire
    bytes in / modeled VT out):
    `submit_vs_newline_chords_on_both_wires_and_history_recall` (kitty
    CSI `13;2u` + legacy ESC CR), `bracketed_paste_inserts_multiline_and_never_submits`,
    `completion_dropdown_full_round_trip_with_damage_containment`,
    `mouse_click_accepts_a_candidate_through_the_wire` (SGR bytes).
    Every test ends with `unknown_seq_count == 0`.
- Measured (debug, 44×12 CaptureTerm): dropdown-open frame 1,148
  bytes; highlight-move frame 136 bytes with every row outside the
  panel byte-identical (containment asserted row-by-row, not just by
  size); model ops are µs-class per keystroke (segments walk 1.88 µs
  on the text module's standing profile — three walks per keystroke).
- Left for the integrator / future items: completion over single-line
  `TextInput` needs the same wire (caret signal + caret cell +
  replace_range — a mechanical `TextInputState` mirror; not done here
  to keep TextInput untouched); 0500's OWNED + TOOLTIP popup modes and
  the select family remain (the substrate half is in); a general
  out-of-handler rect query would delete the two documented staleness
  warts (caret cell + growth width after a pure resize).

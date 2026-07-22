# 0140 — Real language lexers behind the Highlighter seam (stateful + diff)

## Metadata
- Created: 2026-07-21
- Status: Proposed — DIFF SLICE SHIPPED 2026-07-22 (see status note
  below); the stateful seam + python/js/toml presets remain the open
  scope and still need the seam-shape ruling before planning
- Track: app-widgets
- Completed: N/A

## Status note — 2026-07-22: the diff slice shipped, additively

The audit (`reviews/study/backlog-audit-2026-07-22.md` C5 + move 6)
re-ranked this item's options: post-0.2, widening `TokenKind` is a
0.3-window breaking change, while a separate diff vocabulary is additive
and shippable now. The live consumer (abstractcode-tui) renders diffs
TODAY, so the diff leg shipped on the additive path:

- **Vocabulary ruling (diff half) TAKEN**: separate `text::DiffKind`
  (Added/Removed/HunkHeader/FileHeader/Meta/Context), born
  `#[non_exhaustive]` per ADR-0003 §3 — no TokenKind churn, no theme
  reshape. `TokenKind` → `#[non_exhaustive]` is parked on the 0.3
  budget (`planned/0002_the_0_3_breaking_budget.md` entry 2), which
  also unblocks the future Type/Func kinds the stateful presets want.
- **Shipped**: `text::DiffLexer` (stateless line classification —
  deliberate: `CodeView` renders from a scroll offset, so cross-line
  state would tint scroll-dependently; the one `---`-content ambiguity
  is documented and pinned in tests), `widgets::code::diff_token_color`
  (added→`ok`, removed→`error`, hunk→`info`, meta→`text_muted`, file
  headers bold body ink — measured ≥3.0:1 on `surface_raised` across
  all 26 themes, test-pinned), `CodeView::lang(label)`, and diff-labeled
  markdown/Feed fence routing (one shared recipe, `diff_rich_line`).
- **Validation shipped**: unit tests (headers/hunk-split/no-newline
  marker/ambiguities), a 2k-case default-suite totality sweep, a
  5k-case `fuzz_big` campaign (`diff_lexer_5k`), the foreign-crate
  non_exhaustive idiom test (`adv_text`), and render-path cell asserts
  through CodeView and MarkdownView.
- **Deferred, honestly**: the stateful seam (`StatefulHighlighter` +
  adapter) and the python/js/toml presets — the seam SHAPE still needs
  its design ruling (this item's original caution stands), and per-item
  scope discipline kept it out of the diff cycle. The state-threading
  invariance property (whole vs incremental) belongs to that leg.
  NOTE for the seam design: statelessness is load-bearing for the diff
  lexer (scroll invariance); a stateful seam must decide how offset
  rendering re-seeds state (re-lex from top, cached line states, or
  documented approximation) before python lands.

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: possibly one small ADR: the fidelity stance ("approximate
  by design, never a language authority") should be written down before
  anyone tries to grow these lexers into parsers.

## Context
Any app that displays code displays it constantly: consoles and REPLs
(fenced blocks in output — python/js/rust/toml/shell), viewers and
pagers (source files, config files), and review/monitoring surfaces
where — most load-bearing — **diffs** carry the meaning (red/green
tinting is how a human scans a change at a glance). The completeness
review (P1-4, and "What remains" P2) rates the built-in lexer demo-grade
and names this the first of the two P1s that determine day-2 quality for
the console class; the coding-console port (0200) is the first
validator, and its tool-result patch previews are the strongest single
want.

## Current code reality
- `src/text/highlight.rs:41-44` — the seam is right and stays: `trait
  Highlighter { fn spans(&self, line) -> Vec<(Range<usize>, TokenKind)> }`
  — theme-agnostic byte ranges, colors mapped at the consumer
  (`code_token_color` in widgets/code.rs; markdown fences route through
  the same mapping, widgets/markdown.rs:14-16).
- `src/text/highlight.rs:12-18` — the built-in's honest limits, verbatim:
  lexes "ONE LINE at a time with no cross-line state, so block comments
  and string literals spanning lines mis-tint from the second line on; it
  knows C-family surface syntax only". `CLikeLexer` ships `rust()` and
  `c()` keyword presets (highlight.rs:68-88); the trait itself contracts
  "no cross-line state" (highlight.rs:42).
- `src/text/highlight.rs:23-36` — `TokenKind` is six buckets (Keyword,
  String, Number, Comment, Ident, Punct). A diff lexer has nothing to say
  in this vocabulary: inserted/deleted/hunk-header are not token kinds a
  C-like mapping can express.
- Themes map kinds to inks in one place; 26 themes exist. Any TokenKind
  widening touches that mapping and every theme's contrast audit.

## Problem
Three distinct gaps hide under "better highlighting": (a) no cross-line
state, so real python/js/toml files with multi-line strings or block
comments mis-tint; (b) no language presets beyond rust/c keyword lists;
(c) no diff support at all — and diff is line-oriented, needing new token
vocabulary rather than statefulness. Bundling them without a scope ruling
invites the rabbit hole the module doc warns against ("never a language
authority") — the reason this is proposed, not planned.

## What we want (proposed shape)
1. **A stateful seam variant**: `StatefulHighlighter` (or an extension of
   the existing trait) where per-line lexing takes and returns an opaque
   line-state token, carried down a block in order — matching how
   `CodeView`/markdown fences already iterate lines top-to-bottom. The
   stateless trait remains for stateless lexers; an adapter lifts one
   into the other.
2. **Presets**: python (strings incl. triple-quoted via state, comments,
   keywords, numbers), js/ts-lite (template literals via state), toml
   (tables, keys, strings, comments). Same fidelity bar as today:
   surface syntax, totality (any byte sequence lexes), no language
   authority claims.
3. **Diff lexer first**: line-oriented (`+`/`-`/`@@`/`diff --git`/index
   headers), fits even the stateless trait. Requires a vocabulary
   decision: add `TokenKind::Inserted/Deleted/Meta` (touches every theme
   + the contrast audit) vs. a separate `DiffKind` mapped by a dedicated
   consumer style (no theme churn, one more mapping). The vocabulary
   question is the item's main design ruling.
4. **Fuzz parity**: each new lexer joins the existing markdown+highlighter
   fuzz campaign (5,000 hostile cases, totality-asserting) and gets a
   split/state-threading invariant test (lexing a file whole vs.
   line-by-line with carried state yields identical spans).

## Scope / Non-goals
Scope: the stateful seam, three language presets, the diff lexer, theme
mapping decision, fuzz coverage. Non-goals: tree-sitter or any external
grammar dependency (the crate's austere dependency policy stands: four
deps today); semantic/LSP highlighting; shell (deferred — quoting rules
are a tarpit; revisit after the console port reports real need);
regex-driven grammar files (each lexer is honest hand-written Rust like
the current one).

## Expected outcomes
Tool-result diffs tint red/green in the console port; python/js/toml
fences stop mis-tinting after line one of a multi-line string; the seam's
"plug a real lexer later" promise (highlight.rs:7-9) is demonstrated true
by in-crate implementations.

## Validation
- Per-lexer golden tests (representative snippets, exact span asserts).
- State-threading invariance property (whole vs. incremental).
- Fuzz campaigns extended; zero panics, totality held.
- Theme: if TokenKind widens, the contrast audit covers the new kinds in
  all 26 themes.

## Progress checklist
- [ ] Design ruling: stateful seam shape (diff token vocabulary: RULED
      2026-07-22 — separate non_exhaustive `DiffKind`, additive)
- [ ] Stateful seam + adapter
- [x] Diff lexer + consumer mapping (2026-07-22)
- [ ] python / js / toml presets
- [ ] Fuzz + invariance + golden tests (diff legs done 2026-07-22:
      fuzz_big `diff_lexer_5k`, render-path goldens; invariance property
      belongs to the stateful seam leg)
- [x] Theme mapping — no vocabulary widening: semantic inks, measured
      ≥3.0:1 on the code ground across all 26 themes (2026-07-22)

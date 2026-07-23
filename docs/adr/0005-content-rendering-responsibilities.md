# ADR-0005: Content rendering responsibilities — markdown/code/data text is CORE

## Status

Accepted (wave 13, driven by the operator's markdown complaint against
the gateway console: "the markdown viewer is absolutely terrible and it
doesn't even scroll … you are not rendering tables either in
panels/components … I believe md should be a native capability of this
engine").

## Context

Three artifacts were surveyed before ruling, because the complaint names
all three: this engine, the operator's own `mdpad` viewer, and the
gateway console that produced the screenshot.

**The engine already ships markdown in core.** `render::md` parses the
honest core vocabulary (headings, lists, quotes, fences, rules, inline
emphasis/code/links/strikethrough) plus the DOC vocabulary (`parse_doc`:
GFM tables with alignment, task lists, block images), both with
streaming sessions (`StreamSession`, `DocStreamSession`) whose
`finish()` equals the batch parse for any chunking. `widgets::MarkdownView`
typesets the whole doc vocabulary with theme tokens (outline rows, TOC
anchors, find-with-highlights, in-flow mosaic images); `widgets::CodeView`
tints code through the pluggable `text::Highlighter` seam with a
line-oriented diff lexer beside it; the `Feed` transcript widget renders
the same vocabulary through the same `BlockTypesetter` (one recipe,
pixel-pinned). The question was never "should the engine do markdown" —
it does — but where quality work belongs and what each package owes.

**mdpad** (`lpalbou/mdpad`) is a shipped markdown READER/EDITOR app on
ratatui. Its dependency posture is the opposite of ours by design:
`pulldown-cmark` (full CommonMark + GFM: nested emphasis, nested
lists/quotes, footnotes, reference links, HTML passthrough), `syntect`
(full grammar-file syntax highlighting, several-MB lazy-loaded dumps),
`tui-textarea` (editor), `arboard`/OSC-52 (clipboard). What it renders
better than us today, honestly enumerated:

1. **Tables** — the README calls them "the reason this tool exists":
   browser-algorithm column sizing (natural width, WORD-WRAP minimum
   floor, proportional growth toward natural), cell WRAPPING into
   multi-line rows with box borders and mid-separators only when cells
   wrap, numeric columns auto-right-aligned, and a record layout
   ("Header: value" per row) as the honest fallback when even word
   minimums overflow. Ours: shared-solver columns, ellipsis truncation,
   no wrap, and (before this wave) columns crushed to zero silently.
2. **Full-grammar code highlighting** via syntect; ours is a demo-grade
   C-like lexer + a real diff lexer (and, from this wave, JSON/YAML).
3. **Deep nesting** — quotes containing blocks, lists containing
   blocks, footnotes; our core vocabulary deliberately folds quote
   nesting and keeps list items single-line (documented degradations).
4. **Link ergonomics** — clickable link ranges surviving wrap and table
   layout (a style-channel id trick), URL suppression inside tables.
5. **Reader chrome** — TOC pane, incremental search, selection +
   clipboard, editor mode, mermaid hand-off, file watching, statusbar.

**The gateway console** (`abstractgateway/console-tui`) renders model
output as BARE TEXT: the sandbox result path prints
`ellipsize(o.response.trim(), 90)` into a single plain line — no
`MarkdownView`, no `Feed`, no scroll, 90 chars then an ellipsis. The
screenshot's "terrible markdown" is a client that never called the
markdown machinery this engine ships. That is an ADOPTION gap. But two
ENGINE gaps are real and were reproduced from first principles:

- `Scroll::new(MarkdownView…)` measured ZERO content height (a draw
  widget with no intrinsic measure), so the composed pane never
  scrolled — the module docs even instructed apps to hand-roll offsets
  (`scroll_offset` + `MarkdownView::rows` clamps), which the reader
  example dutifully did per-app. The wave-12 handoff filed the same
  class against a plain element tree ("Scroll over a plain element tree
  collapses to zero"). "It doesn't even scroll" is thus a fair charge
  against the engine's DEFAULT composition, independent of the console.
- In an Auto-height panel (content-sized Modal/popup), a measureless
  `MarkdownView` solved to zero rows and rendered NOTHING — a document
  with a table inside a panel simply vanished. And at crush widths the
  table recipe clamped columns to zero IN ORDER, so rightmost columns
  silently disappeared while the left ones stayed full — degradation
  that reads as "tables don't render".

## Decision

**1. Markdown, code, diff, JSON and YAML text rendering are CORE — the
ruling stands, now with the quality to back it.** Every application
class this engine targets (consoles, chat transcripts, dashboards,
readers) renders model/tool output; that output is markdown with fenced
code, tables, JSON and YAML. The capability is pure cell math (parse →
typeset → cells) with zero dependency pressure — hand-rolled parsers and
lexers, the house discipline (PNG/JPEG/GLB precedent). It is already in
core; per ADR-0004 §6 ("does a minimal app pay for it in-tree, and does
it have its own release cadence?") it is neither costly enough to trim
nor cadence-separate: **neither = core**. The gap was QUALITY and
ERGONOMICS, not placement — closed by this wave: intrinsic measure on
`MarkdownView`/`CodeView` so `Scroll::new(…view…)` works out of the box
and content-sized panels size honestly; table crush honesty (per-column
floors, numeric auto-right-alignment, record-layout fallback — never a
silent vanish); hand-rolled JSON/YAML lexers on the `DiffKind` precedent
(a dedicated additive vocabulary, `TokenKind` being frozen-exhaustive
until 0.3) wired into `CodeView::lang` and fence routing.

**2. `abstracttui[md]` — REJECTED as a cargo feature.** Ruled per
ADR-0004's two feature classes: a trim feature must be heavy and
severable (the md stack is a few thousand lines of cell math with no
dependency weight — trimming it saves nothing measurable); an opt-in
feature exists for capability a minimal app must not silently carry
(markdown rendering is not a security or footprint hazard). A feature
gate would only fragment the widget set (`Feed` renders markdown items;
a `Feed` without markdown is a different widget) and violate the
additivity rule's spirit by making core widgets behave differently by
feature. Markdown is to this engine what text wrap is: vocabulary, not
option.

**3. Extensions own DIAGRAM-CLASS content, not text.** `abstracttui-mermaid`
already holds the ruling's boundary: content that needs LAYOUT SOLVING
of a new domain (graphs, sequence lanes, gantt) with its own release
cadence is a sibling crate (ADR-0004 §3). The same boundary admits
future content domains (music notation, plots beyond `Chart`) — but
JSON/YAML tinting, tables, and code fences are text on cells and stay
in. A fence labeled `mermaid` in core `MarkdownView` renders as code
(the honest core behavior); apps that want the diagram compose the
mermaid sibling — the extension REPLACES the fence's rendering, it does
not gate the document's.

**4. mdpad stays an APP — and is the quality bar, not a merge
candidate.** Its heavy deps (`pulldown-cmark`, `syntect`) are the right
call FOR A PRODUCT and impossible for this engine's five-crate law; its
chrome (TOC pane, search UX, selection/clipboard, editor, file
watching, multi-doc session) is app composition our reader example
already sketches with engine primitives. What core takes from mdpad is
the rendering behaviors that are pure cell math: the table minimum-floor
+ proportional-growth sizing, the record-layout fallback, numeric
right-alignment, level-legible headings (dim `###` prefix from H3) —
ported this wave; cell wrapping in tables (1200), OSC-8 hyperlinks
through the widget draw path (1210 — the engine's cell model already
carries link ids; the `StyledCanvas` seam ADR-0004 §5 names "when
landed" is the missing inch), and nested quote/list vocabulary (1220)
— filed as backlog. Syntect-class highlighting
stays out; the `text::Highlighter` seam is the documented door for apps
that want to plug a real grammar engine.

**5. The adoption half is owed BY CLIENTS, and the engine owes them the
one-liner.** The gateway console must render model output through
`MarkdownView` (static) or `Feed` (streaming) — evidence and the
one-paragraph note live in `reviews/wave13/console-md-adoption.md` (the
gateway seat's file). The engine's side of that contract, after this
wave: `Scroll::new(MarkdownView::new(text).view(cx)).view(cx)` is a
complete scrolling markdown pane with zero app-side plumbing.

**Consequences.**

- `MarkdownView`/`CodeView` gained intrinsic measure + a
  `basis(Cells(0))` default (flex behavior byte-stable — basis 0 + grow
  is exactly the previous effective geometry in definite parents;
  Auto-height parents now see true content height instead of zero,
  which is the RT8-6 collapse-class fix, not a break).
- The md table recipe now floors columns, right-aligns numeric columns
  without alignment markers, and degrades to a record layout below
  floor width. Goldens changed deliberately where the old pixels were
  the bug (zero-width columns).
- `TokenKind` stays frozen; `DataKind` (non_exhaustive, ADR-0003 §3) is
  the structured-data vocabulary beside `DiffKind`, mapped to theme inks
  in exactly one place (`widgets::code::data_token_color`).
- Alternatives recorded: full-CommonMark core parser (rejected — the
  honest-subset contract with documented degradations is a feature, not
  a debt; models emit the GFM subset we parse); markdown as a sibling
  crate (rejected — `Feed`/transcripts make it load-bearing for the
  default widget set); syntect-behind-a-feature (rejected — dependency
  law, and the Highlighter seam already admits it app-side).

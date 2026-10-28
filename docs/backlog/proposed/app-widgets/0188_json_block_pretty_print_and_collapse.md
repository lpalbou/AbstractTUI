# 0188 — JSON block: pretty-print + optional collapse over CodeView

## Metadata
- Created: 2026-10-28
- Status: Proposed
- Track: app-widgets
- Completed: N/A
- Related: 0570 (tree view, app-kits) for a full tree; 0140 (lexers).
- Root link: AbstractFramework backlog 1055; consumer: AbstractCode tool
  results (flat, unindented envelopes today).

## Inventory (what exists — reuse, do not rebuild)
- `CodeView` (src/widgets/code.rs): read-only syntax-coloured code, line
  numbers, theme `syntax_*` inks, `.lang("json"|"yaml"|"diff"|"rust"…)`
  via `JsonLexer`/`YamlLexer`/`CLikeLexer` — fenced code with colouring
  EXISTS (markdown fences route through it too).
- `text::data` tints key/value/literal tokens of JSON lines.
- `Disclosure` (src/widgets/disclosure.rs) collapses a region.

## What is missing (the only proposal)
`CodeView` colours JSON as given: a compact one-line envelope stays one
line. Add `JsonBlock::from_value(&serde_json::Value)` /
`JsonBlock::from_str` that PRETTY-PRINTS (configurable indent, stable
key order as received, long strings wrapped not truncated) and renders
through `CodeView` with `lang("json")`; optional collapse of objects /
arrays deeper than N via `Disclosure` (fold markers on the bracket
lines); invalid JSON falls back to the raw text with a one-line note.

## Validation
Headless: a compact envelope renders indented and coloured; collapse
toggles; invalid input shows raw + note; red without the pretty-printer.

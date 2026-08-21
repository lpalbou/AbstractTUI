# Changelog — abstracttui-mermaid

All notable changes to this crate are documented here (family crates
own their changelogs; core's CHANGELOG covers the engine). The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
SemVer.

## [0.3.0] - 2026-08-21

Requires `abstracttui-graph` 0.3.

### Added

- **Sequence control flow**: `alt` + `else`, `opt`, `loop`, and `par` +
  `and` render as labeled frames with dashed branch dividers, nested to
  any depth. Activations — `activate` / `deactivate` and the `->>+` /
  `-->>-` message suffixes — draw a bar on the lifeline that opens on
  its own arrow and steps right when one participant is active twice.
  In the IR a block is a TREE (`Block` holding `Branch`es): the parser
  balances it once and names any mismatch, so consumers recurse over
  something that cannot be malformed. Nesting is capped at 64 —
  layout, rendering and `Drop` all recurse, and a stack overflow aborts
  the host process.
- **Flowchart subset widened** to what people write: edge chaining
  (`A --> B --> C`), infix labels (`-- text -->`, `-. text .->`,
  `== text ==>`), `&` groups as a cross product, any arrow body length,
  the `x` / `o` heads and `<-->`, and five more node shapes
  (`((circle))`, `[[subroutine]]`, `[(cylinder)]`, `{hexagon}`,
  `>flag]`). The statement scanner is quote- and bracket-aware, so an
  arrow inside a label is text on either side of the link.
- **`subgraph` blocks flatten** instead of falling back: members
  render, the group box does not, and the loss is one notice per
  diagram naming the groups.
- **`MermaidFence`** — the claimant for the core's `widgets::FenceBlock`
  seam, so a ```` ```mermaid ```` fence in a `MarkdownView` renders as a
  diagram in place, in the document's own scroll surface. Renders once
  per (source, width, theme).
- `click`, `linkStyle`, `class` and a group-local `direction` joined
  the recognized-and-dropped row instead of aborting a whole diagram.

### Fixed

- Quoted labels kept their quotes (`-->|"yes, always"|` rendered the
  quote characters) in node text, edge labels, state transitions,
  participant aliases and message text.
- `<br/>` reached the screen as literal text inside a one-line card; it
  flattens to a word break, with a notice.
- A node declared twice used the FIRST declaration; mermaid uses the
  last, so the engine drew a different diagram than the source
  described.
- A header with no statements drew a blank rectangle, indistinguishable
  from a broken renderer; both diagram kinds now say so.
- The fallback's mermaid.live link was truncated to dead text — too
  long to read, impossible to copy, clickable nowhere. It is a
  hyperlink where the terminal does OSC 8, and wraps across rows where
  it does not.
- Sequence frame sizing measured only lifeline centres, so notes (which
  are filled boxes) and self-message labels erased the frame border;
  nested frames could share a border with their parent or run past it;
  divider labels truncated because the frame was sized from the opening
  label alone; and layout was quadratic in nesting depth.
- `activate` registered participants, inventing a lifeline for a typo
  and reordering columns when it preceded a participant's first
  message.

### Changed

- The cylinder and hexagon badge sigils are `⌸` and `◈`: the obvious
  picks (`⛁`, `⬡`) are absent from complete monospace fonts, which
  means a terminal draws tofu.
- `Block::branches` is `first` + `rest` rather than one `Vec`, so
  "never empty" is unrepresentable rather than documented.

## [0.1.0] - 2026-07-24

First release, published alongside `abstracttui` 0.2.13 and
`abstracttui-graph` 0.1.0 (which it depends on — ADR-0004 family
order). 45 crate tests green at release: a 30-fixture corpus (11
accepting / 19 falling back, mermaid v11 docs pin 2026-07-24), pinned
sequence goldens (including lifeline-crossing legibility), the BT/RL
odd-band mirror fixture, a no-panic fuzz battery (byte soup, token
soup, truncation sweeps), and the cycle-3 attack pins
(`tests/cycle3_attack.rs`).

### Added

- parser: hand-rolled subset parser over an EXHAUSTIVE spelling table
  (the crate-docs contract, from backlog 0450) — `parse() ->
  Result<Diagram, Unsupported>` is total and ATOMIC: the first
  statement outside the accepted spellings is the verdict (line
  number + verbatim line + named reason; targeted reasons for known
  v2 constructs: subgraph, infix labels, `&`-chaining, edge chaining,
  sequence blocks/activations, composite states). Supported:
  `flowchart`/`graph` TD/TB/LR/BT/RL (five node shape spellings,
  quoted bracket text, edges `-->`/`---`/`-.->`/`==>` with postfix
  `|label|`), `sequenceDiagram` (participants + aliases, four message
  arrows with required `: text`, `Note left of/right of/over`), and
  flat `stateDiagram-v2` (transitions, `: label`, `[*]` as synthetic
  start/end — a third front end to the flowchart IR).
  `classDef`/`style`/`%%{init}` directives are recognized-and-dropped
  with notices; `%%` comments drop silently.
- compiler: `to_graph(&FlowchartIr) -> (GraphDesc, LayeredOpts)` —
  mermaid is a COMPILER onto `abstracttui-graph`, never a second
  graph renderer. Shapes map to the view's vocabulary (kind accents
  `decision`/`rounded`/`stadium` + badge sigils ◆/○/◎); edge kinds
  map to the `dotted`/`thick` stroke hints (`---` carries an `open`
  hint).
- sequence rendering: deterministic solverless plan (lifeline columns
  from participant order, gaps from box halves + adjacent-pair
  message labels; message/note rows in source order; left-overflowing
  notes shift the picture instead of clipping) painted as cell glyphs
  (solid/dashed runs, filled/open arrowheads, self-message loops,
  note boxes; participant boxes on top).
- `MermaidView`: supported diagrams render natively; anything else
  renders the ATOMIC fallback — the source as a verbatim code fence,
  one notice naming the first unsupported construct, and an optional
  mermaid.live escape link (`live_link_url`: the editor's `#base64:`
  state form, URL-safe base64 — the diagram travels in the URL
  fragment only, never to a server).
- example `mermaid`: four embedded samples (TD, LR with labels +
  shapes, sequence, and a gantt falling back honestly) or a `.mmd`
  file argument; exits cleanly without a tty.

### Fixed

- sequence parser: a message or note that auto-registered a
  participant made a LATER explicit `participant id as Alias`
  silently drop the alias. The first explicit alias now ENRICHES the
  implicit registration (column order stays first-encounter; later
  aliases never re-label) — the crate's first-explicit-wins rule,
  now uniform across both diagram kinds. Failing-first:
  `cycle3_attack.rs::sequence_first_explicit_alias_wins_even_after_implicit_registration`
  (found and fixed by the cycle-3 attack battery, CANVAS seat).

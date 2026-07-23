# Changelog — abstracttui-mermaid

All notable changes to this crate are documented here (family crates
own their changelogs; core's CHANGELOG covers the engine). The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
SemVer.

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

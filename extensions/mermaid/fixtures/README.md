# Mermaid corpus fixtures

Real-world-shaped mermaid samples driving `tests/corpus.rs`. The
naming convention IS the assertion:

- `accept_*.mmd` — must parse to a diagram IR (one per YES row of the
  subset table in the crate docs, spellings covered exhaustively).
- `fallback_*.mmd` — must return the named `Unsupported` verdict (one
  per NO row, plus known v2 spellings: infix labels, `&`-chaining,
  edge chaining, activations, composite states, unknown arrows).

## Documentation pin

Mermaid has no spec grammar; the accepted spellings are pinned to the
**mermaid v11 documentation (mermaid.js.org — Flowchart, Sequence
diagrams and State diagrams pages), sampled 2026-07-24**. Accepting
fixtures are adapted from those docs' introductory examples (the
`graph TD;A-->B` four-node split, the Christmas shopping flowchart,
the Alice/John greeting sequence, the Still/Moving/Crash state
machine); fallback fixtures are adapted from the docs' examples for
the constructs the v1 table excludes. When the table grows, re-sample
against the then-current docs and update this pin.

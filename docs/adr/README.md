# Architecture Decision Records

Durable, cross-task engineering policy for this repository. Backlog items
(`docs/backlog/`) own execution planning and completion history; design
notes (`docs/design/`) own internal contracts and rationale essays; ADRs
own the *rulings* — decisions that outlive any one work item and that
code changes must not silently contradict.

This directory is the repository's first ADR system (established with
backlog item 0170; before it, nothing had decision-record status or a
supersession discipline). Conventions:

- Files are `NNNN-short-slug.md`, numbered globally, never renumbered.
- Reader-first shape: Title, Status, Context, Decision (consequences and
  alternatives fold into Decision where they help).
- Status is one of `Accepted`, `Superseded by ADR-NNNN`, or `Deprecated`.
  History is preserved: supersede, do not rewrite.
- A change that conflicts with an Accepted ADR either follows the ADR or
  ships a superseding ADR in the same change.

## Index

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-api-stability-policy.md) | API stability policy toward 0.2/1.0 | Accepted |
| [0002](0002-two-style-types.md) | The two `Style` types stay distinct; `LayoutStyle` is the documented spelling | Accepted |
| [0003](0003-struct-extensibility.md) | Struct extensibility: `non_exhaustive` capability structs, FRU style structs | Accepted |
| [0004](0004-extension-packaging.md) | Extension packaging: feature classes, sibling-crate family, dependency inheritance | Accepted |

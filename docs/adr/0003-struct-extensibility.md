# ADR-0003: Struct extensibility — `non_exhaustive` capability structs, FRU style structs

## Status

Accepted (2026-07-21). Executed for `term::Capabilities` and
`term::GraphicsCaps` in the same change.

## Context

Public structs with all-public fields make two downstream idioms
possible: exhaustive struct literals (including functional record
update, `Struct { field, ..Default::default() }`) and exhaustive
matching. Both turn every added field into a semver break. The crate's
most likely additive change is precisely "a new capability field"
(`term::Capabilities`, ~20 public fields, plain struct; robustness
review R6 flagged it), while style-class structs (`layout::Style`,
`render::Style`) have literal/FRU construction as their ergonomic core.
Before this ADR only the input types (`KeyEvent`, `MouseEvent`) were
`#[non_exhaustive]`.

Verified compiler semantics the ruling rests on (and which the doctest
on `Capabilities` pins as `compile_fail`):

- `#[non_exhaustive]` restricts FOREIGN crates only: struct literals
  and FRU still work inside the defining crate. This repository's OWN
  integration tests (`tests/`), examples and doctests compile as
  separate crates, so they count as downstream and need the
  constructor.
- FRU over `..Default::default()` on a NON-non_exhaustive struct is
  itself add-a-field-safe (new fields fill from the base expression), 
  so Default-anchored style structs can grow without breaking
  FRU-writing downstream code — non_exhaustive would only take away
  their ergonomics for no compatibility gain.

## Decision

1. **Capability-class structs are `#[non_exhaustive]` with a `with`
   constructor.** A capability struct is a grow-forever fact set the
   engine produces and applications mostly READ. Applied now:
   `term::Capabilities` and `term::GraphicsCaps` carry
   `#[non_exhaustive]`; each gains
   `with(f: impl FnOnce(&mut Self)) -> Self` (defaults, adjusted in
   place) as THE downstream construction path. Reading stays plain
   field access; mutating an owned value stays plain field assignment.
   New capability fields are additive under ADR-0001. `KeyEvent` and
   `MouseEvent` keep their existing non_exhaustive + documented
   constructor contract.

2. **Style-class structs stay plain with the FRU idiom.**
   `layout::Style`, `render::Style`, `Edges`, `Inset` and peers are
   construction-heavy author surfaces; their contract is "literal/FRU
   construction over `Default`". They stay exhaustive, and growth
   relies on the FRU-with-Default safety above. Consequence accepted:
   an exhaustive literal (no `..`) in downstream code can still break
   on field addition — the documented idiom is FRU, and doc examples
   must always write it.

3. **Classification rule for future types.** Ask "who constructs this,
   and does it accumulate facts over time?" Engine-produced,
   fact-accumulating: non_exhaustive + `with` (or a dedicated
   builder when construction has invariants). Author-written,
   shape-stable: plain struct + Default + FRU. Enums the engine may
   grow (event kinds, capability replies) are non_exhaustive; enums
   that are closed vocabularies by design (routing phases) are not.

4. **In-crate code may keep literals** for non_exhaustive types (the
   compiler allows it and `detect_env_with`/unit tests use it), but
   doctests and `tests/` must use the downstream idiom — they compile
   as foreign crates and are the compile-time proof the contract
   holds. `tests/wave_stability.rs::
   capabilities_construct_via_with_and_grow_without_breakage` pins it.

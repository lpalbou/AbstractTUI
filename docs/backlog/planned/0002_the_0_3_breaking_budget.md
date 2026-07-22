# 0002 — The 0.3 breaking budget

## Metadata
- Created: 2026-07-22
- Status: Planned — Accepted-pending-maintainer (the LIST is the policy
  artifact ADR-0001 §2 requires; each entry's ruling is recorded below
  and awaits maintainer sign-off before the 0.3 window opens)
- Track: cross-track (release governance, beside `0001_roadmap.md`)
- Completed: N/A

## ADR status
- Governing ADRs: ADR-0001 (API stability policy — §2 requires "a single
  budget list for the release" owned by `docs/backlog/`; this file IS
  that list for 0.3), ADR-0003 (struct/enum extensibility — the
  classification rule the entries below execute).

## Context

ADR-0001 §2: "Every intended break is written into a single budget list
for the release … Nothing breaking ships outside the budgeted release."
0.2.0 (published 2026-07-22) shipped WITHOUT that written list — the
2026-07-22 backlog audit (`reviews/study/backlog-audit-2026-07-22.md`
§S1/S2) records the breach: the intended budget vehicle (0170's audit)
never ran, and one technically-breaking change shipped undeclared
(`Role::TextArea` added to the public exhaustive `Role` enum; no live
victim — the one consumer, abstractcode-tui, never matches `Role`).

This document is the honest repair: the 0.3 budget written BEFORE the
window opens, plus the enforcement mechanism (the `semver` CI job wired
2026-07-22 gates every PR against the latest published release, so
nothing breaking can ship outside this list without a red check).

## The compliance record (what 0.2.0 actually broke)

| Change | Declared? | Victim? | Disposition |
| --- | --- | --- | --- |
| `term::Capabilities` + `GraphicsCaps` `#[non_exhaustive]` | Yes (CHANGELOG Changed + migration note) | none known | compliant |
| `Role::TextArea` added to exhaustive `ui::access::Role` | **No** (listed under Added, no migration note) | none (consumer greps clean of `Role::`) | recorded here; compat note added to CHANGELOG 0.2.0 section on 2026-07-22 |

## The 0.3 budget list

Every entry ships inside the one 0.3 release, each with a migration
note per ADR-0001 §2. Additive work never needs this list.

1. **`ui::access::Role` → `#[non_exhaustive]`, plus the `Tree` /
   `TreeItem` variants** (audit S1/S3; backlog 0570's named guard).
   `Role` is an enum the engine will keep growing — exactly ADR-0003
   §3's non-exhaustive class; 0.2.0's undeclared `TextArea` addition is
   the evidence. Adding the attribute is the one-time break; every
   later variant (0570 wants `Tree`/`TreeItem` in the same window)
   becomes additive. Migration note: downstream `match role` gains a
   `_` arm; control-plane serializers should render unknown roles by
   their `Debug` name. `Role::Select` and future role variants land AT
   THE END of the enum inside the 0.3 window (mid-enum insertion also
   shifts later discriminants — the live catch below; until then the
   select trigger reports `Role::Button` with its choice as the access
   value).

2. **`text::TokenKind` → `#[non_exhaustive]`** (audit C5 — RULING
   RECORDED: non-exhaustive, not "keep exhaustive + document").
   Rationale: the kind vocabulary is DESIGNED to grow — the theme's
   `syntax_type`/`syntax_func` inks already "stand ready for richer
   lexers" (widgets/code.rs), and richer lexers need `Type`/`Func`
   kinds to reach them; ADR-0003 §3 classifies engine-growable enums as
   non-exhaustive. The cost of the current freeze is already paid and
   demonstrated: 0140's diff slice (2026-07-22) shipped as a separate
   additive `DiffKind` (born `#[non_exhaustive]`) because widening
   `TokenKind` was a breaking change. Migration note: downstream
   `match kind` gains a `_` arm mapping unknown kinds to body text
   (the documented consumer rule in `code_token_color`).

3. **`Scroll::content_size` fate** (0130's deprecation posture,
   re-anchored). 0130 shipped the measured content extent and said
   "keep `content_size` working through 0.x; fold any removal into
   0170's 0.2 budget" — that window is spent. Decide at 0.3: mark
   `#[deprecated]` (pointing at measured extent) and remove at 0.4 per
   ADR-0001 §3, or keep it as the documented override forever. Default
   if unruled: deprecate at 0.3, remove no earlier than 0.4.

4. **`List` multi-row content reshape — CANDIDATE, not committed**
   (ADR-0001's context names it as known 0.x churn). Enters the list
   only if the reshape design lands before the window opens; otherwise
   it rolls to the next budget. Named here so it cannot ship "additively
   by accident" (the 0570 lesson).

## Enforcement

- The `semver` CI job (`.github/workflows/ci.yml`, wired 2026-07-22)
  runs cargo-semver-checks against the latest published crates.io
  release on every PR. Between windows it enforces additive-only; when
  the budgeted 0.3 PR lands, its intended failure is the cue to bump
  the version in the same PR.
- **Live catch, day one (2026-07-22, same working session)**: a local
  `cargo semver-checks --baseline-version 0.2.0` run flagged an
  in-flight `Role::Select` addition (src/ui/access.rs) — TWO major
  findings: `enum_variant_added` on the exhaustive enum, and, because
  the variant was inserted mid-enum, `enum_no_repr_variant_discriminant_
  changed` (`ScrollArea` 18 → 19, breaking any `as usize` cast). This is
  the exact 0570 trap entry 1 exists for. Integrator options before the
  next release: park the variant behind entry 1's 0.3 batch, or reuse an
  existing role until the window opens. (At minimum, a new variant must
  go at the END of the enum so discriminants keep their values.)
  RESOLVED 2026-07-22 (cycle 2, before any release): the variant was
  removed; the select trigger reports `Role::Button` (a select trigger
  IS a button opening a menu — the popup already reports
  `Menu`/`MenuItem`) with the current choice as its access value, and
  `Role::Select` is parked in entry 1's 0.3 batch. `cargo semver-checks
  --baseline-version 0.2.0` green after the removal.
- 1.0 criteria (ADR-0001 §4) tie to this list: the 0.3 budget paid down
  is one of the preconditions.

## Non-goals

MSRV bumps are not breaking-budget entries: the declared policy
(Cargo.toml / CONTRIBUTING, 2026-07-22) is that an MSRV raise is a
minor-version event declared in CHANGELOG — it rides any minor release,
budgeted or not.

## Progress checklist
- [x] Budget list written before the 0.3 window (this file, 2026-07-22)
- [x] 0.2.0 compliance record captured (Role::TextArea compat note in CHANGELOG)
- [x] Enforcement gate wired (semver CI job, verified locally)
- [ ] Maintainer sign-off on entries 1–3 rulings
- [ ] 0.3 window opens: entries land batched, one migration note each

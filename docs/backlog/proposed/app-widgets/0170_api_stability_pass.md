# 0170 — 1.0-track API stability pass (audit + breaking-change budget)

## Metadata
- Created: 2026-07-21
- Status: Proposed (an audit + policy item; its rulings gate the public
  shape of 0100/0130 and should precede a 0.2)
- Track: app-widgets (API honesty lane)
- Completed: N/A

## ADR status
- Governing ADRs: **None exist — this repository has no ADR system.**
  `docs/design/` holds eight design notes (00-vision through
  theme-identity) that function as internal contracts, but nothing has
  decision-record status, ownership, or a supersession discipline. ADR
  impact: **Needs new ADR** — the deliverable of this item includes the
  repo's first ADRs (at minimum: the stability/semver policy itself;
  candidates for retroactive capture: the damage contract, the
  write-only-clipboard security stance).
- Update (2026-07-21, wave cycle 1): the ADR system now exists —
  `docs/adr/` with README + ADR-0001 (stability policy), ADR-0002
  (two-`Style` ruling), ADR-0003 (struct extensibility, executed for
  `Capabilities`/`GraphicsCaps`). The paragraph above is kept as the
  record of the pre-item state. This item remains open for the audit,
  the 0.2 budget list, the doctest sweep, and the CI gate.

## Context
The honest critique this item encodes: the 0.1.0 API freeze landed in the
final review cycle, while the cycle before it was still removing public
items (`render::Style` was dropped from the prelude for RT8-1 in cycle 9,
per the prelude's own doc comment) — and the crate has zero external
users, so the freeze has never been validated by anyone who didn't write
the code. That is not an
indictment (every 0.1.0 starts this way) but it is a fact to manage
deliberately: the reviews forecast exactly where 0.2 breakage lands
(scroll/list — precisely where this track's 0100/0130 build), and a crate
that wants applications built on it owes them a written stability story
instead of a trickle of breaking point-releases.

## Current code reality
- `#[non_exhaustive]` protects only the input types: `KeyEvent`
  (src/input/mod.rs:251) and `MouseEvent` (src/input/mod.rs:410), with a
  documented constructor contract. `term::Capabilities`
  (src/term/caps.rs:17-18) is a plain `pub struct` with ~20 public fields
  and `derive(PartialEq, Eq)` — **not** non_exhaustive. Construction
  normally goes through `default()`/`detect_env()`, but any downstream
  exhaustive literal or exhaustive match makes adding one capability
  field semver-breaking (robustness review R6 flags it; new capability
  fields are the *most likely* additive change this crate will make).
- The two-`Style` collision: `layout::Style` and `render::Style` coexist.
  The prelude resolves it the right way for the common path —
  `LayoutStyle` alias exported, `render::Style` deliberately absent
  (src/prelude.rs:7-10) — but full-path users importing both modules
  still collide, and `src/widgets/list.rs:45-47` shows the internal cost:
  every widget imports one as an alias next to the other. Livable;
  needs an explicit ruling (rename one type at 0.2 vs. bless the alias
  convention permanently) rather than drift.
- Prelude curation (src/prelude.rs) is deliberate and documented
  (app-code surface only; engine/test types stay behind explicit
  imports). New surfaces from this track (Feed, TextArea, the 0150
  handle, `WakeHandle` — the robustness review suggests promoting it)
  each need a curation decision.
- Known 0.2 churn, named by the crate's own honesty notes:
  `Scroll::content_size` ("when a layout-query surface lands, the hint
  becomes optional — request filed", scroll.rs:11-14) and `List`
  multi-row content ("a later decision", list.rs:11-13). Items 0100/0130
  implement exactly these — their public shapes and this item's budget
  must be decided together.
- Doc-rot exposure: 23 `ignore`-fenced doctests never compile-check
  against the API (completeness review P2) — every future rename
  silently rots them.

## Problem
Stability today is implicit: no semver policy doc, no non_exhaustive
strategy beyond input types, no deprecation convention, no ADR system to
record any of it, and a known pile of 0.2 breakage with no declared
budget. Downstream applications (the two port epics are the first) cannot
tell which surfaces are load-bearing promises and which are 0.x scaffolding.

## What we want (proposed shape)
1. **Audit**: sweep the public surface (`cargo public-api` or rustdoc
   JSON) for: exhaustive-literal hazards (`Capabilities` first, then any
   public struct with all-pub fields), enums that will grow, derive
   contracts (`Eq` on `Capabilities` limits field types forever),
   accidental `pub` items. Produce a findings table with a
   keep/guard/break verdict each.
2. **Rulings** (each becomes ADR content): non_exhaustive + constructor
   policy for capability-class structs; the Style-collision endgame;
   prelude curation criteria (written test: what earns a prelude slot);
   deprecation convention (`#[deprecated]` for one minor, removal at the
   next breaking release).
3. **The 0.2 breaking budget**: one written list of every intended break
   — scroll/list reshapes from 0100/0130, any Style rename, Capabilities
   guarding — batched into a single 0.2 release with a migration note
   per break. Nothing breaking ships outside the list.
4. **First ADRs**: instantiate a minimal `docs/adr/` (or extend
   docs/design/ with decision-record headers — the mechanism is part of
   the ruling) recording #2 and the semver/MSRV policy (MSRV mechanics
   land via 0180).
5. **Doctest un-ignore sweep** for the fences that exist only because of
   API-shape uncertainty (completes the partially-executed RT8-8).

## Scope / Non-goals
Scope: audit, policy ADRs, the 0.2 budget document, Capabilities
guarding, doctest sweep. Non-goals: executing the 0.2 breaks themselves
(they ship with their owning items against the budget); a 1.0 declaration
(needs external users first — the budget names 1.0 *criteria*, not a
date); any runtime behavior change.

## Expected outcomes
A downstream author can read one document and know what is promised, what
will break at 0.2 and how to migrate, and where decisions get recorded;
`Capabilities` can grow a field without a major release; the two ports
build against surfaces whose stability class is explicit.

## Validation
- `cargo public-api` (or equivalent) diff gate wired so unreviewed public
  surface changes fail CI.
- Capabilities guarded + a downstream-style compile test proving field
  addition is non-breaking.
- ADRs exist, linked from CONTRIBUTING; the 0.2 budget is referenced by
  0100/0130 before their surfaces merge.
- Doctest ignore count reduced to only genuinely non-runnable fences.

## Progress checklist
- [ ] Public-surface audit + findings table
- [x] Rulings: non_exhaustive policy, Style collision, prelude criteria,
      deprecation convention
- [ ] 0.2 breaking-change budget (batched, migration-noted)
- [x] First ADRs recorded (incl. explicit "no prior ADR system" note)
- [x] Capabilities guarded + compile test
- [ ] Doctest un-ignore sweep
- [ ] public-api diff gate in CI

## Progress note (2026-07-21, wave cycle 1 — rulings subset executed)

Executed by the STABILITY seat to unblock the parallel widget work (the
full 1.0 pass stays open; this item stays in `proposed/`):

- **ADR system stood up**: `docs/adr/README.md` (conventions, index,
  explicit "first ADR system" note) + three reader-first ADRs
  (Title/Status/Context/Decision).
- **ADR-0001** — API stability policy toward 0.2/1.0: additive by
  default; breaking changes batched into minor bumps with one migration
  note per break; deprecation convention (`#[deprecated]` one minor,
  removal at the next breaking release); 1.0 criteria not date; scope
  definition (prelude = curated app subset — the prelude-criteria ruling
  lives in §5, refined per-type by ADR-0002 rule 2).
- **ADR-0002** — the two `Style` types stay distinct forever; no 0.2
  rename; `LayoutStyle` is THE documented spelling in app code;
  `render::Style` never enters the prelude.
- **ADR-0003** — struct extensibility: capability-class structs are
  `#[non_exhaustive]` + `with` constructor; style-class structs stay
  plain with the FRU-over-Default idiom; classification rule for future
  types; in-crate literals allowed, `tests/`+doctests must use the
  downstream idiom.
- **ADR-0003 executed**: `term::Capabilities` and `term::GraphicsCaps`
  are `#[non_exhaustive]`; each gained
  `with(f: impl FnOnce(&mut Self)) -> Self`; a `compile_fail` doctest on
  `Capabilities` pins that downstream literals/FRU no longer compile.
  All in-tree downstream-idiom construction sites converted
  (tests/adv_app.rs, tests/adv_image.rs, tests/adv_input.rs,
  tests/adv_overlay.rs, tests/integration_matrix.rs,
  tests/wave_livedata.rs, the `gfx::session::ImageSession` doctest);
  in-crate literals (src unit tests, `detect_env_with`) stay per
  ADR-0003 §4. Downstream compile proof:
  `tests/wave_stability.rs::capabilities_construct_via_with_and_grow_without_breakage`.

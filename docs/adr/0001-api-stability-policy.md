# ADR-0001: API stability policy toward 0.2/1.0

## Status

Accepted (2026-07-21).

## Context

abstracttui 0.1.0 is published with a deliberately curated public
surface, but its stability story is implicit: the 0.1.0 freeze landed in
the final pre-release cycle, the crate has no external users yet who
validated the shapes, and the crate's own honesty notes already name
known 0.2 churn (`Scroll::content_size`, `List` multi-row content — the
exact surfaces the app-widgets track is building on). Downstream
applications cannot tell which surfaces are load-bearing promises and
which are 0.x scaffolding, and nothing prevents a trickle of breaking
point-releases. This repository also had no ADR system to record any
such ruling (this file establishes it — see `docs/adr/README.md`).

## Decision

1. **Additive is the default.** Between breaking releases, changes to
   the public API are additive only: new items, new methods, new
   defaulted behavior, new fields on `#[non_exhaustive]` structs
   (ADR-0003). Anything else waits.

2. **Breaking changes ship batched in minor version bumps, with
   migration notes.** In the 0.x era the minor version is the breaking
   boundary (0.1 → 0.2), per Cargo semver practice. Every intended
   break is written into a single budget list for the release
   (`docs/backlog/` owns the list; item 0170 seeds the 0.2 budget), and
   the release notes carry one migration note per break — what changed,
   why, and the mechanical rewrite. Nothing breaking ships outside the
   budgeted release.

3. **Deprecation convention.** Where a surface can be kept alive
   cheaply, it is marked `#[deprecated]` (pointing at the replacement)
   for at least one minor release before removal at the next breaking
   release. Surfaces that cannot coexist with their replacement may
   break directly — but only inside a budgeted breaking release, with
   the migration note.

4. **1.0 has criteria, not a date.** 1.0 is declared only after
   external applications have built against a stable 0.x surface long
   enough to validate it (the two port epics are the first evidence),
   the 0.2 budget has been paid down, and a public-surface diff gate is
   wired in CI. Until then, 0.x minor bumps are the pressure valve.

5. **Scope.** "Public API" means everything reachable from
   `abstracttui::` without `#[doc(hidden)]`; the prelude is the curated
   app-code subset (engine/test types deliberately stay behind explicit
   imports). Behavioral contracts documented on items (doc comments,
   the damage contract) count as API: changing documented behavior is a
   break under this policy.

The rest of backlog item 0170 (the full public-surface audit, the
written 0.2 budget list, the doctest un-ignore sweep, the `cargo
public-api` CI gate) remains open work; this ADR is the policy those
deliverables execute against.

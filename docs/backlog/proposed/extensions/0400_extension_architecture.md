# 0400 — Extension architecture: cargo features + a sibling-crate family

## Metadata
- Created: 2026-07-22
- Status: Proposed (the track's foundational item — every 04xx item
  names its packaging against this ruling)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: ADR-0001 (API stability: additive default, batched
  breaks, docs/adr/0001-api-stability-policy.md:22-47) and ADR-0003
  (struct extensibility) constrain any answer.
- **ADR impact: a new ADR is REQUIRED** (working title: "ADR-0004 —
  extension packaging: features for in-tree trim, sibling crates for
  new domains"; the NUMBER is a placeholder — the control-plane track
  queues a protocol ADR too, and docs/adr/README.md numbers globally,
  so the integrator assigns at landing). Skeleton drafted:
  reviews/study/extensions-cycle3.md §1(c). This item's deliverable IS
  that ADR plus the policy text below; per docs/adr/README.md:18-19
  the ruling must land before any 04xx code contradicts or presumes
  it.

## Context
The maintainer's brief: "abstracttui should be modular, so we don't
overload the default package, and extensions should be installed only
when needed." Today the crate is one package with zero cargo features
(Cargo.toml has no `[features]` section at all) and a hard five-crate
dependency posture (Cargo.toml:16-34; docs/design/00-vision.md:46-53).
Everything ships to everyone: a dashboard that renders three sparklines
compiles the GLB loader, the software 3D rasterizer, the JPEG decoder
and three pixel-protocol encoders. Meanwhile the next capability wave
(node graphs 0430/0440, mermaid 0450, HTML subset 0470) would, if
landed in-tree, grow the default package by multiples of what the
maintainer wants to trim — the opposite of the brief.

"Installed only when needed" means two different things and the study
must keep them separate:
- For an **app author** (the crate's actual user), cargo features ARE
  the install-when-needed mechanism for in-tree code: `default-features
  = false, features = ["three"]` is one manifest line. Features cannot
  *add* domains that were never written, and they are additive-compile
  only — they trim, they do not modularize new work.
- For **new domains** (mermaid, graphs, HTML), a sibling crate
  (`abstracttui-mermaid`) is real modularity: separate version,
  separate release cadence, zero bytes in the default package. The cost
  is API-stability coupling: every batched 0.x break in core
  (ADR-0001 §2) forces a coordinated release of every sibling crate.

## Current code reality
- `Cargo.toml:1-42` — single package `abstracttui` 0.1.0, no
  `[features]`, no workspace. The manifest is integrator-owned by
  standing rule (Cargo.toml:18: "This manifest is owned by the
  integrator; agents request additions via reviews/") — executing this
  item is an integrator act.
- `src/lib.rs:31-47` — 17 top-level modules (incl. `prelude`), all
  unconditionally compiled. Measured (`wc -l`, includes in-file test
  modules): src totals ≈ 72.5k lines; `three/` 9,257; `gfx/` 6,776;
  `render/` 10,144; `widgets/` 9,111.
- Severability of the heavy modules (grepped `use crate::` per module,
  production code only — test-only imports are `#[cfg(test)]`,
  e.g. src/three/load.rs:727-730):
  - `three` is consumed by exactly two production sites:
    `src/widgets/viewport3d.rs` (the widget) and
    `src/boot/brandmark3d.rs` (the 3D splash — and the splash already
    has a 2D seam: `play_fallback`, src/boot/player.rs:427-436, plus
    the `SplashFrameSource` trait at player.rs:54). `three` itself uses
    `base` + `gfx` (+ `anim`/`boot`/`render`/`theme` only in
    `brandmark.rs`). Cleanly gateable.
  - The JPEG decoder (`gfx/jpeg.rs` 599 + `jpeg_dsp.rs` 182 +
    `jpeg_entropy.rs` 300 lines) is reached only through
    `decode_image`'s magic-byte routing (src/gfx/decode.rs:58-67),
    which already rejects unknown formats with a named error — the
    honest feature-off behavior exists as a pattern.
  - The pixel-protocol encoders (`gfx/proto/` kitty 333 + sixel 482 +
    iterm2 110) are pure `Bitmap -> Vec<u8>` functions
    (src/gfx/proto/mod.rs:1-7), but `gfx::ImageSession` and the app
    driver consume them (src/app/driver.rs:28,
    src/app/driver_images.rs:8) — gating them cuts into the driver, a
    deeper cfg surface than `three`.
  - `gfx` as a whole is NOT severable: `widgets::Bitmap` re-export
    (src/widgets/mod.rs:45), the Image widget, overlays
    (src/app/overlays.rs:35) and the mosaic fallback are core content
    paths.
- `src/prelude.rs:33-36` — `Viewport3D` and `Image` sit in the curated
  prelude; feature-gating must keep the prelude compiling in every
  feature combination (cfg on the re-export).
- ADR-0001 (docs/adr/0001-api-stability-policy.md:27-34): breaking
  changes batch into minor bumps with migration notes — this is the
  exact coupling a sibling-crate family must budget for.
- Prior art in the family's target domain: `mdpad` (the maintainer's
  markdown reader, /Users/albou/projects/gh/mdpad) is a single binary
  that REJECTED in-terminal mermaid ("a faithful text-grid layout
  engine is a multi-thousand-line subsystem — at odds with a lean
  single binary", mdpad src/render/mermaid.rs:1-14). The extension
  family is how AbstractTUI can afford what a lean single binary
  cannot.

## Problem
There is no ruling on how optional capability ships. Without one, every
new domain lands in-tree by default (package grows forever, against the
brief), or lands in ad-hoc external crates with no naming, versioning,
CI, or API-anchor policy (breaks on every core minor bump, silently
rots). The two failure modes are both already visible in the wild
ecosystem: monolith TUI crates nobody can trim, and orphaned
`<engine>-contrib` crates pinned to stale cores.

## What we want
One ADR-backed ruling with three parts:

1. **Features trim OR gate in-tree weight; sibling crates carry new
   domains.** Two feature classes, distinguished by default:
   - **Default-ON trim features** for code that is core-adjacent and
     heavy: `three` (3D), `jpeg`, `proto` (pixel protocols) — the
     out-of-box experience is unchanged and feature unification stays
     additive (`default-features = false` opts into the trim).
     Executed by 0410.
   - **Default-OFF opt-in features** for in-tree capability a minimal
     app must not silently carry — the control-plane band's serve
     surface (0320, a listening-socket security surface) is the first
     applied case: this study's cross-review places it as an in-tree
     `control-server` feature, default-off, NOT a sibling crate
     (rationale recorded in reviews/study/extensions-on-platform.md
     P1-2: its co-design with VirtualTerm/attach is highest-churn
     exactly now, and its protocol freezes with an engine ADR — no
     independent cadence yet). Feature-off = the constructor does not
     exist: compile-time absence is a stronger security posture than
     runtime opt-in. Both classes obey additivity (enabling never
     changes existing behavior).
   - Genuinely separate domains ship as sibling crates named
     `abstracttui-<domain>` (`abstracttui-graph`, `abstracttui-mermaid`),
     each depending on the published core (`abstracttui = "0.x"`) with
     ZERO private access — the widgets rule
     (src/widgets/mod.rs:5-6, "no private engine privileges")
     promoted to a family contract.
   - The classification question for future capability mirrors
     ADR-0003 §3's style: ask "does a minimal app pay for it if it is
     in-tree, and does it have its own release cadence?" Both yes =
     sibling crate. Minimal-app-cost yes but no independent cadence =
     feature (default-on if trim, default-off if opt-in surface).
     Neither = core.
2. **Coupling budget.** Sibling crates version-float on core's minor
   (`abstracttui = "0.2"`); each ADR-0001 breaking batch lists the
   family migration as part of its budget (the release is not done
   until the family compiles). Extension crates live in this
   repository as a cargo workspace (integrator restructure) so CI
   builds them against core HEAD and the coupling is caught at PR
   time, not at release time. Mechanics the ADR must name (peer
   review P2-6): family crates spell the core dependency dual-form
   (`abstracttui = { version = "0.x", path = "../.." }`) so workspace
   builds ride HEAD while crates.io publishes resolve the version;
   publish order is core first, family same day. **The five-crate
   dependency posture BINDS sibling crates** (same review-gated
   exception process as core, docs/design/00-vision.md:46-53) — 0450
   and 0470 hand-roll their parsers under this clause; the ADR states
   it explicitly. **Gate:** the family does not launch before the 0.2
   API-stability pass (backlog 0170) executes — anchor surfaces must
   be the post-audit shapes, or every extension is born into churn.
3. **The anchor surface is named.** Extensions build on: `ui::Element`
   + draw closures (src/ui/view.rs:20,155-158), `StyledCanvas`
   (src/ui/canvas.rs:22-77), `layout` styles, `reactive` signals,
   `theme::TokenSet`, `app::Overlays` (layer_draw/layer_tree/
   on_outside_press, src/app/overlays.rs:158-229 — 0430's tooltips
   already consume it; peer review P2-4) plus the anchored-popup
   primitive once app-kits 0500 lands it in core, and — after 0420 —
   the canvas/vector layer. The ADR lists this surface explicitly;
   anything an extension needs beyond it is a core backlog item first
   (applied case: the draw-closure link-registration seam, peer
   review P1-1 — authored as 0480 this band, producer half of 0165's
   link channel).

Also deliverable: the "non-goals of the extension system" list (in the
track README, ratified by the ADR): no dynamic loading/ABI plugins, no
scripting runtime, no discovery machinery beyond crates.io naming, no
private hooks, no behavior-changing features, token discipline holds.

## Compile-impact estimate (estimate, not measurement)
Per the study's constraint (do not modify the crate), impact is
estimated from source volume and the dependency graph, not from a
scratch feature build; 0410 measures for real (`cargo build --timings`)
when it executes.
- Shipped source (test files excluded): `three` ≈ 8.2k lines (~11-12%
  of the crate), JPEG trio ≈ 1.1k (~1.5%), protocol encoders ≈ 0.9k
  (~1.3%).
- The crate has no external deps beyond the five tiny ones, so build
  time tracks own-code volume (typecheck + codegen, roughly linear for
  a single-unit lib at `codegen-units = 1`): expect ≈10-13% full-build
  savings with `three` off; ≈15% with three+jpeg+proto off. Binary
  `.text` trim on the order of 100–400 KB at opt-level 3 (rasterizer
  and JPEG DSP are code-dense). These are estimates and labeled so.

## Scope / Non-goals
Scope: the ADR, the classification rule, the naming/versioning/CI
policy, the anchor-surface list, the workspace restructure decision
(integrator), the non-goals list. Non-goals: executing the feature
gates (0410); building any extension (0430+); dynamic
loading/scripting (never — see track README); splitting `gfx` or
`render` out of core (they are the content path).

## Expected outcomes
Every subsequent capability discussion starts from a written rule
instead of re-litigating packaging; a minimal app can honestly say what
it pays for; extension authors (including the app-kits band 0500-0590
and control-plane band 0300-0390, whose deliverables face the same
packaging question) inherit one policy.

## Validation
- The ADR exists, is indexed (docs/adr/README.md), and names the
  anchor surface + coupling budget (incl. the dual-form dependency
  spelling, publish order, and the posture-binds-siblings clause).
- Decision table applied to the known queue: three/jpeg/proto →
  default-on features (0410); control server → default-off feature
  (0320 placement, cross-review answer); graph/mermaid → sibling
  crates (0430-0450); HTML subset → sibling crate IF 0470 promotes;
  canvas layer → core (0420).
- Peer-band dry-run EXECUTED (cycle 2), outcome recorded: control
  plane splits bus-core / server-feature / bridges-out-of-crate
  (reviews/study/extensions-on-platform.md P1-2); app-kits choice
  controls and form machinery (0500/0510-class) classify as CORE —
  minimal apps want them and they have no independent cadence — so
  band 0500-0590 is NOT a sibling family by default; only a
  heavyweight composed kit would qualify (peer finding P2-5,
  accepted).

## Progress checklist
- [ ] Ruling drafted (features vs siblings vs core, classification rule)
- [ ] Coupling budget + 0170-gate agreed
- [ ] Anchor-surface list written
- [ ] Workspace restructure decision (integrator sign-off — Cargo.toml owner)
- [ ] ADR-0004 landed + indexed; track README non-goals ratified


## Status update (2026-07-23)

MAINTAINER APPROVED the hybrid architecture (lean core + optional
sibling crates + two feature classes). ADR-0004 is Accepted
(`docs/adr/0004-extension-packaging.md`, executed from this item's
skeleton). Remaining execution: the 0410 feature gates (integrator
Cargo.toml act, batched with a release window) and the workspace
scaffold when the first sibling crate (0430/0440 graph) starts.

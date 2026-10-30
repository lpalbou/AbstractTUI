# 0006 — Share avatar geometry across native and web renderers

Status: Accepted. Date: 2026-10-30.

## Context

AbstractTUI has two procedural avatars; watch SwiftUI has a larger design catalog.
Web/Qt clients should recognize the same styles without copying their geometry or
coupling generic rendering to Gateway, inference, microphone or preferences.

## Decision

Own platform-independent geometry, pose vocabulary and style metadata in the
zero-dependency `abstract-avatar` crate under `crates/abstract-avatar`. AbstractTUI
consumes it natively and retains terminal mosaic conversion, theme materials,
visibility/timing, captions and interaction. Optional bounded C/Wasm exports reuse
that exact core; AbstractUIC ships a generated Wasm artifact and thin Canvas/React
adapters. Native/Qt hosts can use the same optional C ABI. Application integrations
and existing watch implementation are migrated separately; watch codes are catalog
provenance, not new preference authority. Existing halo/hover keys stay valid. Cursor coordinates remain host-fed transient
view state, never identity or a global mouse listener inside the core. Ready gaze
expires after two seconds at the adapter; other phases/reduced motion ignore it.

The renderer accepts explicit pose/time/level/gaze and emits material/light/coverage.
Hosts supply palette lookup values; no literal application colors or UI toolkit
enter shared geometry. No registry/service/model or network call is introduced.
ABI handles bound dimensions and live buffers; destroy releases owned buffers.

## Alternatives and tensions

| Option | Ownership / cost | User effect / operations | Reversibility / gap |
|---|---|---|---|
| Separate Rust/TypeScript/Qt artwork | Simple local builds; duplicate math | Fast initial delivery; future visual drift | Easy local changes; no shared renderer |
| Declarative scene language | Neutral JSON owner; new interpreter per host | Portable but DSL/animation complexity | Harder schema migration; unnecessary interpreter |
| Shared Rust geometry with Wasm/C adapters | One geometry/catalog owner; small build artifact | Same silhouettes/motion; local-only rendering | Thin adapters; explicit artifact verification needed |

Choose the shared core. Palette and scheduling stay host-owned; pixel-perfect
colors require the same palette, but silhouette/motion have one source. Generated
Wasm is bundled so npm users do not need Rust. Maintainers regenerate it from the
crate; hashes and parity tests prevent stale artifacts. No automatic graphics-
protocol upgrades or silent alternate identity on failure.

## Validation and migration

Preserve old Halo/Hover behavior, transparency, clock budgets and cleanup. Exercise
all six styles/seven poses, small terminal/large canvas, reduced motion and bad
ABI inputs. Compare native core/Wasm output using identical palette/inputs. Supply
terminal and browser galleries. Entity identity/voice ownership remains Gateway's
existing separate responsibility; broad watch/Qt/app migration is follow-up work.

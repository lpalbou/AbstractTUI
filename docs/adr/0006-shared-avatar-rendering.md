# 0006 — Share avatar geometry across native and web renderers

Status: Accepted. Date: 2026-10-30. Updated: 2026-10-31 (standalone ownership and opt-in integration).

## Context

Terminal, web and native clients need shared procedural designs without copying geometry.
AbstractTUI is a general-purpose UI engine; its base must not require an avatar catalog or
application-specific activity behavior. Watch/iPhone implementations are design references and
remain outside this migration.

## Decision

Own platform-independent geometry, pose vocabulary and style metadata in the standalone
[AbstractAvatar](https://github.com/lpalbou/AbstractAvatar) repository and `abstractavatar` crate.
AbstractTUI's generic `FramePlayer` consumes application-provided RGBA bitmaps with no generator
dependency. The convenience `Avatar` widget is behind the default-off `avatar` feature and
optional `abstractavatar` dependency. No separate adapter crate is introduced.

AbstractTUI retains terminal mosaic conversion, theme materials, visibility/timing, captions and
interaction. Native C/Python and WebAssembly builds execute the same procedural core;
AbstractUIC presents frames through Canvas/React. Hosts need no independent geometry interpreter.
Watch codes are catalog provenance, not new preference authority. Existing halo/hover keys stay
valid. Cursor coordinates remain host-fed transient
view state, never identity or a global mouse listener inside the core. Ready gaze
expires after two seconds at the adapter; other phases/reduced motion ignore it.

The renderer accepts explicit pose/time/level/gaze and optional audio bands, and emits
material/light/coverage samples or bounded RGBA frames. A versioned descriptor selects trusted
built-in generation; it is not executable code or a new scene language.
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

Preserve existing style behavior, transparency, clock budgets and cleanup. Exercise
the full runtime catalog across seven states, small terminal/large canvas, reduced motion and bad
ABI inputs. Compare native core/Wasm output using identical palette/inputs. Supply
terminal and browser galleries. The base crate must compile without the avatar feature and its
dependency in the compilation graph. Entity identity/voice ownership remains Gateway's existing
separate responsibility; watch/iPhone adoption requires a separate authorized task.

# File ownership (strict — parallel agents, zero merge conflicts)

Integrator only: `Cargo.toml`, `src/lib.rs`, `src/base/**`, `src/prelude.rs`,
`OWNERSHIP.md`, `docs/design/00-vision.md`.

| Area | Owner | Paths |
| --- | --- | --- |
| Terminal kernel | KERNEL | `src/term/**`, `src/input/**` |
| Render core | RENDER | `src/render/**`, `src/text/**`, `src/anim/**` |
| Reactivity & UI | REACT | `src/reactive/**`, `src/layout/**`, `src/ui/**`, `src/app/**` |
| Widgets (behavior) | REACT | `src/widgets/**` (shared with DESIGN — split by file) |
| Graphics & 3D | GFX3D | `src/gfx/**`, `src/three/**` |
| Themes, boot, demos | DESIGN | `src/theme/**`, `src/boot/**`, `examples/**` |
| Test rig, fuzz, benches | REDTEAM | `src/testing/**`, `tests/**`, `benches/**`, `fuzzish/**` |
| Reviews (any agent) | all | `reviews/cycleN/<attacker>-on-<topic>.md` |
| Design notes | owner | `docs/design/<area>-*.md` |

Rules:
1. Never edit outside your paths. Cross-module needs -> file a review note or
   leave a `// CONTRACT(<owner>):` comment request in YOUR file.
2. Widgets: one file per widget; REACT owns behavior-heavy ones (input,
   list, table, scroll, tabs, button), DESIGN owns visual-heavy ones
   (block, progress, spinner, separator, badge, logo), GFX3D owns the
   media ones (image, viewport3d). The file header names its owner.
3. Shared types live in `src/base` (integrator). Propose additions via review.
4. Do not run `cargo fmt` repo-wide (touches others' files). Format only
   your files.
5. Builds: prefer `cargo check` / targeted `cargo test <path>`; the shared
   target dir lock serializes heavy builds — keep them short.
6. Never `git commit`. Never mention AI tools/agents as authors anywhere.
7. Binding integrator rulings live in `docs/design/01-damage-contract.md`
   (frame phases, damage epoch, presenter byte custody, cursor policy,
   one theme signal, base additions). Read it before building loop-,
   damage-, or presentation-adjacent code.

# Contributing to AbstractTUI

Thank you for considering a contribution. AbstractTUI is a reactive,
compositor-grade terminal UI engine written in Rust with a deliberately
small dependency footprint. This document covers how to build, test, and
submit changes.

## Building

```sh
cargo build
```

The crate targets stable Rust (edition 2021) and builds from std plus five
small, permissively licensed dependencies (see `Cargo.toml`). To try it out,
run any of the 12 examples:

```sh
cargo run --example hello
cargo run --example dashboard
cargo run --example viewer3d
```

See `examples/README.md` for the full list.

## Testing

The default test pass runs the whole suite — roughly 1,385 tests across unit
tests, the integration suites under `tests/`, and doctests:

```sh
cargo test
```

Two suites are ignored by default and run explicitly:

```sh
# Live pty smoke tests — spawn a real terminal session; run serially.
cargo test --test live_smoke -- --ignored --test-threads=1

# Performance budgets — meaningful only in release builds; run serially.
cargo test --test perf_budgets --release -- --ignored --test-threads=1
```

### Golden snapshots

Snapshot goldens live in `tests/goldens/`. To (re)mint them deliberately:

```sh
UPDATE_GOLDENS=1 cargo test
```

A missing golden fails with instructions rather than self-minting, so CI can
never create unreviewed truth. A golden change in a pull request is a
semantic claim ("the screen now looks like this on purpose") — update goldens
only for deliberate behavior changes, and say so in the PR description.

## Lint gates

The tree is kept rustfmt- and clippy-clean. Before submitting:

```sh
cargo fmt --all
cargo clippy --all-targets
```

Clippy is expected to produce zero warnings.

## Documentation

```sh
cargo doc --no-deps
```

API documentation should build without warnings. Design documentation lives
under `docs/design/`.

## Architecture: the layering rule

Modules form a strict stack, and lower modules never import upper ones:

```
base → term / input → render / text / anim → reactive → layout
     → ui → widgets / gfx / three → theme → app → boot
```

`testing` cuts across layers (headless test terminal, VT interpreter,
harness utilities). If a change seems to need an upward import, the design
needs restructuring — do not add the import.

## Code conventions

- **No `unsafe`** outside the platform FFI boundary in `src/term/` and the
  test pty helper (`src/testing/pty.rs`).
- **Small files**: aim for under ~600 lines per file; split modules rather
  than growing monoliths.
- **Tokens-only styling in widgets**: widgets resolve theme tokens, never
  raw colors. A lint test in `src/widgets/mod.rs` enforces that no hex
  literals or color arithmetic appear in `src/widgets/`, and a companion
  count check ensures every widget module is on the lint list.
- **Dependencies**: adding a dependency is a significant decision. Open an
  issue to discuss it before writing code against a new crate.

## Adding a widget

1. Create `src/widgets/<name>.rs` and declare it in `src/widgets/mod.rs`.
2. Style exclusively through theme tokens (see the lint rule above), and add
   your file to the `include_str!` lint list in `src/widgets/mod.rs`.
3. Add tests alongside the widget and, where rendering is involved, golden
   snapshots.

## Adding a theme

Theme values are data: add a seed to `src/theme/seeds.rs` and register it
through the theme registry. Every registered theme must pass the contrast
audit (readability floors are tested, not aspirational). See
`docs/design/theme-identity.md` for the token model, derivation rules, and
audit policy.

## Pull requests

- Include tests with behavior changes; keep the tree green
  (`cargo fmt`, `cargo clippy`, `cargo test`).
- Keep PRs focused — one concern per PR reviews faster.
- Update documentation (`docs/`, rustdoc) when behavior or public API
  changes.
- By contributing, you agree that your contributions are licensed under the
  MIT license that covers the project.

## Windows

Windows support is cross-checked with:

```sh
cargo check --target x86_64-pc-windows-msvc
```

Reports from live runs on Windows terminals are especially welcome — please
include the terminal emulator and version in the issue.

## Questions

Open an issue at <https://github.com/lpalbou/abstracttui/issues>.

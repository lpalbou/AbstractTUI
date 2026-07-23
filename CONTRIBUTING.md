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
run any of the 17 examples:

```sh
cargo run --example hello
cargo run --example dashboard
cargo run --example viewer3d
```

See `examples/README.md` for the full list.

## Testing

The default test pass runs the core suite — roughly 1,940 tests across unit
tests, the integration suites under `tests/`, and doctests. The repo is a
cargo workspace since the extension family landed: `--workspace` adds the
family crates (`extensions/graph`, `extensions/mermaid`) for roughly 2,060
tests total, and is what CI gates on:

```sh
cargo test              # core
cargo test --workspace  # core + the extension family (the CI gate)
```

Three suites are ignored by default and run explicitly:

```sh
# Live pty smoke tests — spawn a real terminal session; run serially.
# (CI runs these on ubuntu in the `live pty (ubuntu)` job.)
cargo test --test live_smoke -- --ignored --test-threads=1

# Performance budgets — meaningful only in release builds; run serially.
cargo test --test perf_budgets --release -- --ignored --test-threads=1        # engine primitives
cargo test --test perf_app_surfaces --release -- --ignored --test-threads=1  # app-layer surfaces
```

Timing budgets are load-sensitive: run them on a quiet host, and treat a
red timing on a loaded machine as a re-run signal, not a regression
(allocation and byte-count asserts are load-independent and always hold).
The app-surface suite also ratchets byte emission: printed byte medians
are asserted against quiet-host baselines, so an emission regression
cannot hide behind a busy host. The scheduled `perf.yml` workflow runs
both perf suites plus `fuzz_big` and the soak weekly on a hosted runner
(retrying timing suites once to absorb load noise) and uploads the
printed measurements as an artifact.

### Minimum supported Rust version (MSRV)

The crate declares `rust-version = "1.87"` in Cargo.toml (the floor is
set by the library's own std usage — `is_multiple_of`, stabilized 1.87 —
which sits above the windows-sys 0.61 target floor of 1.71). CI checks
it with a pinned toolchain: `cargo +1.87.0 check --all-targets --locked`.

Bump policy: raising the MSRV is a **minor-version** event — never a
patch release — and is declared in `CHANGELOG.md` with the new floor and
the feature that forced it. Code changes must not raise the floor
silently: if the MSRV job goes red on a new std API, either replace the
call or bump the declaration (and the CI pin) deliberately in the same
change, with the CHANGELOG line.

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

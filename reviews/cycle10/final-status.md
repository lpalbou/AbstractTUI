# AbstractTUI — final publication-readiness status (cycle 10 close)

Date: 2026-07-21. Verified on macOS (Apple Silicon), rustc 1.96.1.

## Verification battery (all green)

| Check | Result |
| --- | --- |
| `cargo test` (default suite) | 1242 passed, 0 failed |
| `cargo test --doc` | 32 passed, 0 failed |
| Live pty smoke (12 examples, real pty, `--ignored`) | 14 passed, 0 failed — exit 0, zero unknown VT sequences, terminal restored |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets` | 0 warnings |
| `cargo check --target x86_64-pc-windows-msvc` | 0 errors |
| `cargo package` (build from tarball) | verified OK (289 files) |
| `mdbook build` (docs site) | OK |
| Allocation pins (`--test alloc_budget`) | 8/8 (idle frames: 0 allocs / 0 bytes) |

## Shipped surface

- ~64,700 lines of library Rust + ~17,300 lines of tests/examples;
  12 runnable examples; 26 themes; 20+ widgets; images via 4 channels;
  GLB 3D with animation/skinning/textures; 2s boot identity splash.
- Docs: README + 7 docs pages + policy set + llms.txt/llms-full.txt +
  mdBook site config + text screenshots under docs/captures/.
- CI/CD: ci.yml (unix matrix + windows lib gate + lint), release.yml
  (verify -> crates.io trusted publishing -> GitHub release), docs.yml
  (mdBook + rustdoc to GitHub Pages), dependabot. One-time setup steps
  in .github/SETUP.md.

## Findings ledger disposition

All review findings filed during development (RT1-* .. RT8-*) are closed
or documented as known limits in the public docs; per-cycle detail lives
in reviews/cycle*/redteam-findings*.md. Known limits carried into docs:
Windows compile-verified only; JPEG baseline-only; sixel single palette
per emission; animation LINEAR/STEP; mosaic 2-color-per-cell; perf
numbers are quiet-box medians.

#!/usr/bin/env bash
#
# Release preflight — run EVERY CI gate locally, before tagging.
#
#     tools/preflight.sh            # the gates CI runs on every push
#     tools/preflight.sh --live     # + the serial live-pty suite
#
# This script exists because a release went out red twice for reasons a
# local `cargo test` can never catch:
#
#   1. A LINT THAT ONLY EXISTS ON A NEWER TOOLCHAIN. CI uses
#      `dtolnay/rust-toolchain@stable`, which is whatever stable is
#      TODAY; a laptop that has not run `rustup update` is behind, and
#      clippy's newest lints fire only on the runner. This script
#      refuses to pass until the local stable matches, and tells you how.
#   2. A SEMVER BREAK. `cargo test` is perfectly happy with a new enum
#      variant or a lost auto-trait impl; downstream compilation is not.
#      The semver gate is the only thing that sees it, and it is the
#      cheapest gate here (~5s) — never skip it.
#
# Every command below is copied from .github/workflows/ci.yml and
# release.yml. If you change one there, change it here.
set -uo pipefail

cd "$(dirname "$0")/.."

fail=0
step() {
  local name="$1"; shift
  printf '\n\033[1m== %s\033[0m\n' "$name"
  if "$@"; then
    printf '\033[32m   ok\033[0m — %s\n' "$name"
  else
    printf '\033[31m   FAILED\033[0m — %s\n' "$name"
    fail=1
  fi
}

# --- toolchain currency (the trap that produced this script) ----------
printf '\033[1m== toolchain\033[0m\n'
rustup update stable >/dev/null 2>&1 || {
  printf '\033[33m   warning\033[0m — `rustup update stable` failed (offline?).\n'
  printf '   Local lints may be older than CI'"'"'s. Re-run when online.\n'
}
printf '   %s\n   %s\n' "$(rustc --version)" "$(cargo clippy --version)"

# --- the CI jobs, in the order they usually go red --------------------
step "fmt (workspace)"        cargo fmt --all --check
step "clippy (deny warnings)" cargo clippy --workspace --all-targets -- -D warnings
step "rustdoc (deny warnings)" env RUSTDOCFLAGS=-D\ warnings cargo doc --no-deps --workspace
step "build (all targets)"    cargo build --workspace --all-targets
step "test (workspace)"       cargo test --workspace
step "msrv (1.87)"            cargo +1.87.0 check --all-targets --locked

# --- semver vs the latest published release ---------------------------
# `--baseline-version` is deliberately not pinned here: cargo-semver-checks
# resolves the latest release from the registry, exactly as the CI action
# does with no baseline argument.
if command -v cargo-semver-checks >/dev/null 2>&1; then
  step "semver (core)"    cargo semver-checks --package abstracttui
  step "semver (graph)"   cargo semver-checks --package abstracttui-graph
  step "semver (mermaid)" cargo semver-checks --package abstracttui-mermaid
else
  printf '\n\033[33m== semver — SKIPPED (not installed)\033[0m\n'
  printf '   cargo install cargo-semver-checks --locked\n'
  printf '   This gate is the one a local test suite cannot replace. Install it.\n'
  fail=1
fi

# --- packaging, as the release workflow does it -----------------------
step "package (core, dry run)" cargo package --allow-dirty --quiet

if [ "${1:-}" = "--live" ]; then
  step "live pty (serial)" cargo test --test live_smoke -- --ignored --test-threads=1
fi

printf '\n'
if [ "$fail" -eq 0 ]; then
  printf '\033[32m\033[1mpreflight green\033[0m — safe to tag.\n'
else
  printf '\033[31m\033[1mpreflight red\033[0m — fix the above BEFORE tagging.\n'
  printf 'A semver failure means bump the version (ADR-0001), not soften the gate.\n'
fi
exit "$fail"

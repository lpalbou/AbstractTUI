# One-time repository setup

Manual steps for the maintainer to activate CI/CD and the documentation site.
Everything below is done once; after that, releases and docs are automated.

## 1. Push to the GitHub repo

`github.com/lpalbou/abstracttui` already exists. From this directory:

```bash
git init -b main            # if not already a git repo
git remote add origin git@github.com:lpalbou/abstracttui.git
git add -A && git commit -m "Initial import"
git push -u origin main
```

CI (`ci.yml`) runs on every push to `main` and on pull requests.

## 2. Enable GitHub Pages

Repo **Settings → Pages → Build and deployment → Source: GitHub Actions**.

The `Docs` workflow (`docs.yml`) then publishes on every push to `main` that
touches `docs/**`, `book.toml`, or `src/**` (or run it manually via
workflow_dispatch):

- Guide (mdBook from `docs/`): `https://lpalbou.github.io/abstracttui/`
- API reference (rustdoc): `https://lpalbou.github.io/abstracttui/api/abstracttui/`

The rustdoc tree is copied into the site under `/api/` by CI; the mdBook
SUMMARY deliberately lists only the guide chapters. If you want a visible
"API reference" link in the book navigation later, add a line to
`docs/SUMMARY.md` pointing at a small page that links to `/abstracttui/api/`
(and docs.rs, which publishes automatically after the first crates.io
release).

## 3. Configure crates.io Trusted Publishing

`release.yml` publishes with a short-lived OIDC token
(`rust-lang/crates-io-auth-action`) — no long-lived secret in the repo.

On crates.io (logged in as the crate owner):

- **New crate (first publish):** Trusted Publishing can only be configured
  for an existing crate, so either
  - use the **Pending Publisher** flow (crates.io account settings →
    "Publishing tokens / Trusted Publishing" → add a pending publisher for
    crate `abstracttui`) *before* the first CI publish, or
  - do the very first `cargo publish` manually with an API token from your
    machine, then continue below.
- **Existing crate:** crate page → **Settings → Trusted Publishing → Add**:
  - GitHub owner: `lpalbou`
  - Repository: `AbstractTUI` (exact GitHub capitalization — the OIDC claim
    reports `lpalbou/AbstractTUI` and the match is on that string)
  - Workflow filename: `release.yml`
  - Environment: `crates-io`

In the GitHub repo, create the environment **Settings → Environments →
`crates-io`** (it must match the crates.io config; optionally add yourself
as a required reviewer so every publish needs a click of approval).

**Fallback (classic token):** if you prefer not to use Trusted Publishing,
create an API token on crates.io (scope: `publish-update`, and
`publish-new` for the first release), store it as the repository secret
`CARGO_REGISTRY_TOKEN`, and in `release.yml` comment out the
"Authenticate with crates.io" step and swap the two `CARGO_REGISTRY_TOKEN`
lines in the `cargo publish` step (the fallback line is already there,
commented).

## 4. Protect `main` (optional, recommended)

**Settings → Branches → Add branch ruleset** for `main`: require the CI
status checks (`test (ubuntu-latest)`, `test (macos-latest)`,
`test (windows, lib only)`, `lint (clippy + rustdoc)`) to pass before
merging, and require pull requests if you want review gates.

## 5. Cutting a release

1. Bump `version` in `Cargo.toml`.
2. Update `CHANGELOG.md` with a dated heading for the new version.
3. Commit, then tag and push:

   ```bash
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```

The `Release` workflow verifies (full unix test battery, clippy, rustdoc,
tag == `Cargo.toml` version, `cargo package --list`, `cargo publish
--dry-run`), publishes to crates.io, and creates a GitHub Release with
auto-generated notes.

**Rehearsal:** run the `Release` workflow via workflow_dispatch with
`publish` left at `false` — it runs the whole verify job (minus the tag
check) and stops before publishing. Setting `publish=true` publishes from
whatever ref you dispatched on, so prefer tags for real releases.

## Follow-ups / deliberate omissions

- **`cargo fmt --check` is enforced in CI.** The tree was formatted with
  rustfmt on 2026-07-21 and the `lint` job gates on it.
- **No MSRV job.** `Cargo.toml` declares no `rust-version`. When you decide
  on a minimum supported Rust version, add `rust-version = "..."` to
  `Cargo.toml` and a CI job pinned to that toolchain.
- **Windows runs lib tests only.** The integration harness drives real
  terminals through a unix pty helper (cfg-gated), so `cargo test --lib` is
  the Windows gate for now. A Windows ConPTY harness would close the gap.
- **Local `mdbook build` output.** Building the book locally writes to
  `book/` at the repo root (CI does the same on the runner). Consider adding
  `/book` to `.gitignore` so a local build is never committed. `book.toml`
  and `.github/` are already excluded from the crates.io package via
  `Cargo.toml`'s `exclude` list.

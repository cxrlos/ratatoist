# Releasing

This document describes how releases are created and published for ratatoist.

## Version scheme

Each crate follows [Semantic Versioning](https://semver.org/):

| Bump | When | Example |
|------|------|---------|
| `patch` (0.1.**1**) | Bug fixes, docs, refactors | Fix comment rendering |
| `minor` (0.**2**.0) | New features, backward compatible | Add search, delete task |
| `major` (**1**.0.0) | Breaking changes | Config format change, API redesign |

## Automated release flow

```
PR with label ──► merge to main ──► bot bumps version + tags ──► release builds ──► crates.io publish
```

### How it works

1. Open a PR to `main`.
2. Add a label: `major`, `minor`, or `patch`.
3. Merge the PR.
4. The **version-bump** workflow automatically:
   - Reads the PR label to determine bump type
   - Calculates the next version from the latest git tag (defaults to `v0.1.0` if no tags exist)
   - Updates `Cargo.toml` workspace version
   - Commits and creates a git tag
5. The **release** workflow triggers on the new tag and:
   - Builds release binaries for 4 targets (macOS aarch64/x86_64, Linux x86_64/aarch64)
   - Creates a GitHub Release with binaries and SHA256 checksums
   - Publishes `ratatoist-core` and `ratatoist-tui` to crates.io

### No label = no release

If a PR has no `major`/`minor`/`patch` label, no version bump or release happens. Use this for PRs that shouldn't trigger a release (docs-only, CI changes, etc).

## Required GitHub secrets

Add in **Settings > Secrets and variables > Actions**:

| Secret | Purpose | How to get it |
|--------|---------|---------------|
| `CARGO_REGISTRY_TOKEN` | Publish crates to crates.io | [crates.io/settings/tokens](https://crates.io/settings/tokens) -- scopes: `publish-new`, `publish-update` |

`GITHUB_TOKEN` is provided automatically by GitHub Actions.

## Rolling back a release

If a release needs to be reverted:

1. Go to **Actions > Rollback Release > Run workflow**
2. Enter the version to rollback (e.g. `v0.2.0`) and a reason
3. The workflow will:
   - Delete the git tag (local + remote)
   - Delete the GitHub Release and its artifacts
   - Restore the previous version in `Cargo.toml`
   - Commit the rollback

> **crates.io**: Published crate versions cannot be deleted, only yanked. To yank:
> ```sh
> cargo yank --version 0.2.0 ratatoist-core
> cargo yank --version 0.2.0 ratatoist-tui
> ```

## Manual release (fallback)

If the automation fails or you need a manual release:

```sh
# 1. Bump version in Cargo.toml
# 2. Update CHANGELOG.md
# 3. Commit, tag, push
git add -A
git commit -m "release: v0.2.0"
git tag v0.2.0
git push origin main --tags

# 4. Publish to crates.io
cargo publish -p ratatoist-core
sleep 30
cargo publish -p ratatoist-tui
```

## Tagging conventions

| Tag format | What it releases |
|------------|-----------------|
| `v0.1.0` | All crates at that version |
| `core-v0.2.0` | Only ratatoist-core (future, when versions diverge) |
| `tui-v0.2.0` | Only ratatoist-tui (future, when versions diverge) |

## Installing from a release

```sh
# From GitHub Releases (prebuilt binary)
curl -L https://github.com/cxrlos/ratatoist/releases/latest/download/ratatoist-aarch64-apple-darwin.tar.gz | tar xz
sudo mv ratatoist /usr/local/bin/

# From source
cargo install --path crates/ratatoist-tui

# From crates.io
cargo install ratatoist-tui
```

## Pre-release checklist

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --release
cargo test --workspace
```

# Releasing

This document describes how releases are created and published for ratatoist.

## Version scheme

Each crate is versioned independently following [Semantic Versioning](https://semver.org/):

| Bump | When | Example |
|------|------|---------|
| `patch` (0.1.**1**) | Bug fixes, docs, refactors | Fix comment rendering |
| `minor` (0.**2**.0) | New features, backward compatible | Add search, delete task |
| `major` (**1**.0.0) | Breaking changes | Config format change, API redesign |

## Tag format

Tags are per-crate:

| Tag | Meaning |
|-----|---------|
| `ratatoist-core-v0.1.0` | Core library release |
| `ratatoist-tui-v0.2.0` | TUI binary release |
| `ratatoist-nvim-v0.1.0` | Neovim plugin release (future) |

## Automated release flow (PR merge)

```
PR with label ──► merge ──► detect changed crates ──► bump + tag each ──► CI ──► build ──► release ──► publish
```

1. Open a PR to `main` and add a label: `major`, `minor`, or `patch`.
2. Merge the PR.
3. The workflow automatically:
   - Detects which crates were modified in the PR
   - Calculates the next version for each changed crate from its latest tag (defaults to `0.1.0` if no prior tags)
   - Updates each crate's `Cargo.toml` version
   - Commits and creates per-crate tags
   - Runs full CI validation (fmt, clippy, build, test)
   - Builds TUI binaries for macOS + Linux (only if TUI changed)
   - Creates a GitHub Release per changed crate
   - Publishes changed crates to crates.io

No label = no release. Use this for docs-only or CI-only PRs.

## Manual release

Go to **Actions > Release (manual) > Run workflow**:

- Select a crate from the dropdown: `ratatoist-core`, `ratatoist-tui`, or `ratatoist-nvim`
- Select bump type: `patch`, `minor`, or `major`
- The version is calculated automatically from the latest tag -- no manual version input
- Defaults to `0.1.0` if no prior release exists for that crate
- Binaries are only built for `ratatoist-tui` releases

## Rolling back a release

Go to **Actions > Rollback Release > Run workflow**:

- Enter the full tag to rollback (e.g. `ratatoist-tui-v0.2.0`) and a reason
- The workflow deletes the tag, GitHub Release, and restores the previous version in `Cargo.toml`

> **crates.io**: Published versions cannot be deleted, only yanked:
> ```sh
> cargo yank --version 0.2.0 ratatoist-core
> cargo yank --version 0.2.0 ratatoist-tui
> ```

## Manual release (fallback)

If automation fails:

```sh
# 1. Bump version in the crate's Cargo.toml
# 2. Update CHANGELOG.md
# 3. Commit, tag, push
git add -A
git commit -m "release: ratatoist-tui-v0.2.0"
git tag ratatoist-tui-v0.2.0
git push origin main --tags

# 4. Publish to crates.io
cargo publish -p ratatoist-tui
```

## Required GitHub secrets

| Secret | Purpose | How to get it |
|--------|---------|---------------|
| `CARGO_REGISTRY_TOKEN` | Publish to crates.io | [crates.io/settings/tokens](https://crates.io/settings/tokens) -- scopes: `publish-new`, `publish-update` |

## Pre-release checklist

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --release
cargo test --workspace
```

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

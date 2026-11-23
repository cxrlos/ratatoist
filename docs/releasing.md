# Releasing

How I ship ratatoist, and the one-time setup that only I can do. The crate is `ratatoist` and its binary is `ratatoist`. Versions follow SemVer and continue from the old binary crate, so the first release of `ratatoist` is `0.6.0`. While on `0.x`, a breaking change or a `feat` bumps the minor version (`features_always_increment_minor` in `release-plz.toml`) and a `fix` bumps the patch version. Each milestone lands on `main` as one squash-merged `feat`, so it ships as the next minor version.

## Pipeline

1. Every change lands on `main` as a squash-merged PR whose title is a Conventional Commit.
2. On each push to `main`, [release-plz](https://release-plz.dev) (`.github/workflows/release-plz.yml`, config in `release-plz.toml`) opens or updates a release PR: it bumps the version in `Cargo.toml` and `Cargo.lock` and writes `CHANGELOG.md` from the commits with git-cliff. `feat`, `fix`, `perf`, `revert`, `refactor` and `docs` commits get a changelog group; `build`, `chore`, `ci`, `style` and `test` commits are left out unless they are breaking. Only a `feat`, `fix`, `perf`, `refactor` or `revert` commit, or any breaking commit (`!` or a `BREAKING CHANGE:` footer), makes release-plz propose a new version (`release_commits`). A `docs`, `ci` or `chore` commit alone opens no release PR; it lands in the changelog of the next release.
3. Merging the release PR makes release-plz publish to crates.io through trusted publishing (OIDC, no stored token) and then create the `vX.Y.Z` tag through the GitHub API. It does not create the GitHub release (`git_release_enable = false`).
4. The `vX.Y.Z` tag triggers [dist](https://axodotdev.github.io/cargo-dist/book/) (`.github/workflows/release.yml`, config in `dist-workspace.toml`). It builds the four targets, creates the GitHub release with every asset attached, and then pushes the Homebrew formula `Formula/ratatoist.rb` to `cxrlos/homebrew-tap`.

Both release-plz jobs run with a GitHub App token, not `GITHUB_TOKEN`, because events made with `GITHUB_TOKEN` do not start workflows: without the App, CI would not run on the release PR and the tag would not trigger dist.

Targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. Linux and macOS only; Windows is not supported. dist builds `aarch64-apple-darwin` on `macos-15` (`[dist.github-custom-runners]`), because its default runner is retired.

Each release carries one `.tar.xz` archive per target, `ratatoist-<target>.tar.xz`, holding `ratatoist-<target>/` with the `ratatoist` binary, both licenses, `README.md` and `CHANGELOG.md`. Next to each archive is its `.sha256`, plus `source.tar.gz` and its `.sha256`, `sha256.sum`, the shell installer `ratatoist-installer.sh` and the Homebrew formula `ratatoist.rb`. Release binaries are built with `[profile.dist]` in `Cargo.toml` (the release profile with thin LTO). The install channels are:

```bash
cargo install --locked ratatoist
cargo binstall ratatoist
brew install cxrlos/tap/ratatoist
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/cxrlos/ratatoist/releases/latest/download/ratatoist-installer.sh \
  | sh
```

cargo-binstall finds the archives through `[package.metadata.binstall]` in `Cargo.toml` (`{ repo }/releases/download/v{ version }/{ name }-{ target }{ archive-suffix }` with `pkg-fmt = "txz"`, binary at `{ name }-{ target }/{ bin }`); dist itself has no binstall setting, so that table must keep matching dist's archive names. Nothing in CI compares them: when I change either, I read the archive names `dist plan` prints against that table, and after the release the binstall check under Verifying a release proves it.

## Immutable releases

Immutable releases are on for this repo: once a release is published, its tag and assets cannot change. So exactly one tool, dist, creates the release, and it publishes it only after every asset is attached.

- GitHub locks a release's tag and assets when the release is published, and recommends creating it as a draft, attaching every asset, then publishing ([Immutable releases](https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases)).
- dist creates the release in its `host` job with a single `gh release create <tag> ... artifacts/*` call, after all build jobs succeed (`github-release = "host"`, `create-release` left at its default `true`). Nothing in the workflow uploads to the release afterwards; the Homebrew job only pushes to the tap.
- With `git_release_enable = false`, release-plz creates the tag and no release.

What this means when something fails: if a build job fails, `host` never runs and no release exists, so a transient failure is fixed by rerunning the failed jobs for the same tag. By then the version is already on crates.io, so a failure that needs a code change, or a published release that is wrong, is fixed by the next patch version (see Rolling back).

## Changing the pipeline

`release.yml` is generated by dist 0.34.0. I never edit it by hand: the `plan` job, which also runs on every pull request, fails when the file differs from what `dist-workspace.toml` produces, and the tag-time `plan` job would then leave a version on crates.io with no GitHub release. To change it, I edit `dist-workspace.toml` and regenerate:

```bash
dist generate
dist plan
```

The actions in `release.yml` are pinned to commits, with a version comment, through `github-action-commits` in `dist-workspace.toml`; `actions/checkout` is the same commit `ci.yml` uses. Dependabot skips `release.yml` (`exclude-paths` in `.github/dependabot.yml`), and the `plan` check is required on `main`, so a hand edit cannot merge. When Dependabot bumps `actions/checkout` in the other workflows, I copy the new commit into `github-action-commits` and run `dist generate` in that PR, and I refresh `actions/upload-artifact` and `actions/download-artifact` the same way when I upgrade dist. To upgrade dist itself, I install the new version and run `dist init`, which rewrites `cargo-dist-version` and `release.yml`.

## One-time setup (manual)

Done: squash merge only with the PR title as the commit message, delete branches on merge, immutable releases, the `main` ruleset with the CI checks.

| # | What | Why |
|---|---|---|
| 1 | Install the release-plz GitHub App (repository permissions Contents: read and write, Pull requests: read and write) on `cxrlos/ratatoist`, and store its App ID (or Client ID) as the secret `RELEASE_PLZ_APP_ID` and a private key as `RELEASE_PLZ_APP_PRIVATE_KEY` | With the default `GITHUB_TOKEN`, CI does not run on the release PR and the tag does not trigger dist |
| 2 | Store a fine-grained PAT with Contents: read and write on `cxrlos/homebrew-tap` only as `HOMEBREW_TAP_TOKEN`, and prove it is non-empty and valid (below) | dist pushes the formula there; an empty secret only fails at the end of the first tagged run, after the release exists |
| 3 | Delete the old `CARGO_REGISTRY_TOKEN` secret | Publishing uses trusted publishing; no registry token is stored |
| 4 | Add the check `plan` (the `plan` job of `release.yml`) to the required checks of the `main` ruleset, next to `fmt`, `clippy`, `test (ubuntu-latest)`, `test (macos-latest)`, `msrv (1.88)`, `cargo deny`, `package`, `gitleaks` and `PR title` | A `release.yml` that dist did not generate cannot merge |
| 5 | Publish `0.6.0` to crates.io by hand (see below) | crates.io cannot trusted-publish a crate that does not exist yet |
| 6 | On crates.io, add a trusted publisher for `ratatoist`: repository owner `cxrlos`, repository `ratatoist`, workflow `release-plz.yml`, no environment | Lets later releases publish with OIDC instead of a token |
| 7 | Revoke the API token used in step 5 | It is no longer needed |

To store the tap token (step 2) and prove it is non-empty before the first tag:

```bash
read -rs TAP_TOKEN
test -n "$TAP_TOKEN" && echo non-empty
GH_TOKEN="$TAP_TOKEN" gh api repos/cxrlos/homebrew-tap --jq .full_name
printf '%s' "$TAP_TOKEN" | gh secret set HOMEBREW_TAP_TOKEN -R cxrlos/ratatoist
unset TAP_TOKEN
gh secret list -R cxrlos/ratatoist
```

The second command prints `non-empty`, the third prints `cxrlos/homebrew-tap`, and the list shows `HOMEBREW_TAP_TOKEN` with today's date.

To add the `plan` check (step 4), once a PR has run `release.yml`:

```bash
gh api repos/cxrlos/ratatoist/rulesets/27865560 \
  --jq '{rules: [.rules[] | if .type == "required_status_checks" then .parameters.required_status_checks += [{context: "plan"}] else . end]}' \
  | gh api --method PUT repos/cxrlos/ratatoist/rulesets/27865560 --input -
gh api repos/cxrlos/ratatoist/rulesets/27865560 \
  --jq '.rules[] | select(.type == "required_status_checks") | .parameters.required_status_checks[].context'
```

## First release (0.6.0)

`ratatoist` is not on crates.io yet, so the first publish is mine. Setup steps 1 to 4 come first.

1. Merge the umbrella PR (`feat/v0.6.0` into `main`). release-plz sees an unpublished crate and opens a release PR for `0.6.0`; I leave it alone for now.
2. From a clean, up-to-date `main`, publish by hand with a crates.io token scoped to publishing new crates:

   ```bash
   git switch main
   git pull --ff-only
   git status --short
   cargo publish --dry-run --locked
   cargo publish --locked
   ```

   `git status --short` must print nothing, so the published crate records the merge commit.
3. Do setup steps 6 and 7: add the trusted publisher, then revoke the token.
4. Tag the published commit by hand, so dist runs and creates the release:

   ```bash
   git tag -s v0.6.0 -m v0.6.0
   git push origin v0.6.0
   ```

5. Close the `0.6.0` release PR from step 1 without merging it: that version is published, and its changelog would cover the whole history. On the next `feat`, `fix`, `perf`, `refactor`, `revert` or breaking commit, release-plz opens a new release PR; I check it proposes `0.7.0` for a `feat` or `0.6.1` for a fix, never `0.6.0`, before merging it. From then on, merging the release PR publishes, tags and releases with no manual step.

`0.6.0` has no section of its own in `CHANGELOG.md`: release-plz writes sections from `0.7.0` on.

## Final releases of the old crates

`ratatoist-core` and `ratatoist-tui` each get one last release that points to `ratatoist`, after `ratatoist` 0.6.0 is on crates.io. They are built from a branch off `a14534c` (the commit that shipped `ratatoist-tui` 0.5.0), which is pushed for the record and never merged, tagged or released on GitHub.

1. Branch: `git switch -c chore/final-old-crates a14534c`.
2. `ratatoist-core` 0.4.1: set `version = "0.4.1"` in `crates/ratatoist-core/Cargo.toml` and the `ratatoist-core` version in the root `[workspace.dependencies]` to `0.4.1`; set its `description` to `Moved to the ratatoist crate: cargo install ratatoist`; replace `crates/ratatoist-core/README.md` with a short note that the app moved to `ratatoist`, with `cargo install ratatoist`.
3. `ratatoist-tui` 0.5.1: set `version = "0.5.1"` and the same `description` in `crates/ratatoist-tui/Cargo.toml`; replace `crates/ratatoist-tui/README.md` with the same note; make the binary print `ratatoist-tui has moved to ratatoist: cargo install ratatoist` on stderr as the first thing `main` does in `crates/ratatoist-tui/src/main.rs`.
4. Refresh only the workspace's own entries in the lockfile and check it builds: `cargo update --workspace`, then `cargo build --locked -p ratatoist-tui` and `cargo run -p ratatoist-tui -- --help`, which prints the moved line first.
5. Commit (signed) and push the branch: `git push -u origin chore/final-old-crates`.
6. Publish core before tui, because tui resolves core from crates.io:

   ```bash
   cargo publish --locked -p ratatoist-core
   cargo publish --locked -p ratatoist-tui
   ```

No yanks: earlier versions stay installable. The names `ratatoist-core` and `ratatoist-tui` receive nothing after this.

## Verifying a release

After each release, for version `X.Y.Z`:

```bash
git fetch --tags origin
git rev-parse 'vX.Y.Z^{commit}'
git log -1 --format=%s 'vX.Y.Z^{commit}'
curl -s -A ratatoist-release-check https://crates.io/api/v1/crates/ratatoist \
  | jq -r '.crate.max_version'
gh release view vX.Y.Z -R cxrlos/ratatoist --json assets --jq '.assets[].name'
gh release verify vX.Y.Z -R cxrlos/ratatoist
mkdir -p ~/scratch/ratatoist-vX.Y.Z
gh release download vX.Y.Z -R cxrlos/ratatoist -D ~/scratch/ratatoist-vX.Y.Z
cd ~/scratch/ratatoist-vX.Y.Z
sha256sum -c sha256.sum
for f in *.tar.xz; do sha256sum -c "$f.sha256"; done
tar -tJf ratatoist-x86_64-unknown-linux-gnu.tar.xz
```

I check that the tag points to the release commit on `main`, crates.io shows `X.Y.Z`, the release has the four archives with their `.sha256`, `source.tar.gz` with its `.sha256`, `sha256.sum`, `ratatoist-installer.sh` and `ratatoist.rb`, every checksum says `OK`, and each archive holds `ratatoist-<target>/ratatoist`, both licenses, `README.md` and `CHANGELOG.md`. Then the installers:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/cxrlos/ratatoist/releases/download/vX.Y.Z/ratatoist-installer.sh \
  | sh
~/.cargo/bin/ratatoist --version
brew install cxrlos/tap/ratatoist
"$(brew --prefix)/bin/ratatoist" --version
cargo binstall --dry-run --no-confirm \
  --disable-strategies quick-install,compile \
  ratatoist@X.Y.Z
gh api repos/cxrlos/homebrew-tap/contents/Formula/ratatoist.rb --jq .content \
  | base64 -d | grep -E 'url|sha256'
cat ~/scratch/ratatoist-vX.Y.Z/*.tar.xz.sha256
```

Both print `ratatoist X.Y.Z`. The binstall dry run, allowed only to download from the release, resolves `ratatoist-<target>.tar.xz` from `vX.Y.Z` and stops before installing; if `[package.metadata.binstall]` no longer matches the archive names, it fails instead of falling back to a source build. Every `sha256` in the formula equals the `.sha256` of the archive its `url` names.

## Rolling back

A broken release is never deleted: its tag, GitHub release and crate stay. I yank it with `cargo yank --version X.Y.Z ratatoist` and ship a fix as the next patch version.

A version number is never reused, yanked or not: crates.io versions are permanent. `ratatoist-tui` 0.4.0 is burned that way (published, then yanked).

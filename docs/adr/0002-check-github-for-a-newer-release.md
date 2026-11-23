# Check GitHub once a day for a newer release

ratatoist asks GitHub's `releases/latest` for `cxrlos/ratatoist` at most once a day, in the background after the first frame, and shows a muted release notice in the tab row when a newer stable version is out. It is the only request ratatoist makes to anything other than Todoist. It is on by default because an opt-in check reaches almost nobody, and the point is fewer bug reports against old versions. The request carries no token and no account data, only ratatoist's version in the User-Agent, and `[updates] check = false` or `RATATOIST_NO_UPDATE_CHECK` turns it off.

## Considered options

- **Opt-in**: rejected, for the reach reason above.
- **crates.io as the source**: rejected. A version is on crates.io before its binaries, installer and Homebrew formula exist, while a GitHub release is published only once every asset is attached.

## Consequences

Installs that the package manager keeps up to date and that may lag the release (AUR, Nix) show no notice. Homebrew installs show `brew upgrade ratatoist`. `cargo install`, `cargo binstall` and the shell installer all land in the cargo bin directory with no receipt, so their notice points at the README's install section rather than one command. `--demo`, tests and the `auth` commands never check. The answer is cached in `release.json` in the cache folder, with the release I chose to skip, so a restart within the day asks nothing, and a failed check (offline, rate limited, a bad answer) is only logged and waits a day like a good one.

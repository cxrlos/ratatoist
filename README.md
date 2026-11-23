<div align="center"><pre>
░█▀▄░█▀█░▀█▀░█▀█░▀█▀░█▀█░▀█▀░█▀▀░▀█▀
░█▀▄░█▀█░░█░░█▀█░░█░░█░█░░█░░▀▀█░░█░
░▀░▀░▀░▀░░▀░░▀░▀░░▀░░▀▀▀░▀▀▀░▀▀▀░░▀░
</pre></div>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Todoist-E44332?style=for-the-badge&logo=todoist&logoColor=white" alt="Todoist">
  <img src="https://img.shields.io/badge/Terminal-4D4D4D?style=for-the-badge&logo=gnometerminal&logoColor=white" alt="Terminal">
</p>

<p align="center">
  <em>Your Todoist inbox, without leaving the terminal.</em>
</p>

<p align="center">
  <a href="https://crates.io/crates/ratatoist">
    <img src="https://img.shields.io/crates/v/ratatoist?style=for-the-badge&color=9ccfd8" alt="ratatoist on crates.io">
  </a>
  <img src="https://img.shields.io/badge/MSRV-1.88-orange?style=for-the-badge&logo=rust&logoColor=white" alt="MSRV 1.88">
  <a href="https://github.com/cxrlos/ratatoist/stargazers">
    <img src="https://img.shields.io/github/stars/cxrlos/ratatoist?style=for-the-badge&color=eb6f92" alt="GitHub stars">
  </a>
</p>

---

<!-- video later -->

- **Fast**: incremental delta sync, real-time WebSocket updates, exponential-backoff retries
- **Vim-native**: `j`/`k`/`h`/`l`, folds, modal editing; standard arrow-key mode also available
- **Polished**: task comments, priority indicators, human-readable dates, 10 built-in themes
- **Secure**: the token lives in the OS keyring (or `TODOIST_API_TOKEN`), never in the log or on screen; a pre-0.6.0 `api_token` in `config.toml` moves to the keyring on first start

## Quick start

```sh
cargo install ratatoist
```

To look around first, `ratatoist --demo` opens the app on a fictional account. It needs no token and never touches the network, and anything you change there lasts until you quit. A due date typed there can be `today`, `tomorrow`, a weekday or `YYYY-MM-DD`; it reads nothing fancier.

Log in once. ratatoist asks for your API token (from [Todoist settings](https://app.todoist.com/app/settings/integrations/developer)) without echoing it, checks it with Todoist and keeps it in your OS keyring:

```sh
ratatoist auth login
ratatoist
```

To read the token from a password manager instead, pipe it in: `pass show todoist | ratatoist auth login --stdin`. `TODOIST_API_TOKEN`, when set, wins over the keyring. [docs/auth.md](docs/auth.md) has the details.

## Features

<details>
<summary><strong>Navigation and views</strong></summary>

- Dual input modes: Vim (Normal/Visual/Insert) and Standard (arrows/Enter)
- Project tree with workspaces, folders, and favorites pinned to top
- Folder expand/collapse (`Space` in Projects pane)
- Task hierarchy with foldable subtask trees (`Space`, `za`/`zR`/`zM`)
- Task detail pane with scrollable content, comments, and metadata
- StatsDock: overdue / today / week / P1–P4 counts; click to filter tasks (`f`)
- Active / Done / Both task filter cycling (`f`)
- Sort cycling: default / priority / due date / created (`o`)
- Splash screen with ASCII art and terminal-adaptive progress bar

</details>

<details>
<summary><strong>Task operations</strong></summary>

- Complete / uncomplete (`x`) with optimistic UI: instant feedback, reverts on error
- Quick-add (`a`) with multi-field form: content, priority, due date, project
- Inline field editing in detail pane (`i` / `Enter`)
- Priority picker popup with visual selector
- Star / unstar projects (`s`)
- View completed tasks per project (Done / Both filter fetches from API)

</details>

<details>
<summary><strong>Comments and collaboration</strong></summary>

- Multi-user comment threads with per-user colors
- Consecutive same-user message collapsing
- Attachment display with file type metadata
- Add comments from the detail pane (`c`)
- Collaborator name resolution from API

</details>

<details>
<summary><strong>Theming</strong></summary>

- 10 built-in themes: Rose Pine, Gruvbox Dark, Dracula, Nord, One Dark, Solarized Dark, Catppuccin Mocha, Tokyo Night, Monokai, Material Dark
- Theme picker in Settings (`,` → theme)
- Custom themes: drop any Base16 JSON file into `~/.config/ratatoist/themes/`
- Theme and idle timeout preferences persisted across sessions

</details>

<details>
<summary><strong>Developer experience</strong></summary>

- Logs live in `$XDG_STATE_HOME/ratatoist/` (by default `~/.local/state/ratatoist/`, on macOS too), one file a day (named by the UTC date) for a week; set the level with `RATATOIST_LOG`, for example `RATATOIST_LOG=debug`
- Error popups with context and suggestions
- Dimmed background overlay on popups
- Keybinding cheatsheet (`?`)
- `ratatoist auth login | status | logout` for the token in the OS keyring

</details>

## Requirements

- Rust 1.88+ (edition 2024)
- A [Todoist](https://todoist.com) account with API token
- A true-color terminal (Alacritty, iTerm2, WezTerm, Kitty, etc.)

## Documentation

|                                                             |                                                    |
| ----------------------------------------------------------- | -------------------------------------------------- |
| [Changelog](CHANGELOG.md)                                   | Version history                                    |
| [Logging in](docs/auth.md)                                  | The token, the keyring and `ratatoist auth`        |

## Development

[CONTRIBUTING.md](CONTRIBUTING.md) covers setup, and the Commands section of [AGENTS.md](AGENTS.md) lists every check CI runs.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in ratatoist by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

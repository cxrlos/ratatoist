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
  <a href="https://crates.io/crates/ratatoist-core">
    <img src="https://img.shields.io/crates/v/ratatoist-core?style=for-the-badge&label=core&color=9ccfd8" alt="core on crates.io">
  </a>
  <a href="https://crates.io/crates/ratatoist-tui">
    <img src="https://img.shields.io/crates/v/ratatoist-tui?style=for-the-badge&label=tui&color=9ccfd8" alt="tui on crates.io">
  </a>
  <a href="https://crates.io/crates/ratatoist-tui">
    <img src="https://img.shields.io/crates/d/ratatoist-tui?style=for-the-badge&label=downloads&color=c4a7e7" alt="crates.io downloads">
  </a>
  <img src="https://img.shields.io/badge/MSRV-1.85-orange?style=for-the-badge&logo=rust&logoColor=white" alt="MSRV 1.85">
  <a href="https://github.com/cxrlos/ratatoist/stargazers">
    <img src="https://img.shields.io/github/stars/cxrlos/ratatoist?style=for-the-badge&color=eb6f92" alt="GitHub stars">
  </a>
</p>

---

<!-- video later -->

- **Fast** — incremental delta sync, real-time WebSocket updates, exponential-backoff retries
- **Vim-native** — `j`/`k`/`h`/`l`, folds, modal editing; standard arrow-key mode also available
- **Polished** — threaded comments, priority indicators, human-readable dates, 10 built-in themes
- **Secure** — token never logged, config file permissions validated

## Packages

| Crate                                      | Description                                  |
| ------------------------------------------ | -------------------------------------------- |
| [`ratatoist-core`](crates/ratatoist-core/) | Todoist Sync API client, config, logging     |
| [`ratatoist-tui`](crates/ratatoist-tui/)   | Terminal UI binary (installs as `ratatoist`) |
| [`ratatoist-nvim`](crates/ratatoist-nvim/) | Neovim plugin — coming soon                  |

## Quick start

```sh
git clone https://github.com/cxrlos/ratatoist.git
cd ratatoist
cargo install --path crates/ratatoist-tui
```

Run the guided setup (validates your token and optionally writes a shell alias):

```sh
ratatoist --new-user
```

Or configure manually (token from [Todoist settings](https://app.todoist.com/app/settings/integrations)):

```sh
mkdir -p ~/.config/ratatoist
echo 'api_token = "your-token"' > ~/.config/ratatoist/config.toml
chmod 600 ~/.config/ratatoist/config.toml
ratatoist
```

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

- Complete / uncomplete (`x`) with optimistic UI — instant feedback, reverts on error
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

- Structured JSON logging (`--debug`)
- Error popups with context and suggestions
- Dimmed background overlay on popups
- Keybinding cheatsheet (`?`)
- `--new-user` onboarding: token entry + shell alias setup
- `--idle-forcer` flag for testing idle timeout (adds 5 s option)

</details>

## Requirements

- Rust 1.85+ (edition 2024)
- A [Todoist](https://todoist.com) account with API token
- A true-color terminal (Alacritty, iTerm2, WezTerm, Kitty, etc.)

## Documentation

|                                                             |                                                    |
| ----------------------------------------------------------- | -------------------------------------------------- |
| [TUI keybindings and usage](crates/ratatoist-tui/README.md) | Full key reference for both Vim and Standard modes |
| [Core library API](crates/ratatoist-core/README.md)         | Using the Todoist client in your own Rust projects |
| [Changelog](CHANGELOG.md)                                   | Version history                                    |

## Development

```sh
cargo run -p ratatoist-tui              # run TUI
cargo run -p ratatoist-tui -- --debug   # with debug logs
cargo build --workspace --release       # build all
./scripts/format.sh                     # format code
cargo clippy --workspace                # lint
cargo test --workspace                  # test
```

## Roadmap

- [ ] Delete task with confirmation
- [ ] Global search (`/`) with ranked results
- [ ] Move task between projects
- [ ] Undo / redo stack
- [ ] Calendar and Board views
- [ ] Neovim plugin

## License

MIT

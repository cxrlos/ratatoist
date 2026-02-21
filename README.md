<p align="center">
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Todoist-E44332?style=for-the-badge&logo=todoist&logoColor=white" alt="Todoist">
  <img src="https://img.shields.io/badge/Terminal-4D4D4D?style=for-the-badge&logo=gnometerminal&logoColor=white" alt="Terminal">
</p>

<h1 align="center">ratatoist</h1>

<p align="center">
  <em>A keyboard-driven terminal UI for <a href="https://todoist.com">Todoist</a>, built with Rust and <a href="https://ratatui.rs">ratatui</a>.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/crates/v/ratatoist-tui?style=flat-square&color=9ccfd8&label=tui" alt="crates.io tui">
  <img src="https://img.shields.io/crates/v/ratatoist-core?style=flat-square&color=c4a7e7&label=core" alt="crates.io core">
  <img src="https://img.shields.io/github/license/cxrlos/ratatoist?style=flat-square&color=ebbcba" alt="license">
  <img src="https://img.shields.io/github/actions/workflow/status/cxrlos/ratatoist/ci.yml?style=flat-square&label=CI" alt="CI">
  <img src="https://img.shields.io/badge/edition-2024-eb6f92?style=flat-square" alt="Rust Edition 2024">
</p>

---

```
╭── Projects ──────╮╭── Work ──────────────────────────────╮
│   Inbox           ││ ◇ ● API migration           Fri     │
│ ★ Work            ││   ▾ ● Auth middleware       tmrw    │
│   Personal        ││     ◦   Database schema     today   │
│   Side project    ││   ▸ ● Data pipeline         Mon     │
│                   ││ ◇   Update documentation            │
╰──────────────────╯╰──────────────────────────────────────╯
 NORMAL  Work ▸ 5 tasks
 j/k navigate  Enter open  x complete  a add  o sort  q quit
```

## Packages

| Crate | Description | Version |
|-------|-------------|---------|
| [`ratatoist-core`](crates/ratatoist-core/) | Todoist API v1 client, config, logging | ![core](https://img.shields.io/crates/v/ratatoist-core?style=flat-square&color=c4a7e7&label=) |
| [`ratatoist-tui`](crates/ratatoist-tui/) | Terminal UI binary (installs as `ratatoist`) | ![tui](https://img.shields.io/crates/v/ratatoist-tui?style=flat-square&color=9ccfd8&label=) |
| [`ratatoist-nvim`](crates/ratatoist-nvim/) | Neovim plugin | 🔜 Coming soon |

## Quick start

```sh
git clone https://github.com/cxrlos/ratatoist.git
cd ratatoist
cargo install --path crates/ratatoist-tui
```

Set your API token (get it from [Todoist settings](https://app.todoist.com/app/settings/integrations)):

```sh
mkdir -p ~/.config/ratatoist
echo 'api_token = "your-token"' > ~/.config/ratatoist/config.toml
chmod 600 ~/.config/ratatoist/config.toml
```

Run:

```sh
ratatoist
```

## Why ratatoist?

Todoist is great. But switching to a browser tab to check a task breaks flow. ratatoist brings your tasks into the terminal where you already live -- with vim motions, instant navigation, and zero mouse required.

- **Fast**: Full project sync at startup, cached navigation, async background refresh.
- **Vim-native**: `j`/`k`/`h`/`l`, folds with `za`/`zR`/`zM`, modal editing with `i`/`Esc`. Or use Standard mode with arrows.
- **Polished**: Threaded comments, priority-colored indicators, human-readable dates, attachment display.
- **Secure**: Token never logged, config permissions validated, structured error handling.

## Features

<details>
<summary><strong>Navigation and views</strong></summary>

- Dual input modes: Vim (Normal/Visual/Insert) and Standard (arrows/Enter)
- Project list with favorites pinned to top, auto-loading tasks
- Task hierarchy with foldable subtask trees (`Space`, `za`/`zR`/`zM`)
- Task detail pane with scrollable content, comments, and metadata
- Overview dashboard: overdue/today/week counts with weekly progress bar
- Sort cycling: default / priority / due date / created (`o`)
- Splash screen with ASCII art and terminal-adaptive progress bar

</details>

<details>
<summary><strong>Task operations</strong></summary>

- Complete / uncomplete (`x`)
- Quick-add (`a`) with multi-field form: content, priority, due date, project
- Inline field editing in detail pane (`i` / `Enter`)
- Priority picker popup with visual selector
- Star / unstar projects (`s`)
- Content parsing: `p1`-`p4`, natural language dates, structured dates (`YYYY-MM-DD`, `DD/MM/YYYY`, `DD-MM-YYYY`)

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
<summary><strong>Developer experience</strong></summary>

- Structured JSON logging (`--debug`)
- Error popups with context and suggestions
- Dimmed background overlay on popups
- Keybinding cheatsheet (`?`)
- Settings pane for mode toggle

</details>

## Requirements

- Rust 1.85+ (edition 2024)
- A [Todoist](https://todoist.com) account with API token
- A true-color terminal (Alacritty, iTerm2, WezTerm, Kitty, etc.)

## Documentation

| | |
|---|---|
| [TUI keybindings and usage](crates/ratatoist-tui/README.md) | Full key reference for both Vim and Standard modes |
| [Core library API](crates/ratatoist-core/README.md) | Using the Todoist client in your own Rust projects |
| [Changelog](CHANGELOG.md) | Version history |

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

- [ ] Global search (`/`) with ranked results
- [ ] Delete task with confirmation
- [ ] Move task between projects
- [ ] Undo / redo stack
- [ ] Config-driven theming (custom color palettes)
- [ ] Calendar and Board views
- [ ] Background auto-refresh
- [ ] Neovim plugin

## License

MIT

---

<p align="center">
  <sub>Built with <a href="https://ratatui.rs">ratatui</a> · Powered by <a href="https://developer.todoist.com/api/v1">Todoist API v1</a></sub>
</p>

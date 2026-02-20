# ratatoist

A terminal user interface for [Todoist](https://todoist.com), built with Rust and [ratatui](https://ratatui.rs). Keyboard-driven, vim-native, themed with [Rose Pine](https://rosepinetheme.com).

## Features

- Browse projects and tasks in a split-pane layout
- Vim-style navigation (`j`/`k`, `h`/`l`, `Enter`, `Esc`)
- Rose Pine color palette with priority-coded task indicators
- Async API communication with Todoist REST API v2

## Requirements

- Rust 1.85+ (edition 2024)
- A [Todoist](https://todoist.com) account
- A Todoist API token ([get yours here](https://app.todoist.com/app/settings/integrations))
- A true-color terminal (Alacritty, iTerm2, WezTerm, Kitty, etc.)

## Installation

```sh
cargo install --path .
```

Or build from source:

```sh
git clone https://github.com/cxrlos/ratatoist.git
cd ratatoist
cargo build --release
```

The binary will be at `target/release/ratatoist`.

## Configuration

### API token

ratatoist needs your Todoist API token to authenticate. Two options, checked in this order:

**Option 1 -- Environment variable (recommended for ephemeral use)**

```sh
export TODOIST_API_TOKEN="your-token-here"
ratatoist
```

**Option 2 -- Config file (recommended for daily use)**

Create `~/.config/ratatoist/config.toml`:

```toml
api_token = "your-token-here"
```

If the config file exists, ratatoist validates that its permissions are restricted to owner-only (`600`) before reading. Your token never leaves your machine -- it is only sent over HTTPS to Todoist's API.

### Security notes

- Your API token is **never** logged, printed to stdout, or included in debug output.
- The config file must be owner-read/write only (`chmod 600`). ratatoist refuses to start if permissions are too open.
- The token is stored in memory only for the duration of the process.
- Never commit your token. The `.gitignore` excludes `.env` and config files.
- If you suspect your token is compromised, regenerate it immediately in [Todoist settings](https://app.todoist.com/app/settings/integrations).

## Usage

```sh
ratatoist
```

### Key bindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection down / up |
| `h` / `l` | Switch pane left / right |
| `Tab` / `Shift-Tab` | Switch pane forward / back |
| `Enter` | Select project / expand task |
| `q` | Quit |
| `Ctrl-c` | Force quit |

### Layout

```
+-- Projects -------+-- Tasks (Project Name) --------+
|   Inbox            |  ● Buy groceries     today     |
| > Work             |  ● Review PR         tomorrow  |
|   Personal         |    Write docs                   |
|   Side Projects    |  ● Deploy v2         overdue   |
+--------------------+--------------------------------+
  j/k navigate  Tab switch pane  Enter select  q quit
```

## Development

```sh
cargo run                # debug build
cargo build --release    # optimized build
cargo clippy             # lint
cargo test               # run tests
```

## License

MIT

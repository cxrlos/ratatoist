# Ratatoist

A terminal UI client for Todoist. Lets users manage their tasks without leaving the terminal.

## Packages

- **ratatoist-core** — Todoist Sync API v1 client, config management, structured logging. Published independently on crates.io; usable without the TUI.
- **ratatoist-tui** — Terminal UI binary, installs as `ratatoist`.
- **ratatoist-nvim** — Neovim plugin (planned).

## Navigation

- Sidebar shows workspaces, folders, and projects hierarchically; favorites pinned at top.
- Dual input modes: Vim (`j`/`k`/`h`/`l`, Normal/Insert/Visual) and Standard (arrow keys).
- Task list with foldable subtask trees.
- Detail pane with scrollable content, comments, and inline field editing.
- StatsDock showing overdue / today / week / P1–P4 counts; click to filter.

## Task Management

- Complete / uncomplete (`x`) with optimistic UI — instant feedback, reverts on server error.
- Quick-add (`a`) with content, priority, due date, and project fields.
- Inline field editing from the detail pane.
- Priority picker popup.
- Add comments from the detail pane (`c`).
- Star / unstar projects (`s`).
- Filter by Active / Done / Both (`f`); sort by default / priority / due / created (`o`).

## Sync

- Delta sync using Todoist Sync API v1; sync token persisted across sessions.
- Real-time updates via WebSocket; reconnects automatically with exponential backoff.
- Rate-limit retry: up to 3 attempts with exponential backoff.
- Idle mode: buffers incoming events when no user input for the configured timeout.

## Security

- API token never logged; config file saved at 0o600 (Unix); token masked in debug output.
- Supports `TODOIST_API_TOKEN` env var as an alternative to the config file.

## Theming

- 10 built-in themes; custom Base16 JSON themes loaded from `~/.config/ratatoist/themes/`.
- Theme and idle timeout preference persisted across sessions.

## Onboarding

- `--new-user` flag: guided token entry with API validation, optional shell alias setup.
- Onboarding shown automatically on first launch when no config is found.

## Developer Flags

- `--debug` — structured JSON logging to `~/.config/ratatoist/logs/`.
- `--idle-forcer` — adds a 5s idle timeout option for testing.

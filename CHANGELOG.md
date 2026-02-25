# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## ratatoist-core 0.3.0 / ratatoist-tui 0.3.0 -- 2026-02-24

### Added

- Sync API transport — all reads and writes now go through `POST /api/v1/sync`; incremental delta sync with sync token persisted to `~/.config/ratatoist/sync_state.json`; exponential-backoff retry with jitter on 429
- Completed tasks — `f` cycles Active → Done → Both; Done/Both fetches from `GET /api/v1/tasks/completed` and caches per project
- StatsDock — interactive stats pane (overdue / today / week / P1–P4) with `Projects → Tasks → StatsDock` pane cycle; `h`/`l` scroll items, `Enter` applies filter, `Esc` clears
- Dynamic theming — 10 built-in Base16 themes (Rose Pine, Gruvbox Dark, Dracula, Nord, One Dark, Solarized Dark, Catppuccin Mocha, Tokyo Night, Monokai, Material Dark); custom themes via `~/.config/ratatoist/themes/*.json`; theme persisted across sessions
- Workspace and folder navigation — Projects pane renders full org tree with workspace headers, folders, and folder expand/collapse (`Space`)
- New-user onboarding — `--new-user` flag: guided token entry with live validation, optional shell alias setup written to rc file
- Optimistic mutations — complete, add, update, and comment operations apply locally immediately with revert snapshots on server error
- Settings pane expanded — mode toggle, theme picker, and idle timeout cycling (60 s → 30 min)
- WebSocket infrastructure — URL fetched from user endpoint; background task with exponential-backoff reconnect
- `SyncCommand` / `SyncResponse` types in `ratatoist-core::api::sync` for type-safe Sync API framing
- `Workspace` and `Folder` models; `CompletedRecord` and `CompletedTasksResponse` for completed tasks endpoint
- `--idle-forcer` debug flag adds 5 s idle timeout option for testing

### Changed

- `ratatoist-core` client stripped down to `sync()`, `get_user()`, `get_comments()`, and `get_completed_tasks()` — all CRUD previously done via REST now goes through Sync API commands
- Theme system rewritten from static Rose Pine constants to dynamic `Theme` struct loaded from Base16 JSON; all render functions receive `&Theme` at call time
- `App::new()` now accepts `idle_forcer` and `ephemeral` flags
- Task filter cycling (`f`) replaces the old single-state active-only view

### Fixed

- Task list scroll position clamped after completing a task when selected index would exceed new visible count

### References

- PR #8: StatsDock filtering, visual modifications
- PR #9: Sync API transport rewrite, WebSocket, real-time refresh

## ratatoist-core 0.1.0 / ratatoist-tui 0.1.0 -- 2026-02-21

### Added

- Cargo workspace with three crates: `ratatoist-core`, `ratatoist-tui`, `ratatoist-nvim` (placeholder)
- Async Todoist API v1 client with pagination and structured logging
- Config module: API token from env var or `~/.config/ratatoist/config.toml` with permission validation
- Dual input modes: Vim (Normal/Visual/Insert) and Standard (arrows/Enter)
- Project list with favorites pinned to top, auto-loading tasks on navigation
- Task hierarchy with foldable subtask trees (Space, za/zR/zM)
- Task detail pane with inline field editing and priority picker popup
- Multi-user comment threads with per-user colors, consecutive message collapsing, and attachment display
- Task operations: complete/uncomplete (x), quick-add (a) with multi-field form, star projects (s)
- Content parsing: extracts p1-p4 priority, natural language dates, structured dates (YYYY-MM-DD, DD/MM/YYYY, DD-MM-YYYY) with validation
- Overview dashboard with overdue/today/week counts and weekly progress bar
- Sort cycling: default/priority/due/created (o)
- Splash screen with ASCII art logo and terminal-adaptive progress bar
- Structured error system with context, suggestions, and dimmed popup background
- In-memory task cache with async background refresh via tokio channels
- Structured JSON logging to file with --debug flag
- Keybinding cheatsheet popup (?)
- Settings pane for mode toggle
- GitHub Actions CI (format, clippy, build, test) and release workflow

### Fixed

- Switched `reqwest` from `native-tls` to `rustls-tls` to remove OpenSSL system dependency (fixes Linux CI builds) -- PR #4
- Independent per-crate versioning with per-crate tags (`ratatoist-core-v0.1.0`, `ratatoist-tui-v0.1.0`) -- PR #3
- Unified release workflow: version bump, CI validation, build, GitHub Release, and crates.io publish in a single pipeline -- PR #2, #3
- Removed test examples and `.ai/` references from tracked files -- PR #2

### References

- PR #1: Initial scaffold, workspace restructure, release infrastructure
- PR #2: Workflow fixes, independent versioning
- PR #3: Switch to rustls-tls, retrigger v0.1.0 release

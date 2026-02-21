# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

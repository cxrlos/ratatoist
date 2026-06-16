# Ratatoist

TUI for Todoist in Rust (ratatui + tokio + reqwest). Keyboard-driven, vim-native, Rose Pine.
Designed to feel as fluid as navigating code in Neovim — lazygit-quality panes, gum-clean chrome.

## Workspace

```
crates/
  ratatoist-core/   Todoist Sync API client, config, logging (the reusable library)
    src/api/        client.rs (sync + REST, reqwest/tokio), models.rs, sync.rs (SyncCommand/Response)
    src/            config.rs (token + 0600 perms), sync_state.rs (sync_token persistence), logging.rs
  ratatoist-tui/    the `ratatoist` binary
    src/            app.rs (App state, event loop, background mpsc channel, optimistic ops, websocket),
                    keys.rs (Vim Normal/Visual/Insert + Standard dispatch), main.rs (clap, onboarding)
    src/ui/         layout, statusbar, theme (Rose Pine + user themes), views/{projects,tasks,detail,
                    settings,overview}, components/{error_popup,input_popup,cheatsheet,priority_picker,…}
  ratatoist-nvim/   stub — not started; deferred until core exposes a UI-agnostic Store
```

## Transport

All reads and writes go through `POST /api/v1/sync` with a `sync_token` (`*` = full, else incremental
delta), persisted atomically to `~/.config/ratatoist/sync_state.json`. Completed tasks come from
`GET /api/v1/tasks/completed`, comments from `GET /api/v1/comments` (both cursor-paginated). Token from
`TODOIST_API_TOKEN` or `~/.config/ratatoist/config.toml`; never logged (the `Config` Debug impl redacts).

## Optimistic UI

Mutations apply locally immediately; a `SyncCommand` is queued in `pending_commands` and a revert snapshot
in `temp_id_pending` (keyed by command uuid). The background task flushes via the Sync API. On a server
rejection **or** a flush network failure, the snapshot is reverted and an error popup is shown — no silent
divergence. A racing delta must not clobber an in-flight edit: `apply_sync_delta` skips incremental items
whose task is still in `temp_id_pending`.

**Invariant — one command per flush.** Callers queue + flush one command at a time; the revert path stores
absolute `before` snapshots that don't compose under reordering, so batching same-task edits into one flush
would make failure-revert order-dependent. Keep it one-at-a-time.

## Working in this repo

- Build / lint / test: `cargo build --workspace` · `cargo clippy --workspace --all-targets -- -D warnings`
  · `cargo fmt --all --check` · `cargo test --workspace`. CI gates all of these plus an MSRV (1.88) job.
- **MSRV is 1.88** (edition 2024; required by ratatui 0.30 / time / darling). It is *not* 1.85.
- Branch flow: `staging` is the long-lived integration branch; feature work branches off it and PRs in;
  `staging → main` triggers a release. Publishing is **manual and main-only** (`publish.yml`
  `workflow_dispatch`); **core publishes before tui** (tui resolves core from crates.io); versions stay in
  lockstep. crates.io versions are immutable — never reuse a number.
- Errors use `anyhow`; never panic on API errors — surface them via the error popup (Rose Pine `love`).
  Network failures degrade to cached data. The terminal is restored on panic (`ratatui::init` installs a
  panic hook).

## Gotchas

- `crates/ratatoist-core/examples/*.rs`, if present locally, hit a **real Todoist account** and call a
  deleted API surface. They are untracked/gitignored — not in CI or the published crate. Don't run them;
  `rm` them so `cargo test` / `--all-targets` build locally.
- **`ratatoist-tui` 0.4.0 is burned** on crates.io (published then yanked; numbers are permanent). Never
  reuse it — the next minor is 0.5.0.
- Completed-tasks pagination follows a top-level `next_cursor` that is **unverified against the live API**;
  if the field is absent it safely no-ops (first page only). Comments use the confirmed `Paginated` shape.

## Design north star

- **lazygit**: panel navigation, contextual keybindings, information density. **gum**: clean prompts,
  bordered boxes with padding, minimal chrome, no visual noise.
- Keyboard-first, vim conventions (`h/j/k/l`, `/`, `?`, `Esc`). Progressive disclosure: task list → `Enter`
  → detail. **Rose Pine everywhere** — no default terminal colors left unstyled. Rounded borders on floats.

## Color usage

| Token  | Hex       | Use                               |
|--------|-----------|-----------------------------------|
| love   | `#eb6f92` | P1, overdue, errors / destructive |
| gold   | `#f6c177` | P2, due today, warnings           |
| rose   | `#ebbcba` | P3                                |
| pine   | `#31748f` | completed tasks, success          |
| foam   | `#9ccfd8` | selected / focused, active border |
| iris   | `#c4a7e7` | project names, label badges       |
| muted  | `#6e6a86` | timestamps, metadata, inactive    |

## Status & roadmap

Implemented: sync transport, incremental delta, websocket real-time refresh, optimistic mutations
(add/complete/update/comment) with revert, the org tree (workspaces/folders/projects), task subtree folds,
section headers, StatsDock filtering, sort cycling, completed-tasks fetch, detail-pane editing, comments,
10 built-in themes + custom, `--new-user` onboarding, the `?` cheatsheet, and force re-sync (`R`). See
`CHANGELOG.md` for version history.

Not yet: delete task (`d`) + confirm, global search (`/`), move task (`m`), filter/sort persistence,
scroll-position indicator. The big refactor is extracting a UI-agnostic `Store` into core (which is what
makes `ratatoist-nvim` buildable).

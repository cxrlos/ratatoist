# ratatoist

A terminal client for Todoist, written in Rust with ratatui. One crate; the binary is `ratatoist`.

## Read first

- `GLOSSARY.md`: the words for what ratatoist shows and acts on. Use them in code, docs and issues; never use a term listed under _Avoid_.
- `docs/adr/`: the decisions in force, one file each.
- `docs/agents/`: issue tracker, triage labels and domain-doc rules.
- `docs/releasing.md`: how versions, tags and publishing work.

## Status

Active. Each milestone's work lands on a `feat/vX.Y.0` branch, which is squash-merged into `main` and ships as that minor version. Plans live in GitHub issues; repo files describe what exists.

## Critical rules

1. Never read, print, log or commit a Todoist API token or `.env` file. Tests never call the Todoist API; responses come from fixtures recorded from real responses, and a fixture that is not one is named `HYPOTHETICAL_*`.
2. The PR title is the commit: a Conventional Commit, with `!` for breaking changes. Squash merge only.
3. release-plz owns `CHANGELOG.md` and the version in `Cargo.toml`. Never edit them by hand.
4. Never accept a changed insta snapshot without showing the `.snap.new` diff first. Generate every pending snapshot in one run with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new cargo test --locked --no-fail-fast`, review them, then accept once.
5. `main` and every `feat/vX.Y.0` branch are shared: add commits on top, never rewrite them. Every CI job and the `.claude/settings.json` format hook gate each change; fix what they flag instead of bypassing them.
6. MSRV is 1.88 (edition 2024; ratatui 0.30 needs it). A dependency or language feature that raises it needs an ADR.
7. Linux and macOS only. Do not add Windows-specific code paths.

## Workflow

1. Orient: read the issue, `GLOSSARY.md` and the relevant ADRs. Branch `<type>/<issue>-<slug>` off `main` (during a milestone, off `feat/vX.Y.0`).
2. Agree seams: name the module and public interface you will touch before writing code.
3. Red then green with `/tdd`. After each slice, run that test target and `cargo clippy`.
4. Verify: every command under Commands passes.
5. Review: `/code-review` against the merge-base.
6. Write back: fill in `.github/pull_request_template.md`, with `Fixes #N` or `Part of #N`.

Done when someone reading only the issue and the PR could pick up the next piece.

## Design principles

These are targets. New code meets them; older code is brought up to them as it is touched.

- Rendering is a pure function of app state; every screen has a `TestBackend` + insta snapshot.
- All Todoist API access goes through one module, so tests swap it for fixtures.
- One action registry drives key handling, the keymap config, the `?` cheatsheet, the bottom-bar hints and the palette. Never list a key binding anywhere else.
- No `unwrap`/`expect` outside tests; errors carry context to the screen, and a network failure keeps the data already loaded.
- Every style comes from a `Theme` method named for its role (`due_overdue`, `error_border`), so built-in and user themes reach every widget.
- Text is measured and cut only through `src/ui/text.rs`: widths in terminal cells, and cut text ends in `…`.
- The terminal is restored on every exit path, panics included.

## Map

When you add, move or split a module, update its entry here in the same PR.

- `src/main.rs`: the `ratatoist` binary. Parses the CLI, runs onboarding when there is no token, loads the config, starts logging and runs the app.
- `src/lib.rs`: the library root, so tests import `ratatoist::...`.
- `src/app.rs`: the core. `App` state, the event loop, the background task fed over an mpsc channel, the WebSocket refresh and optimistic writes. A write applies locally, queues a `SyncCommand` in `pending_commands` and stores a revert snapshot in `temp_id_pending`, keyed by command uuid; a rejection or a failed flush restores the snapshot and shows the error. Invariant: one command per flush. Queue and flush one command at a time, because the snapshots are absolute `before` values that do not compose when same-task edits share a flush. `apply_sync_delta` skips incremental items whose task is still in `temp_id_pending`, so a racing delta never clobbers an in-flight edit.
- `src/keys.rs`: key dispatch for the Vim (Normal, Insert) and Standard modes.
- `src/todoist/`: all Todoist access. `client.rs` (the reqwest client, retrying 429s with backoff), `model.rs` (the serde types), `sync.rs` (sync requests, commands and responses). Reads and writes go through `POST /api/v1/sync` with a sync token (`*` for a full sync, otherwise an incremental one). Comments and completed tasks are cursor-paged; completed tasks follow a top-level `next_cursor` that is unverified against the live API, so when it is absent only the first page loads.
- `src/config.rs`: the token from `TODOIST_API_TOKEN` or `config.toml`, kept at mode 0600. Its `Debug` impl redacts the token.
- `src/sync_state.rs`: the sync token, persisted atomically to `sync_state.json` in the config directory.
- `src/terminal.rs`: the terminal guard, the one owner of raw mode and the alternate screen. `Guard::enter()` takes the terminal; dropping the guard, or a panic while it lives, restores it, the panic hook doing so before the message prints. `shutdown_signal()` resolves on SIGINT, SIGTERM or SIGHUP, which the runtime turns into `Event::Shutdown`. `tests/pty.rs` checks all of this against the real binary in a pseudo-terminal.
- `src/logging.rs`: `tracing` as JSON lines to `ratatoist.<date>.log` in the directory `main` passes (`paths::state_dir()`), rolled daily and keeping seven files, never to the terminal. The level comes from `RATATOIST_LOG` (an `EnvFilter` string, default `ratatoist=info`); `RUST_LOG` is ignored. tungstenite's handshake is never logged. `init` returns the `WorkerGuard`, which `main` holds until it returns.
- `src/ui/`: rendering. Layout, splash and setup screens, status bar and key hints, dates (`dates.rs`), themes (`theme.rs`: the built-ins under `themes/` and user Base16 files), the views (projects, tasks, detail, settings) and the components (popups, pickers, the task form, the cheatsheet).

## Commands

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo insta pending-snapshots
cargo +1.88 check --all-targets --locked
cargo deny check
cargo package --locked
gitleaks git --redact
cargo run -- --demo
```

CI runs all of these except `cargo run` on every PR, with tests on Linux and macOS. `gitleaks git` is the local form of CI's full-history secret scan. Until `--demo` exists, `cargo run` needs a token.

## Agent skills

### Issue tracker

GitHub Issues on cxrlos/ratatoist, through the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

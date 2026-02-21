# ratatoist-core

Core library for ratatoist: async Todoist API v1 client, configuration, and structured logging.

This crate has zero TUI dependencies and can be used independently for any Rust project that needs to interact with the Todoist API.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
ratatoist-core = { path = "../ratatoist-core" }
```

## Modules

- **`api::client`** -- Async HTTP client for Todoist API v1 with pagination, structured logging, and error handling.
- **`api::models`** -- Data models: Project, Task, Comment, Label, Section, Collaborator, and request/response types.
- **`config`** -- Token loading from environment variable or `~/.config/ratatoist/config.toml` with file permission validation.
- **`logging`** -- Structured JSON logging to file with configurable log levels.

## API coverage

| Resource | GET | POST | UPDATE | DELETE |
|----------|-----|------|--------|--------|
| Projects | all, by id | create | update (name, color, favorite) | delete |
| Tasks | all, by project | create | update (content, description, priority, due, labels) | - |
| Tasks | - | close, reopen | - | - |
| Comments | by task | create | - | - |
| Labels | all | create | - | delete |
| Sections | by project | - | - | - |
| Collaborators | by project | - | - | - |
| User | current user info | - | - | - |

## Usage

```rust
use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::config::Config;

let config = Config::load()?;
let client = TodoistClient::new(config.token())?;

let projects = client.get_projects().await?;
let tasks = client.get_tasks(Some(&projects[0].id)).await?;
```

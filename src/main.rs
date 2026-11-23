use std::io;
use std::path::Path;

use anyhow::Result;
use clap::Parser;
use crossterm::event::EventStream;
use ratatui::backend::CrosstermBackend;

use ratatoist::app::App;
use ratatoist::cli::{Cli, Command};
use ratatoist::config::{self, Config};
use ratatoist::todoist::{FakeClient, HttpClient, TodoistClient};
use ratatoist::{auth, logging, paths, runtime, terminal};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Auth(command)) = cli.command {
        return auth::run(command).await;
    }

    let config_dir = paths::config_dir()?;
    let config = config::load(&config_dir);
    for warning in &config.warnings {
        eprintln!("{warning}");
    }
    let _log_guard = logging::init(&paths::state_dir()?)?;

    if cli.demo {
        // The demo account is fictional: its sync token and settings must
        // never replace the real ones.
        run_app(FakeClient::new(), &config, None).await
    } else {
        let resolved = auth::resolve()?;
        let client = HttpClient::new(&resolved.token)?;
        run_app(client, &config, Some(&config_dir)).await
    }
}

/// Runs the TUI against `client`, reading and saving the sync token and
/// settings in `files`.
async fn run_app<C: TodoistClient>(client: C, config: &Config, files: Option<&Path>) -> Result<()> {
    let shutdown = terminal::shutdown_signal()?;
    let _guard = terminal::Guard::enter()?;
    let state = runtime::initial_state(config, files);
    let mut app = App::new(CrosstermBackend::new(io::stdout()), state)?;
    runtime::run(&mut app, &client, EventStream::new(), shutdown, files).await
}

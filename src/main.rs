use std::io;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use crossterm::event::EventStream;
use ratatui::backend::CrosstermBackend;

use ratatoist::app::App;
use ratatoist::cli::{Cli, Command};
use ratatoist::config::{self, Config};
use ratatoist::todoist::{HttpClient, TodoistClient, demo};
use ratatoist::{auth, logging, paths, runtime, terminal};

/// Prints a failure as its plain message on stderr and exits 1. Every
/// terminal guard has dropped by the time `start` returns, so the message
/// lands on the restored screen.
#[tokio::main]
async fn main() -> ExitCode {
    match start(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn start(cli: Cli) -> Result<()> {
    if let Some(Command::Auth(command)) = cli.command {
        return auth::run(command).await;
    }

    if cli.demo {
        // The demo account is fictional: it reads nothing from the config
        // folder (no config.toml, themes, settings or sync token) and writes
        // nothing there, so it always starts from defaults.
        let _log_guard = logging::init(&paths::state_dir()?)?;
        return run_app(demo::client(runtime::today()), &Config::default(), None).await;
    }

    let config_dir = paths::config_dir()?;
    let config = config::load(&config_dir);
    for warning in &config.warnings {
        eprintln!("{warning}");
    }
    let _log_guard = logging::init(&paths::state_dir()?)?;
    let resolved = auth::resolve()?;
    let client = HttpClient::new(&resolved.token)?;
    run_app(client, &config, Some(&config_dir)).await
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

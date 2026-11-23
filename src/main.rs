use std::io;

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

    let config = config::load(&paths::config_dir()?);
    for warning in &config.warnings {
        eprintln!("{warning}");
    }
    let _log_guard = logging::init(&paths::state_dir()?)?;

    if cli.demo {
        run_app(FakeClient::new(), &config, cli.idle_forcer).await
    } else {
        let resolved = auth::resolve()?;
        let client = HttpClient::new(&resolved.token)?;
        run_app(client, &config, cli.idle_forcer).await
    }
}

async fn run_app<C: TodoistClient>(client: C, config: &Config, idle_forcer: bool) -> Result<()> {
    let shutdown = terminal::shutdown_signal()?;
    let _guard = terminal::Guard::enter()?;
    let state = runtime::initial_state(config, idle_forcer);
    let mut app = App::new(CrosstermBackend::new(io::stdout()), state)?;
    runtime::run(&mut app, &client, EventStream::new(), shutdown).await
}

use std::io;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use crossterm::event::EventStream;
use ratatui::backend::CrosstermBackend;

use ratatoist::app::{App, LoadError, ReleaseState, State};
use ratatoist::cli::{Cli, Command};
use ratatoist::config::{self, Config};
use ratatoist::release::{self, Channel, ReleaseCheck};
use ratatoist::todoist::{FakeClient, HttpClient, TodoistClient, demo};
use ratatoist::{auth, logging, paths, runtime, terminal};

/// Prints a failure as its plain message on stderr and exits 1. Every
/// terminal guard has dropped by the time `start` returns, so the message
/// lands on the restored screen. A missing or rejected token is not such a
/// failure: the app starts and says so in its body.
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
    if let Some(Command::Auth(command)) = &cli.command {
        return auth::run(command.clone()).await;
    }

    if cli.demo {
        // The demo account is fictional: it reads nothing from the config
        // folder (no config.toml, themes, settings or sync token) and writes
        // nothing there, so it always starts from defaults.
        let _log_guard = logging::init(&paths::state_dir()?)?;
        let config = Config::default();
        let mut state = runtime::initial_state(&config, None);
        let releases = release_check(&cli, &config, &mut state);
        return run_app(
            demo::client(runtime::today()),
            state,
            None,
            releases.as_ref(),
        )
        .await;
    }

    let config_dir = paths::config_dir()?;
    let config = config::load(&config_dir);
    for warning in &config.warnings {
        eprintln!("{warning}");
    }
    let _log_guard = logging::init(&paths::state_dir()?)?;
    let mut state = runtime::initial_state(&config, Some(&config_dir));
    let releases = release_check(&cli, &config, &mut state);
    let resolved = match auth::resolve() {
        Ok(resolved) => resolved,
        Err(error) if runtime::is_missing_token(&error) => {
            state.not_loaded(LoadError::NoToken);
            // With no token there is nothing to ask Todoist, and no sync
            // starts, so the client is never called and no sync token is
            // saved.
            return run_app(FakeClient::new(), state, None, releases.as_ref()).await;
        }
        Err(error) => return Err(error),
    };
    state.token_source = Some(resolved.source);
    let client = HttpClient::new(&resolved.token)?;
    run_app(client, state, Some(&config_dir), releases.as_ref()).await
}

/// The release check for this run, with its cached record and install
/// channel set on `state`. Every start calls it, `--demo` too, so
/// `release::background_check` alone decides that the demo never asks
/// GitHub.
fn release_check(cli: &Cli, config: &Config, state: &mut State) -> Option<ReleaseCheck> {
    let check = release::background_check(
        cli,
        &config.updates,
        std::env::var_os(release::OFF_VAR).as_deref(),
    )?;
    state.release = ReleaseState::new(true, Channel::of_running(), check.cache.load());
    Some(check)
}

/// Runs the TUI from `state` against `client`, saving the sync token in
/// `files` and asking `releases` for a newer release.
async fn run_app<C: TodoistClient>(
    client: C,
    state: State,
    files: Option<&Path>,
    releases: Option<&ReleaseCheck>,
) -> Result<()> {
    let shutdown = terminal::shutdown_signal()?;
    let _guard = terminal::Guard::enter()?;
    let mut app = App::new(CrosstermBackend::new(io::stdout()), state)?;
    runtime::run_checking(
        &mut app,
        &client,
        EventStream::new(),
        shutdown,
        files,
        releases,
    )
    .await
}

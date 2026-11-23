//! The async loop: terminal input, the shutdown signal, a clock tick and
//! effects carried out against a [`TodoistClient`], each answer fed back as
//! an [`Event`]. The one place that reads the clock.

use std::future::Future;
use std::io;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local};
use crossterm::event::Event as TerminalEvent;
use futures_util::{Stream, StreamExt};
use ratatui::backend::Backend;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tracing::warn;

use crate::app::{App, DEFAULT_IDLE_TIMEOUT_SECS, Effect, Event, State};
use crate::config::Config;
use crate::sync_state::SyncState;
use crate::todoist::sync::SyncRequest;
use crate::todoist::{CompletedQuery, TodoistClient};
use crate::ui::theme::Theme;

/// How often the clock moves on. Ticks redraw only when they change what
/// shows (the idle state or the date).
const TICK: Duration = Duration::from_secs(1);

const UI_SETTINGS: &str = "ui_settings.json";

/// What the Settings pane saved last time, from `ui_settings.json`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct UiSettings {
    pub theme: Option<String>,
    pub idle_timeout_secs: Option<u64>,
    /// Written by older versions.
    pub idle_timeout_mins: Option<u64>,
}

impl UiSettings {
    /// Reads `ui_settings.json` in `dir`; missing or broken reads as empty.
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join(UI_SETTINGS))
            .ok()
            .and_then(|src| serde_json::from_str(&src).ok())
            .unwrap_or_default()
    }

    fn idle_timeout(&self) -> Option<u64> {
        self.idle_timeout_secs
            .or(self.idle_timeout_mins.map(|mins| mins * 60))
    }
}

/// The first state, with "now" read from the clock and the saved settings
/// and sync token read from `files`. With `None` nothing is read: no saved
/// settings and a full sync.
pub fn initial_state(config: &Config, files: Option<&Path>) -> State {
    let ui = files.map(UiSettings::load).unwrap_or_default();
    let sync_token = files
        .map(|dir| SyncState::load(dir).sync_token)
        .unwrap_or_else(|| "*".to_owned());
    initial_state_from(config, &ui, sync_token, Local::now())
}

/// The first state from what was read. Where `config` and `ui` both set the
/// theme or the idle timeout, `config` wins.
pub fn initial_state_from(
    config: &Config,
    ui: &UiSettings,
    sync_token: String,
    now: DateTime<Local>,
) -> State {
    let mut state = State::new(now);
    state.sync_token = sync_token;
    state.themes = Theme::builtin();
    state.themes.extend(config.user_themes.iter().cloned());
    let chosen = config
        .theme
        .as_deref()
        .is_some_and(|name| state.select_theme_named(name));
    if !chosen && let Some(name) = ui.theme.as_deref() {
        state.select_theme_named(name);
    }
    state.idle_timeout_secs = config
        .idle_timeout
        .map(|timeout| timeout.as_secs())
        .or(ui.idle_timeout())
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);
    state
}

/// Runs until the app asks to quit, `shutdown` resolves or terminal input
/// ends, saving the sync token and settings in `files`, or nowhere when it
/// is `None`.
pub async fn run<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    mut input: I,
    shutdown: S,
    files: Option<&Path>,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    C: TodoistClient,
    I: Stream<Item = io::Result<TerminalEvent>> + Unpin,
    S: Future<Output = ()>,
{
    let mut shutdown = std::pin::pin!(shutdown);
    let (tx, mut answers) = mpsc::unbounded_channel();
    let outside = Outside {
        client,
        answers: tx,
        files: files.map(Path::to_path_buf),
    };
    let mut ticks = tokio::time::interval(TICK);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut next = Some(Event::Start);
    loop {
        if let Some(event) = next.take() {
            for effect in app.handle(event)? {
                if outside.execute(app.state(), effect).is_break() {
                    return Ok(());
                }
            }
        }
        next = tokio::select! {
            () = &mut shutdown => Some(Event::Shutdown),
            Some(answer) = answers.recv() => Some(answer),
            read = input.next() => match read {
                None => return Ok(()),
                Some(read) => from_terminal(read?),
            },
            _ = ticks.tick() => Some(Event::Tick { now: Local::now() }),
        };
    }
}

fn from_terminal(event: TerminalEvent) -> Option<Event> {
    match event {
        TerminalEvent::Key(key) => Some(Event::Key(key)),
        TerminalEvent::Resize(width, height) => Some(Event::Resize { width, height }),
        _ => None,
    }
}

/// What effects reach: the client, the way back for answers, and the folder
/// files are saved in.
struct Outside<'a, C> {
    client: &'a C,
    answers: mpsc::UnboundedSender<Event>,
    files: Option<PathBuf>,
}

impl<C: TodoistClient> Outside<'_, C> {
    /// Carries out `effect`. A request runs in its own task and comes back
    /// as an event; saving a file happens here, in order.
    fn execute(&self, state: &State, effect: Effect) -> ControlFlow<()> {
        match effect {
            Effect::Quit => return ControlFlow::Break(()),
            Effect::Sync(request) => {
                self.spawn(|client| async move {
                    Event::Synced(client.sync(request).await.map(Box::new))
                });
            }
            Effect::SendCommands(commands) => {
                let uuids = commands.iter().map(|c| c.uuid.clone()).collect();
                let request = SyncRequest {
                    sync_token: state.sync_token.clone(),
                    resource_types: vec![],
                    commands,
                };
                self.spawn(|client| async move {
                    let result = client.sync(request).await.map(Box::new);
                    Event::CommandsSent { uuids, result }
                });
            }
            Effect::FetchComments { task_id, fetch_seq } => {
                self.spawn(|client| async move {
                    let result = client.comments(&task_id).await;
                    Event::Comments {
                        task_id,
                        fetch_seq,
                        result,
                    }
                });
            }
            Effect::FetchCompletedTasks(query) => {
                let project_id = query.project_id.clone();
                self.spawn(|client| async move {
                    let result = client.completed_tasks(CompletedQuery { ..query }).await;
                    Event::CompletedTasks { project_id, result }
                });
            }
            Effect::OpenLive { url } => {
                let messages = self.client.live_events(&url);
                let answers = self.answers.clone();
                tokio::spawn(async move {
                    let mut messages = std::pin::pin!(messages);
                    while let Some(message) = messages.next().await {
                        if answers.send(Event::Live(message)).is_err() {
                            break;
                        }
                    }
                });
            }
            Effect::PersistSyncToken(sync_token) => {
                if let Some(dir) = &self.files
                    && let Err(error) = (SyncState { sync_token }).save(dir)
                {
                    warn!(error = %error, "failed to persist sync token");
                }
            }
            Effect::SaveUiSettings {
                theme,
                idle_timeout_secs,
            } => {
                if let Some(dir) = &self.files
                    && let Err(error) = save_ui_settings(dir, &theme, idle_timeout_secs)
                {
                    warn!(error = %error, "failed to save ui settings");
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// Runs `request` against a clone of the client in its own task and
    /// sends the event it ends with back to the loop.
    fn spawn<F, Fut>(&self, request: F)
    where
        F: FnOnce(C) -> Fut,
        Fut: Future<Output = Event> + Send + 'static,
    {
        let answer = request(self.client.clone());
        let answers = self.answers.clone();
        tokio::spawn(async move {
            // The loop has stopped if this fails; nothing is waiting.
            let _ = answers.send(answer.await);
        });
    }
}

fn save_ui_settings(dir: &Path, theme: &str, idle_timeout_secs: u64) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::json!({
        "theme": theme,
        "idle_timeout_secs": idle_timeout_secs,
    });
    std::fs::write(dir.join(UI_SETTINGS), serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::TimeZone;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::todoist::FakeClient;

    fn now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 22, 9, 0, 0)
            .earliest()
            .unwrap()
    }

    fn ui(theme: &str, idle_timeout_secs: u64) -> UiSettings {
        UiSettings {
            theme: Some(theme.to_owned()),
            idle_timeout_secs: Some(idle_timeout_secs),
            idle_timeout_mins: None,
        }
    }

    fn start(config: &Config, ui: &UiSettings) -> State {
        initial_state_from(config, ui, "*".to_owned(), now())
    }

    #[test]
    fn config_values_win_over_ui_settings() {
        let config = Config {
            theme: Some("Nord".to_owned()),
            idle_timeout: Some(Duration::from_secs(900)),
            ..Config::default()
        };

        let state = start(&config, &ui("Dracula", 120));

        assert_eq!(state.theme().name, "Nord");
        assert_eq!(state.idle_timeout_secs, 900);
    }

    #[test]
    fn ui_settings_fill_in_what_config_leaves_out() {
        let state = initial_state_from(
            &Config::default(),
            &ui("Dracula", 120),
            "saved-token".to_owned(),
            now(),
        );

        assert_eq!(state.theme().name, "Dracula");
        assert_eq!(state.idle_timeout_secs, 120);
        assert_eq!(state.sync_token, "saved-token");
    }

    #[test]
    fn an_unknown_config_theme_falls_back_to_ui_settings() {
        let config = Config {
            theme: Some("No Such Theme".to_owned()),
            ..Config::default()
        };

        assert_eq!(start(&config, &ui("Dracula", 120)).theme().name, "Dracula");
    }

    #[test]
    fn a_user_theme_from_config_can_be_chosen() {
        let mut custom = Theme::builtin().remove(0);
        custom.name = "Mine".to_owned();
        let config = Config {
            theme: Some("Mine".to_owned()),
            user_themes: vec![custom],
            ..Config::default()
        };

        let state = start(&config, &UiSettings::default());

        assert_eq!(state.theme().name, "Mine");
        assert_eq!(state.idle_timeout_secs, DEFAULT_IDLE_TIMEOUT_SECS);
    }

    #[test]
    fn ui_settings_read_the_older_minutes_field() {
        let settings: UiSettings =
            serde_json::from_str(r#"{"theme":"Nord","idle_timeout_mins":10}"#).unwrap();

        assert_eq!(settings.idle_timeout(), Some(600));
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_sync_runs_against_the_client_and_comes_back() {
        let state = start(&Config::default(), &UiSettings::default());
        let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
        assert!(app.state().loading);

        let shutdown = tokio::time::sleep(Duration::from_secs(5));
        let no_input = futures_util::stream::pending();
        run(&mut app, &FakeClient::new(), no_input, shutdown, None)
            .await
            .unwrap();

        assert!(!app.state().loading);
        assert!(app.state().error.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn the_sync_token_is_saved_only_in_the_folder_given() {
        let dir = std::env::temp_dir().join(format!("ratatoist-runtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for files in [None, Some(dir.as_path())] {
            let state = initial_state(&Config::default(), files);
            assert_eq!(state.sync_token, "*");
            let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
            let shutdown = tokio::time::sleep(Duration::from_secs(5));
            let no_input = futures_util::stream::pending();
            run(&mut app, &FakeClient::new(), no_input, shutdown, files)
                .await
                .unwrap();
            assert_eq!(SyncState::path(&dir).exists(), files.is_some());
        }

        let resumed = initial_state(&Config::default(), Some(&dir)).sync_token;
        let without_files = initial_state(&Config::default(), None).sync_token;
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(resumed, "fake");
        assert_eq!(without_files, "*");
    }

    #[tokio::test(start_paused = true)]
    async fn a_quit_key_ends_the_loop() {
        let state = start(&Config::default(), &UiSettings::default());
        let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
        let q = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
        let input = futures_util::stream::iter([Ok(TerminalEvent::Key(q))])
            .chain(futures_util::stream::pending());

        run(
            &mut app,
            &FakeClient::new(),
            input,
            std::future::pending(),
            None,
        )
        .await
        .unwrap();
    }
}

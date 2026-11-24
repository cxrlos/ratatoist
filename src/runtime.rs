//! The async loop: terminal input, the shutdown signal, a clock tick and
//! effects carried out against a [`TodoistClient`], each answer fed back as
//! an [`Event`]. The one place that reads the clock.

use std::future::Future;
use std::io;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate};
use crossterm::event::Event as TerminalEvent;
use futures_util::{Stream, StreamExt};
use ratatui::backend::Backend;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::app::{App, DEFAULT_IDLE_TIMEOUT_SECS, Effect, Event, State};
use crate::browser;
use crate::config::Config;
use crate::keys::Keymap;
use crate::release::ReleaseCheck;
use crate::sync_state::SyncState;
use crate::todoist::TodoistClient;
use crate::todoist::sync::SyncRequest;
use crate::ui::theme::{self, Theme};

/// How often the clock moves on. Ticks redraw only when they change what
/// shows (the idle state or the date).
const TICK: Duration = Duration::from_secs(1);

const UI_SETTINGS: &str = "ui_settings.json";

/// What versions before 0.7.0 saved in `ui_settings.json`. ratatoist reads
/// it once at startup, to carry its values into a session and say which
/// `config.toml` lines replace it, and never writes or deletes it.
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

/// The first state, with "now" read from the clock, `NO_COLOR` from the
/// environment, and the old saved settings and the sync token read from
/// `files`. With `None` nothing is read: no saved settings and a full sync.
pub fn initial_state(config: &Config, files: Option<&Path>) -> State {
    let no_color = theme::no_color_requested(std::env::var_os(theme::NO_COLOR_VAR).as_deref());
    initial_state_read(config, files, no_color)
}

/// [`initial_state`] with `no_color` given instead of read.
fn initial_state_read(config: &Config, files: Option<&Path>, no_color: bool) -> State {
    let ui = files.map(UiSettings::load).unwrap_or_default();
    let sync_token = files
        .map(|dir| SyncState::load(dir).sync_token)
        .unwrap_or_else(|| "*".to_owned());
    initial_state_from(config, &ui, sync_token, clock(), no_color)
}

/// The first state from what was read. Where `config` and `ui` both set the
/// theme or the idle timeout, `config` wins; a value only `ui` sets applies
/// for this session, and the notice names the `config.toml` line that keeps
/// it. With `no_color` the one theme is [`Theme::no_color`].
pub fn initial_state_from(
    config: &Config,
    ui: &UiSettings,
    sync_token: String,
    now: DateTime<Local>,
    no_color: bool,
) -> State {
    let mut state = State::new(now);
    state.keymap = Keymap::new(config.keys.preset, &config.keys.overrides);
    state.sync_token = sync_token;
    state.icon_set = config.icons;
    if let Some(columns) = config.wide_layout_columns {
        state.wide_layout_columns = columns;
    }
    state.themes.extend(config.user_themes.iter().cloned());
    let mut lines = Vec::new();
    if let Some(name) = config.theme.as_deref() {
        state.select_theme_named(name);
    } else if let Some(name) = ui.theme.as_deref()
        && state.select_theme_named(name)
    {
        lines.push(format!(
            "[theme] name = \"{}\"",
            theme::slug(&state.theme().name)
        ));
    }
    state.idle_timeout_secs = match (config.idle_timeout, ui.idle_timeout()) {
        (Some(timeout), _) => timeout.as_secs(),
        (None, Some(secs)) => {
            lines.push(format!("[sync] idle-timeout = {secs}"));
            secs
        }
        (None, None) => DEFAULT_IDLE_TIMEOUT_SECS,
    };
    if !lines.is_empty() {
        state.notice = Some(format!(
            "{UI_SETTINGS} is read until config.toml sets these; to keep them, add to config.toml: {}",
            lines.join(" and ")
        ));
    }
    if no_color {
        state.themes = vec![Theme::no_color()];
        state.theme_idx = 0;
    }
    state
}

/// Runs until the app asks to quit, `shutdown` resolves or terminal input
/// ends, saving the sync token and settings in `files`, or nowhere when it
/// is `None`. It never asks for a newer release.
pub async fn run<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    input: I,
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
    run_checking(app, client, input, shutdown, files, None).await
}

/// [`run`], asking `releases` for a newer release when the app's release
/// state says a check is due.
pub async fn run_checking<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    mut input: I,
    shutdown: S,
    files: Option<&Path>,
    releases: Option<&ReleaseCheck>,
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
        releases,
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
            _ = ticks.tick() => Some(Event::Tick { now: clock() }),
        };
    }
}

/// Whether `error`, from resolving the token, says there is none, so the
/// app starts on its Not logged in screen instead of failing.
pub fn is_missing_token(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<crate::auth::AuthError>(),
        Some(crate::auth::AuthError::NoToken)
    )
}

/// Today's date on the wall clock.
pub fn today() -> NaiveDate {
    clock().date_naive()
}

/// The wall clock: the one read the `disallowed_methods` lint lets through.
#[allow(clippy::disallowed_methods)]
fn clock() -> DateTime<Local> {
    Local::now()
}

fn from_terminal(event: TerminalEvent) -> Option<Event> {
    match event {
        TerminalEvent::Key(key) => Some(Event::Key(key)),
        TerminalEvent::Resize(width, height) => Some(Event::Resize { width, height }),
        TerminalEvent::Paste(text) => Some(Event::Paste(text)),
        _ => None,
    }
}

/// What effects reach: the client, the way back for answers, the folder
/// files are saved in, and GitHub's releases with their cache.
struct Outside<'a, C> {
    client: &'a C,
    answers: mpsc::UnboundedSender<Event>,
    files: Option<PathBuf>,
    releases: Option<&'a ReleaseCheck>,
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
                self.spawn(|client| async move {
                    let result = client.completed_tasks(query.clone()).await;
                    Event::CompletedTasks { query, result }
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
            Effect::CheckRelease => {
                let releases = self.releases.cloned();
                let answers = self.answers.clone();
                tokio::spawn(async move {
                    let _ = answers.send(check_release(releases.as_ref()).await);
                });
            }
            Effect::SaveRelease(record) => {
                if let Some(releases) = self.releases
                    && let Err(error) = releases.cache.save(&record)
                {
                    warn!("could not save the release cache: {error:#}");
                }
            }
            Effect::OpenUrl(url) => {
                if let Err(error) = browser::open(&url) {
                    warn!("{error:#}");
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

/// The latest release, or `None`, logged, when it could not be read. A
/// failed check is quiet: no notice and no entry in the messages log.
async fn check_release(releases: Option<&ReleaseCheck>) -> Event {
    let Some(releases) = releases else {
        return Event::ReleaseChecked(None);
    };
    match releases.source.latest().await {
        Ok(latest) => {
            info!(%latest, "checked for a newer release");
            Event::ReleaseChecked(Some(latest))
        }
        Err(error) => {
            warn!("could not check for a newer release: {error:#}");
            Event::ReleaseChecked(None)
        }
    }
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
        initial_state_from(config, ui, "*".to_owned(), now(), false)
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
            false,
        );

        assert_eq!(state.theme().name, "Dracula");
        assert_eq!(state.idle_timeout_secs, 120);
        assert_eq!(state.sync_token, "saved-token");
    }

    #[test]
    fn ui_settings_values_come_with_a_notice_naming_the_config_lines() {
        let state = start(&Config::default(), &ui("Rose Pine", 120));

        assert_eq!(
            state.notice.as_deref(),
            Some(
                "ui_settings.json is read until config.toml sets these; to keep them, add to \
                 config.toml: [theme] name = \"rose-pine\" and [sync] idle-timeout = 120"
            )
        );
    }

    #[test]
    fn the_notice_names_only_what_config_toml_leaves_out() {
        let config = Config {
            theme: Some("Nord".to_owned()),
            ..Config::default()
        };

        let state = start(&config, &ui("Dracula", 120));

        assert_eq!(state.theme().name, "Nord");
        assert_eq!(
            state.notice.as_deref(),
            Some(
                "ui_settings.json is read until config.toml sets these; to keep them, add to \
                 config.toml: [sync] idle-timeout = 120"
            )
        );
    }

    #[test]
    fn there_is_no_notice_when_config_toml_sets_both_or_there_is_no_file() {
        let config = Config {
            theme: Some("Nord".to_owned()),
            idle_timeout: Some(Duration::from_secs(900)),
            ..Config::default()
        };

        assert_eq!(start(&config, &ui("Dracula", 120)).notice, None);
        assert_eq!(
            start(&Config::default(), &UiSettings::default()).notice,
            None
        );
    }

    #[test]
    fn a_saved_theme_that_no_longer_exists_is_left_out_of_the_notice() {
        let state = start(&Config::default(), &ui("Paper", 60));

        assert_eq!(state.theme(), &Theme::default());
        assert_eq!(
            state.notice.as_deref(),
            Some(
                "ui_settings.json is read until config.toml sets these; to keep them, add to \
                 config.toml: [sync] idle-timeout = 60"
            )
        );
    }

    #[test]
    fn ui_settings_json_is_read_from_the_folder_given_and_left_in_place() {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-ui-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let json = r#"{"theme":"Nord","idle_timeout_secs":120}"#;
        std::fs::write(dir.join(UI_SETTINGS), json).unwrap();

        let state = initial_state_read(&Config::default(), Some(&dir), false);
        let left = std::fs::read_to_string(dir.join(UI_SETTINGS)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(state.theme().name, "Nord");
        assert_eq!(state.idle_timeout_secs, 120);
        assert!(state.notice.is_some());
        assert_eq!(left, json);
    }

    #[test]
    fn no_color_draws_with_the_no_color_theme_whatever_the_config_picks() {
        let config = Config {
            theme: Some("Nord".to_owned()),
            ..Config::default()
        };

        let state =
            initial_state_from(&config, &UiSettings::default(), "*".to_owned(), now(), true);

        assert_eq!(state.theme(), &Theme::no_color());
    }

    #[test]
    fn a_user_theme_from_config_can_be_chosen() {
        let mut custom = Theme::built_in("nord").unwrap();
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
        assert!(app.state().failures.is_empty());
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

    #[test]
    fn only_a_missing_token_starts_the_app_logged_out() {
        use crate::auth::AuthError;

        assert!(is_missing_token(&AuthError::NoToken.into()));
        assert!(!is_missing_token(&AuthError::LoginRejected.into()));
        assert!(!is_missing_token(&anyhow::anyhow!("no config folder")));
    }

    #[tokio::test(start_paused = true)]
    async fn a_logged_out_start_asks_todoist_for_nothing() {
        let mut state = start(&Config::default(), &UiSettings::default());
        state.not_loaded(crate::app::LoadError::NoToken);
        let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
        let client = FakeClient::new();

        let shutdown = tokio::time::sleep(Duration::from_secs(5));
        let no_input = futures_util::stream::pending();
        run(&mut app, &client, no_input, shutdown, None)
            .await
            .unwrap();

        assert!(client.commands().is_empty());
        assert!(app.state().last_sync_at.is_none());
    }

    #[test]
    fn pasted_text_reaches_the_app_whole() {
        let pasted = from_terminal(TerminalEvent::Paste("two\nlines".to_owned()));

        assert!(matches!(pasted, Some(Event::Paste(text)) if text == "two\nlines"));
    }

    #[test]
    fn the_keymap_comes_from_the_config() {
        let config = crate::config::parse("[keys]\npreset = \"standard\"\n", &[]);

        let state = start(&config, &UiSettings::default());

        assert_eq!(state.keymap.preset(), crate::keys::Preset::Standard);
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

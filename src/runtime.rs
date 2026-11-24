//! The async loop: terminal input, the shutdown signal, a clock tick and
//! effects carried out against a [`TodoistClient`], each answer fed back as
//! an [`Event`]. The one place that reads the clock.

use std::future::Future;
use std::io;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::Event as TerminalEvent;
use futures_util::{Stream, StreamExt};
use jiff::Zoned;
use ratatui::backend::Backend;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::app::{App, DEFAULT_IDLE_TIMEOUT_SECS, Effect, Event, State};
use crate::browser;
use crate::cache::{self, Cache};
use crate::config::Config;
use crate::config_write;
use crate::keys::Keymap;
use crate::release::{Record, ReleaseCache, ReleaseCheck};
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

/// The folders a run on a real account reads and writes: the config folder
/// (the old settings, the old sync token) and the cache folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    pub config: PathBuf,
    pub cache: PathBuf,
}

/// The first state, with "now" read from the clock, `NO_COLOR` from the
/// environment, and the old saved settings read from `config_dir`. With
/// `None` nothing is read.
pub fn initial_state(config: &Config, config_dir: Option<&Path>) -> State {
    let no_color = theme::no_color_requested(std::env::var_os(theme::NO_COLOR_VAR).as_deref());
    initial_state_read(config, config_dir, no_color)
}

/// [`initial_state`] with `no_color` given instead of read.
fn initial_state_read(config: &Config, config_dir: Option<&Path>, no_color: bool) -> State {
    let ui = config_dir.map(UiSettings::load).unwrap_or_default();
    initial_state_from(config, &ui, "*".to_owned(), clock(), no_color)
}

/// Paints `state` from the cache in `cache_dir`, so the first sync asks only
/// for what changed. A cache that is missing, unreadable or from another
/// schema version leaves `state` as it was, to start with a full sync.
pub fn paint_from_cache(state: &mut State, cache_dir: &Path) {
    match Cache::load(cache_dir) {
        Some(cache) => {
            info!(tasks = cache.store.tasks.len(), "painting from the cache");
            state.paint_from_cache(cache.store, cache.sync_token, cache.user_id);
        }
        None => info!("no usable cache"),
    }
}

/// The first state from what was read. Where `config` and `ui` both set the
/// theme or the idle timeout, `config` wins; a value only `ui` sets applies
/// for this session, and the notice names the `config.toml` line that keeps
/// it. With `no_color` the one theme is [`Theme::no_color`].
pub fn initial_state_from(
    config: &Config,
    ui: &UiSettings,
    sync_token: String,
    now: Zoned,
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
        state.no_color = true;
    }
    state
}

/// Runs until the app asks to quit, `shutdown` resolves or terminal input
/// ends, saving the cache in `files`, or nowhere when it is `None`. It never asks for a newer release.
pub async fn run<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    input: I,
    shutdown: S,
    files: Option<&Files>,
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
    input: I,
    shutdown: S,
    files: Option<&Files>,
    releases: Option<&ReleaseCheck>,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    C: TodoistClient,
    I: Stream<Item = io::Result<TerminalEvent>> + Unpin,
    S: Future<Output = ()>,
{
    let writer = Writer::start(Save::write);
    run_writing(app, client, input, shutdown, files, releases, writer).await
}

/// [`run_checking`] with files saved by `writer`. It returns once the writer
/// has saved everything it was sent, so the last cache is on disk.
async fn run_writing<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    input: I,
    shutdown: S,
    files: Option<&Files>,
    releases: Option<&ReleaseCheck>,
    writer: Writer,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    C: TodoistClient,
    I: Stream<Item = io::Result<TerminalEvent>> + Unpin,
    S: Future<Output = ()>,
{
    let result = run_loop(app, client, input, shutdown, files, releases, writer.saves).await;
    if let Err(error) = writer.done.await {
        warn!("the file writer stopped: {error}");
    }
    result
}

async fn run_loop<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    input: I,
    shutdown: S,
    files: Option<&Files>,
    releases: Option<&ReleaseCheck>,
    saves: mpsc::UnboundedSender<Save>,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    C: TodoistClient,
    I: Stream<Item = io::Result<TerminalEvent>> + Unpin,
    S: Future<Output = ()>,
{
    let (tx, answers) = mpsc::unbounded_channel();
    let outside = Outside {
        client,
        answers: tx,
        files: files.cloned(),
        saves,
        releases,
    };
    let result = drive(app, &outside, answers, input, shutdown).await;
    // However the loop ended, what the store gained since the last write
    // goes to the writer, which `run` waits for.
    if let Some(effect) = app.state().unsaved_cache() {
        let _ = outside.execute(app.state(), effect);
    }
    result
}

/// Feeds events to the app and carries out its effects until it quits,
/// `shutdown` resolves or terminal input ends.
async fn drive<B, C, I, S>(
    app: &mut App<B>,
    outside: &Outside<'_, C>,
    mut answers: mpsc::UnboundedReceiver<Event>,
    mut input: I,
    shutdown: S,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    C: TodoistClient,
    I: Stream<Item = io::Result<TerminalEvent>> + Unpin,
    S: Future<Output = ()>,
{
    let mut shutdown = std::pin::pin!(shutdown);
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

/// The wall clock: the one read the `disallowed_methods` lint lets through.
#[allow(clippy::disallowed_methods)]
fn clock() -> Zoned {
    Zoned::now()
}

fn from_terminal(event: TerminalEvent) -> Option<Event> {
    match event {
        TerminalEvent::Key(key) => Some(Event::Key(key)),
        TerminalEvent::Resize(width, height) => Some(Event::Resize { width, height }),
        TerminalEvent::Paste(text) => Some(Event::Paste(text)),
        _ => None,
    }
}

/// A file for the [`Writer`] to save or delete.
enum Save {
    /// The cache to write in `dir`, or `None` to delete it.
    Cache {
        dir: PathBuf,
        cache: Option<Box<Cache>>,
    },
    /// The sync token versions before 0.8.0 kept in the config folder `dir`.
    ForgetOldSyncState {
        dir: PathBuf,
    },
    Release {
        cache: ReleaseCache,
        record: Record,
    },
    /// `[theme] name` to set in `config.toml` in the config folder `dir`;
    /// the result goes back over `answers`.
    Theme {
        dir: PathBuf,
        slug: String,
        answers: mpsc::UnboundedSender<Event>,
    },
}

impl Save {
    /// Writes the file. A failure is logged, and reaches the screen only for
    /// the theme, which the user asked to keep.
    fn write(self) {
        match self {
            Self::Cache {
                dir,
                cache: Some(cache),
            } => {
                if let Err(error) = cache.save(&dir) {
                    warn!("could not save the cache: {error:#}");
                }
            }
            Self::Cache { dir, cache: None } => {
                if let Err(error) = Cache::remove(&dir) {
                    warn!("could not delete the cache: {error}");
                }
            }
            Self::ForgetOldSyncState { dir } => {
                if let Err(error) = cache::remove_old_sync_state(&dir) {
                    warn!("could not delete the old sync token: {error}");
                }
            }
            Self::Release { cache, record } => {
                if let Err(error) = cache.save(&record) {
                    warn!("could not save the release cache: {error:#}");
                }
            }
            Self::Theme { dir, slug, answers } => {
                let result = config_write::save_theme(&dir, &slug);
                if let Err(error) = &result {
                    warn!(?error, "could not keep the theme in config.toml");
                }
                // After a quit nothing is listening, and nothing needs to.
                let _ = answers.send(Event::ThemeSaved { slug, result });
            }
        }
    }
}

/// The one writer of files: a task that runs each [`Save`] sent to `saves`
/// on the blocking pool, one at a time and in the order sent, so a slow disk
/// never holds up the loop and a later save never lands before an earlier
/// one. `done` finishes once `saves` is dropped and everything sent is saved.
struct Writer {
    saves: mpsc::UnboundedSender<Save>,
    done: JoinHandle<()>,
}

impl Writer {
    fn start(write: impl Fn(Save) + Send + Sync + 'static) -> Self {
        let write = Arc::new(write);
        let (saves, mut queue) = mpsc::unbounded_channel();
        let done = tokio::spawn(async move {
            while let Some(save) = queue.recv().await {
                let write = Arc::clone(&write);
                if let Err(error) = tokio::task::spawn_blocking(move || write(save)).await {
                    warn!("a file save stopped: {error}");
                }
            }
        });
        Self { saves, done }
    }
}

/// What effects reach: the client, the way back for answers, the folder
/// files are saved in and the writer that saves them, and GitHub's releases
/// with their cache.
struct Outside<'a, C> {
    client: &'a C,
    answers: mpsc::UnboundedSender<Event>,
    files: Option<Files>,
    saves: mpsc::UnboundedSender<Save>,
    releases: Option<&'a ReleaseCheck>,
}

impl<C: TodoistClient> Outside<'_, C> {
    /// Carries out `effect`. A request runs in its own task and comes back
    /// as an event; a file to save goes to the writer.
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
            Effect::FetchUser => {
                self.spawn(|client| async move { Event::UserFetched(client.user().await) });
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
            Effect::SaveCache => {
                if let Some(files) = &self.files {
                    // The writer outlives the loop, so this send cannot fail.
                    let _ = self.saves.send(Save::Cache {
                        dir: files.cache.clone(),
                        cache: Cache::of(state).map(Box::new),
                    });
                }
            }
            Effect::ForgetOldSyncState => {
                if let Some(files) = &self.files {
                    let _ = self.saves.send(Save::ForgetOldSyncState {
                        dir: files.config.clone(),
                    });
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
                if let Some(releases) = self.releases {
                    let _ = self.saves.send(Save::Release {
                        cache: releases.cache.clone(),
                        record,
                    });
                }
            }
            Effect::SaveTheme(slug) => {
                if let Some(files) = &self.files {
                    let _ = self.saves.send(Save::Theme {
                        dir: files.config.clone(),
                        slug,
                        answers: self.answers.clone(),
                    });
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

    use jiff::civil::date;
    use jiff::tz::TimeZone;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::todoist::FakeClient;

    fn now() -> Zoned {
        date(2026, 11, 22)
            .at(9, 0, 0, 0)
            .to_zoned(TimeZone::UTC)
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

    const FULL_SYNC: &str = include_str!("../tests/fixtures/todoist/HYPOTHETICAL_full_sync.json");

    /// A client serving the fixture account, whose full sync is dated, so
    /// one incremental sync follows it.
    fn account() -> FakeClient {
        FakeClient::new()
            .with_full_sync(serde_json::from_str(FULL_SYNC).unwrap())
            .with_incremental_sync(serde_json::json!({
                "full_sync": false,
                "sync_token": "incremental-token",
            }))
    }

    /// Config and cache in one empty folder for one test.
    fn files(test: &str) -> Files {
        let dir = temp_dir(test);
        Files {
            config: dir.join("config"),
            cache: dir.join("cache"),
        }
    }

    fn remove(files: &Files) {
        if let Some(dir) = files.cache.parent() {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    /// The state `main` builds for a real account with `files`.
    fn started_with(files: &Files) -> State {
        let mut state = initial_state(&Config::default(), Some(&files.config));
        paint_from_cache(&mut state, &files.cache);
        state
    }

    /// A cache in `files` for user `user_id` holding one task.
    fn cached(files: &Files, user_id: &str, content: &str) {
        let mut store = crate::app::Store::default();
        store.projects.upsert(crate::todoist::model::Project {
            id: "inbox".to_owned(),
            name: "Inbox".to_owned(),
            inbox_project: Some(true),
            ..crate::todoist::model::Project::default()
        });
        store.tasks.upsert(crate::todoist::model::Task {
            id: "cached-task".to_owned(),
            project_id: "inbox".to_owned(),
            content: content.to_owned(),
            // Overdue, so it shows on Today, the first screen, whatever day
            // the test runs.
            due: Some(crate::todoist::model::Due {
                date: "2020-01-01".to_owned(),
                ..crate::todoist::model::Due::default()
            }),
            ..crate::todoist::model::Task::default()
        });
        store.current_user_id = Some(user_id.to_owned());
        Cache {
            version: cache::SCHEMA_VERSION,
            user_id: user_id.to_owned(),
            sync_token: "cached-token".to_owned(),
            store,
        }
        .save(&files.cache)
        .unwrap();
    }

    /// Runs the app from `state` against `client` until five seconds have
    /// passed on the paused clock.
    async fn run_for_a_while(
        state: State,
        client: &FakeClient,
        files: Option<&Files>,
    ) -> App<TestBackend> {
        let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
        let shutdown = tokio::time::sleep(Duration::from_secs(5));
        let no_input = futures_util::stream::pending();
        run(&mut app, client, no_input, shutdown, files)
            .await
            .unwrap();
        app
    }

    fn screen(app: &App<TestBackend>) -> String {
        let buffer = app.backend().buffer();
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn the_cache_is_written_only_in_the_folders_given() {
        let files = files("written");
        std::fs::create_dir_all(&files.config).unwrap();
        let old = cache::old_sync_state_path(&files.config);
        std::fs::write(&old, r#"{"sync_token":"old"}"#).unwrap();

        run_for_a_while(started_with(&files), &account(), None).await;
        let without_files = (Cache::path(&files.cache).exists(), old.exists());
        run_for_a_while(started_with(&files), &account(), Some(&files)).await;
        let saved = Cache::load(&files.cache);
        let old_left = old.exists();
        remove(&files);

        assert_eq!(without_files, (false, true));
        let saved = saved.unwrap();
        assert_eq!(saved.user_id, "1000001");
        assert_eq!(saved.sync_token, "incremental-token");
        assert!(saved.store.tasks.iter().any(|t| t.content == "Buy compost"));
        assert!(!old_left, "the old sync token is deleted");
    }

    #[tokio::test(start_paused = true)]
    async fn the_cache_paints_the_tasks_before_the_first_sync_answers() {
        let files = files("paint");
        cached(&files, "1000001", "Cached errand");
        let state = started_with(&files);
        assert!(!state.loading);
        assert_eq!(state.sync_token, "cached-token");

        let app = run_for_a_while(state, &account().hold_syncs(), Some(&files)).await;
        remove(&files);

        assert!(app.state().last_sync_at.is_none(), "no sync answered");
        assert!(screen(&app).contains("Cached errand"), "{}", screen(&app));
    }

    #[tokio::test(start_paused = true)]
    async fn a_start_from_the_cache_asks_only_for_what_changed() {
        let files = files("resume");
        cached(&files, "1000001", "Cached errand");
        let client = FakeClient::new().with_incremental_sync(serde_json::json!({
            "full_sync": false,
            "sync_token": "resumed-token",
            "user": { "id": "1000001" },
        }));

        let app = run_for_a_while(started_with(&files), &client, Some(&files)).await;
        let saved = Cache::load(&files.cache).unwrap();
        remove(&files);

        assert_eq!(app.state().sync_token, "resumed-token");
        assert!(screen(&app).contains("Cached errand"), "{}", screen(&app));
        assert_eq!(saved.sync_token, "resumed-token");
        assert!(saved.store.tasks.by_id("cached-task").is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn a_first_answer_without_the_user_asks_todoist_who_it_is() {
        let files = files("no-user");
        cached(&files, "1000001", "Cached errand");
        let client = FakeClient::new()
            .with_incremental_sync(serde_json::json!({
                "full_sync": false,
                "sync_token": "resumed-token",
            }))
            .with_user(serde_json::json!({
                "id": "1000001",
                "websocket_url": "wss://ws.example.com/ws",
            }));

        let app = run_for_a_while(started_with(&files), &client, Some(&files)).await;
        remove(&files);

        assert!(app.state().live_opened);
        assert!(app.state().store.tasks.by_id("cached-task").is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn a_corrupt_cache_falls_back_to_a_full_sync() {
        let files = files("corrupt");
        std::fs::create_dir_all(&files.cache).unwrap();
        std::fs::write(Cache::path(&files.cache), "{ not json").unwrap();
        let state = started_with(&files);
        assert!(state.loading);
        assert_eq!(state.sync_token, "*");

        let app = run_for_a_while(state, &account(), Some(&files)).await;
        let saved = Cache::load(&files.cache);
        remove(&files);

        assert!(
            app.state()
                .store
                .tasks
                .iter()
                .any(|t| t.content == "Buy compost")
        );
        assert!(saved.is_some(), "the corrupt file is replaced");
    }

    #[tokio::test(start_paused = true)]
    async fn a_cache_of_another_user_is_discarded_for_a_full_sync() {
        let files = files("other-user");
        cached(&files, "someone-else", "Their errand");
        let client = account().with_incremental_sync(serde_json::json!({
            "full_sync": false,
            "sync_token": "incremental-token",
            "user": { "id": "1000001" },
        }));

        let app = run_for_a_while(started_with(&files), &client, Some(&files)).await;
        let saved = Cache::load(&files.cache).unwrap();
        remove(&files);

        let tasks = &app.state().store.tasks;
        assert!(tasks.by_id("cached-task").is_none());
        assert!(tasks.iter().any(|t| t.content == "Buy compost"));
        assert_eq!(saved.user_id, "1000001");
        assert!(saved.store.tasks.by_id("cached-task").is_none());
    }

    /// An empty folder for one test.
    fn temp_dir(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-runtime-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn two_cache_saves_in_a_row_leave_the_second_on_disk() {
        let files = files("order");
        let client = FakeClient::new();
        let writer = Writer::start(|save| {
            // Were saves run side by side, the first would land last.
            if matches!(&save, Save::Cache { cache: Some(c), .. } if c.sync_token == "first") {
                std::thread::sleep(Duration::from_millis(50));
            }
            save.write();
        });
        let (answers, _) = mpsc::unbounded_channel();
        let outside = Outside {
            client: &client,
            answers,
            files: Some(files.clone()),
            saves: writer.saves,
            releases: None,
        };
        let mut state = State::new(now());
        state.store.current_user_id = Some("1000001".to_owned());

        for sync_token in ["first", "second"] {
            state.sync_token = sync_token.to_owned();
            let flow = outside.execute(&state, Effect::SaveCache);
            assert!(flow.is_continue());
        }
        drop(outside);
        writer.done.await.unwrap();

        let saved = Cache::load(&files.cache).map(|c| c.sync_token);
        remove(&files);
        assert_eq!(saved.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn a_theme_picked_is_written_to_config_toml_and_answered() {
        let files = files("theme");
        let client = FakeClient::new();
        let writer = Writer::start(Save::write);
        let (answers, mut answered) = mpsc::unbounded_channel();
        let outside = Outside {
            client: &client,
            answers,
            files: Some(files.clone()),
            saves: writer.saves,
            releases: None,
        };

        let flow = outside.execute(&State::new(now()), Effect::SaveTheme("nord".to_owned()));
        assert!(flow.is_continue());
        drop(outside);
        writer.done.await.unwrap();

        let written = std::fs::read_to_string(files.config.join("config.toml"));
        remove(&files);
        assert_eq!(written.unwrap(), "[theme]\nname = \"nord\"\n");
        assert!(matches!(
            answered.recv().await,
            Some(Event::ThemeSaved { result: Ok(()), .. })
        ));
    }

    #[tokio::test]
    async fn with_no_config_folder_a_theme_picked_writes_nothing() {
        let files = files("theme-demo");
        let client = FakeClient::new();
        let writer = Writer::start(Save::write);
        let (answers, mut answered) = mpsc::unbounded_channel();
        let outside = Outside {
            client: &client,
            answers,
            files: None,
            saves: writer.saves,
            releases: None,
        };

        let flow = outside.execute(&State::new(now()), Effect::SaveTheme("nord".to_owned()));
        assert!(flow.is_continue());
        drop(outside);
        writer.done.await.unwrap();

        let written = files.config.exists();
        remove(&files);
        assert!(!written);
        assert!(answered.recv().await.is_none());
    }

    #[tokio::test]
    async fn the_cache_is_on_disk_when_run_returns_after_a_quit() {
        let files = files("quit");
        let (started, first_started) = tokio::sync::oneshot::channel();
        let started = std::sync::Mutex::new(Some(started));
        let writer = Writer::start(move |save| {
            if let Some(started) = started.lock().unwrap().take() {
                let _ = started.send(());
            }
            std::thread::sleep(Duration::from_millis(100));
            save.write();
        });
        // The first save is the old sync token's deletion, after the first
        // sync; the quit comes long before the cache's debounce would.
        let q = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
        let input = futures_util::stream::once(async move {
            first_started.await.unwrap();
            Ok(TerminalEvent::Key(q))
        })
        .chain(futures_util::stream::pending());
        let mut app = App::new(TestBackend::new(80, 24), started_with(&files)).unwrap();

        let client = account();
        let shutdown = std::future::pending();
        run_writing(
            &mut app,
            &client,
            Box::pin(input),
            shutdown,
            Some(&files),
            None,
            writer,
        )
        .await
        .unwrap();

        let saved = Cache::load(&files.cache).map(|c| c.user_id);
        remove(&files);
        assert_eq!(saved.as_deref(), Some("1000001"));
    }

    #[tokio::test]
    async fn the_release_record_found_before_quit_is_on_disk_when_run_returns() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let github = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "tag_name": "v99.0.0" })),
            )
            .mount(&github)
            .await;
        let dir = temp_dir("release");
        let check = ReleaseCheck {
            source: crate::release::ReleaseSource::new(format!("{}/latest", github.uri())).unwrap(),
            cache: ReleaseCache::new(&dir),
        };
        let (started, first_started) = tokio::sync::oneshot::channel();
        let started = std::sync::Mutex::new(Some(started));
        let writer = Writer::start(move |save| {
            if let Some(started) = started.lock().unwrap().take() {
                let _ = started.send(());
            }
            std::thread::sleep(Duration::from_millis(100));
            save.write();
        });
        let q = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('q'));
        let input = futures_util::stream::once(async move {
            first_started.await.unwrap();
            Ok(TerminalEvent::Key(q))
        })
        .chain(futures_util::stream::pending());
        let mut state = State::new(now());
        state.release = crate::app::ReleaseState::new(
            true,
            crate::release::Channel::CargoBin,
            Record::default(),
        );
        let mut app = App::new(TestBackend::new(80, 24), state).unwrap();

        let client = FakeClient::new();
        let shutdown = std::future::pending();
        let releases = Some(&check);
        run_writing(
            &mut app,
            &client,
            Box::pin(input),
            shutdown,
            None,
            releases,
            writer,
        )
        .await
        .unwrap();

        let saved = check.cache.load();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            saved.latest.map(|v| v.to_string()).as_deref(),
            Some("99.0.0")
        );
    }

    #[tokio::test]
    async fn a_held_up_save_does_not_stop_keys_or_redraws() {
        let files = files("held");
        std::fs::create_dir_all(&files.config).unwrap();
        let old = cache::old_sync_state_path(&files.config);
        std::fs::write(&old, r#"{"sync_token":"old"}"#).unwrap();
        let (started, first_started) = tokio::sync::oneshot::channel();
        let started = std::sync::Mutex::new(Some(started));
        let (open, gate) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Mutex::new(gate);
        let writer = Writer::start(move |save| {
            if let Some(started) = started.lock().unwrap().take() {
                let _ = started.send(());
            }
            let _ = gate.lock().unwrap().recv();
            save.write();
        });
        let help = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('?'));
        let still_there = old.clone();
        let key_while_held = futures_util::stream::once(async move {
            first_started.await.unwrap();
            Ok(TerminalEvent::Key(help))
        });
        // The loop asks for the next input only once it has handled and
        // drawn the key, so the save must still be held here.
        let then_open_the_gate = futures_util::stream::once(async move {
            assert!(still_there.exists(), "the save finished before the key");
            open.send(()).unwrap();
        })
        .filter_map(|()| std::future::ready(None::<io::Result<TerminalEvent>>));
        let input = Box::pin(key_while_held.chain(then_open_the_gate));
        let mut app = App::new(TestBackend::new(80, 24), started_with(&files)).unwrap();

        let client = account();
        let shutdown = std::future::pending();
        run_writing(
            &mut app,
            &client,
            input,
            shutdown,
            Some(&files),
            None,
            writer,
        )
        .await
        .unwrap();

        let old_left = old.exists();
        let saved = Cache::load(&files.cache).is_some();
        remove(&files);
        assert!(matches!(
            app.state().top_modal(),
            Some(crate::app::Modal::Cheatsheet { .. })
        ));
        assert!(app.frames_drawn() > 1);
        assert!(!old_left);
        assert!(saved);
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

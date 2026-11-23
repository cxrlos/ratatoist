//! The release notice: the check as an effect and its answer as an event,
//! the notice on the tab row and in the cheatsheet, the palette actions,
//! the runtime asking a fake GitHub, and the source itself.

#![cfg(test)]

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{DateTime, Local, TimeDelta, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures_util::stream;
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, ReleaseState, State};
use ratatoist::release::{
    self, Channel, Record, ReleaseCache, ReleaseCheck, ReleaseSource, release_url,
};
use ratatoist::runtime;
use ratatoist::todoist::sync::SyncResponse;
use ratatoist::todoist::{FakeClient, demo};
use ratatui::backend::TestBackend;
use semver::Version;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const LATEST_PATH: &str = "/repos/cxrlos/ratatoist/releases/latest";
const NOTICE: &str = "v99.0.0 available";
/// A local port nothing listens on, so connecting is refused at once. A
/// dropped `MockServer` would not do: wiremock pools them, and the next
/// test's server can answer on the same port.
const OFFLINE: &str = "http://127.0.0.1:1";

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

/// A release newer than any ratatoist will be.
fn newer() -> Version {
    version("99.0.0")
}

fn now() -> DateTime<Local> {
    Local.from_local_datetime(&demo::NOW).earliest().unwrap()
}

fn hours_ago(hours: i64) -> DateTime<Utc> {
    (now() - TimeDelta::hours(hours)).with_timezone(&Utc)
}

fn tick() -> Event {
    Event::Tick { now: now() }
}

fn with_latest(latest: Version) -> Record {
    Record {
        checked_at: Some(hours_ago(1)),
        latest: Some(latest),
        skipped: None,
    }
}

fn demo_sync() -> Box<SyncResponse> {
    Box::new(serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap())
}

/// The demo account, synced at [`now`], checking for releases through
/// `channel` with `record` from the cache.
fn state(check: bool, channel: Channel, record: Record) -> State {
    let mut state = State::new(now());
    state.release = ReleaseState::new(check, channel, record);
    state
}

fn app_at(columns: u16, rows: u16, state: State) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), state).unwrap();
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();
    app
}

fn app(state: State) -> App<TestBackend> {
    app_at(120, 36, state)
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn enter() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

fn lines(app: &App<TestBackend>) -> Vec<String> {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn tab_row(app: &App<TestBackend>) -> String {
    lines(app).swap_remove(0)
}

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    lines(app).iter().any(|line| line.contains(text))
}

fn has_notice(app: &App<TestBackend>) -> bool {
    tab_row(app).contains("available")
}

#[test]
fn a_tick_asks_for_a_check_when_none_ran_yet() {
    let mut app = app(state(true, Channel::CargoBin, Record::default()));

    let effects = app.handle(tick()).unwrap();

    assert!(effects.contains(&Effect::CheckRelease));
}

#[test]
fn a_check_under_a_day_old_asks_for_nothing() {
    let record = Record {
        checked_at: Some(hours_ago(23)),
        ..Record::default()
    };
    let mut app = app(state(true, Channel::CargoBin, record));

    let effects = app.handle(tick()).unwrap();

    assert!(!effects.contains(&Effect::CheckRelease));
}

#[test]
fn a_check_a_day_old_is_due_again_and_asked_for_once() {
    let record = Record {
        checked_at: Some(hours_ago(24)),
        ..Record::default()
    };
    let mut app = app(state(true, Channel::CargoBin, record));

    let first = app.handle(tick()).unwrap();
    let second = app.handle(tick()).unwrap();

    assert!(first.contains(&Effect::CheckRelease));
    assert!(!second.contains(&Effect::CheckRelease));
}

#[test]
fn a_long_session_checks_again_once_the_day_has_passed() {
    let mut app = app(state(true, Channel::CargoBin, Record::default()));
    app.handle(tick()).unwrap();
    app.handle(Event::ReleaseChecked(Some(release::running())))
        .unwrap();

    let later = |hours| Event::Tick {
        now: now() + TimeDelta::hours(hours),
    };
    let before = app.handle(later(23)).unwrap();
    let after = app.handle(later(24)).unwrap();

    assert!(!before.contains(&Effect::CheckRelease));
    assert!(after.contains(&Effect::CheckRelease));
}

#[test]
fn with_the_check_off_a_tick_asks_for_nothing() {
    let mut app = app(state(false, Channel::CargoBin, Record::default()));

    let effects = app.handle(tick()).unwrap();

    assert!(!effects.contains(&Effect::CheckRelease));
}

#[test]
fn the_default_state_never_checks() {
    let mut app = app(State::new(now()));

    let effects = app.handle(tick()).unwrap();

    assert!(!effects.contains(&Effect::CheckRelease));
}

#[test]
fn a_newer_release_shows_the_notice_and_is_saved() {
    let mut app = app(state(true, Channel::CargoBin, Record::default()));
    app.handle(tick()).unwrap();

    let effects = app.handle(Event::ReleaseChecked(Some(newer()))).unwrap();

    assert!(tab_row(&app).contains(NOTICE));
    assert_eq!(
        effects,
        vec![Effect::SaveRelease(Record {
            checked_at: Some(now().with_timezone(&Utc)),
            latest: Some(newer()),
            skipped: None,
        })]
    );
}

#[test]
fn the_same_or_an_older_release_shows_nothing() {
    for latest in [release::running(), version("0.0.1")] {
        let mut app = app(state(true, Channel::CargoBin, Record::default()));
        app.handle(tick()).unwrap();

        app.handle(Event::ReleaseChecked(Some(latest))).unwrap();

        assert!(!has_notice(&app));
    }
}

#[test]
fn a_cached_newer_release_shows_at_startup_without_a_check() {
    let mut app = app(state(true, Channel::CargoBin, with_latest(newer())));

    assert!(tab_row(&app).contains(NOTICE));
    let effects = app.handle(tick()).unwrap();
    assert!(!effects.contains(&Effect::CheckRelease));
}

#[test]
fn a_skipped_release_shows_nothing_until_a_newer_one() {
    let record = Record {
        skipped: Some(newer()),
        ..with_latest(newer())
    };
    let mut app = app(state(true, Channel::CargoBin, record));
    assert!(!has_notice(&app));

    app.handle(Event::ReleaseChecked(Some(version("99.0.1"))))
        .unwrap();

    assert!(tab_row(&app).contains("v99.0.1 available"));
}

#[test]
fn a_failed_check_adds_no_notice_and_no_error() {
    let mut app = app(state(true, Channel::CargoBin, Record::default()));
    app.handle(tick()).unwrap();

    let effects = app.handle(Event::ReleaseChecked(None)).unwrap();

    assert!(!has_notice(&app));
    assert!(app.state().notice.is_none());
    assert!(app.state().failures.is_empty());
    // It counts as the day's check, so the next is tried a day later.
    assert_eq!(
        effects,
        vec![Effect::SaveRelease(Record {
            checked_at: Some(now().with_timezone(&Utc)),
            ..Record::default()
        })]
    );
}

#[test]
fn aur_and_nix_installs_never_show_the_notice() {
    for channel in [Channel::Aur, Channel::Nix] {
        let app = app(state(true, channel, with_latest(newer())));

        assert!(!has_notice(&app));
    }
}

/// The tab row at `columns` x `rows`, synced 4 seconds ago, with or
/// without a newer release in the cache.
fn synced_tab_row(columns: u16, rows: u16, notice: bool) -> String {
    let record = if notice {
        with_latest(newer())
    } else {
        Record::default()
    };
    let mut app = app_at(columns, rows, state(false, Channel::CargoBin, record));
    app.handle(Event::Tick {
        now: now() + TimeDelta::seconds(4),
    })
    .unwrap();
    tab_row(&app)
}

#[test]
fn the_tab_row_with_the_notice_at_120x36() {
    assert_snapshot!(synced_tab_row(120, 36, true));
}

#[test]
fn the_tab_row_without_the_notice_at_120x36() {
    assert_snapshot!(synced_tab_row(120, 36, false));
}

#[test]
fn the_tab_row_with_the_notice_at_80x24() {
    assert_snapshot!(synced_tab_row(80, 24, true));
}

#[test]
fn the_tab_row_without_the_notice_at_80x24() {
    assert_snapshot!(synced_tab_row(80, 24, false));
}

#[test]
fn at_80_columns_the_notice_goes_before_the_sync_state() {
    let row = synced_tab_row(80, 24, true);

    assert!(!row.contains("available"));
    assert!(row.contains("synced"));
}

#[test]
fn the_cheatsheet_line_says_how_to_upgrade_a_homebrew_install() {
    let mut app = app_at(
        120,
        48,
        state(true, Channel::Homebrew, with_latest(newer())),
    );

    app.handle(key('?')).unwrap();

    assert!(shows(
        &app,
        "ratatoist v99.0.0 is available: brew upgrade ratatoist"
    ));
}

#[test]
fn the_cheatsheet_line_points_a_cargo_install_at_the_readme() {
    let mut app = app_at(
        120,
        48,
        state(true, Channel::CargoBin, with_latest(newer())),
    );

    app.handle(key('?')).unwrap();

    assert!(shows(
        &app,
        "ratatoist v99.0.0 is available: see https://github.com/cxrlos/ratatoist#quick-start"
    ));
}

/// Opens the palette and types `query`.
fn find(app: &mut App<TestBackend>, query: &str) {
    app.handle(key(':')).unwrap();
    for c in query.chars() {
        app.handle(key(c)).unwrap();
    }
}

#[test]
fn without_a_newer_release_the_palette_actions_cannot_run() {
    let mut app = app(state(true, Channel::CargoBin, Record::default()));
    find(&mut app, "skip this release");

    let effects = app.handle(enter()).unwrap();

    assert_eq!(effects, vec![]);
}

#[test]
fn skipping_the_release_hides_the_notice_and_records_it() {
    let mut app = app(state(true, Channel::CargoBin, with_latest(newer())));
    find(&mut app, "skip this release");

    let effects = app.handle(enter()).unwrap();

    assert!(!has_notice(&app));
    assert_eq!(
        effects,
        vec![Effect::SaveRelease(Record {
            skipped: Some(newer()),
            ..with_latest(newer())
        })]
    );
}

#[test]
fn whats_new_names_the_release_and_opens_its_page() {
    let mut app = app(state(true, Channel::CargoBin, with_latest(newer())));
    find(&mut app, "what's new");
    assert!(shows(&app, "What's new in v99.0.0"));

    let effects = app.handle(enter()).unwrap();

    assert_eq!(effects, vec![Effect::OpenUrl(release_url(&newer()))]);
    assert_eq!(
        release_url(&newer()),
        "https://github.com/cxrlos/ratatoist/releases/tag/v99.0.0"
    );
}

#[test]
fn install_channels_come_from_the_binary_path() {
    let cases = [
        (
            "/opt/homebrew/Cellar/ratatoist/0.7.0/bin/ratatoist",
            Channel::Homebrew,
        ),
        (
            "/home/linuxbrew/.linuxbrew/Cellar/ratatoist/0.7.0/bin/ratatoist",
            Channel::Homebrew,
        ),
        ("/home/someone/.cargo/bin/ratatoist", Channel::CargoBin),
        ("/usr/bin/ratatoist", Channel::Aur),
        (
            "/nix/store/abc123-ratatoist-0.7.0/bin/ratatoist",
            Channel::Nix,
        ),
    ];
    for (binary, channel) in cases {
        assert_eq!(Channel::of(Path::new(binary)), channel);
    }
}

#[test]
fn only_homebrew_and_cargo_installs_get_an_upgrade_hint() {
    assert_eq!(Channel::Homebrew.upgrade(), Some("brew upgrade ratatoist"));
    assert_eq!(
        Channel::CargoBin.upgrade(),
        Some("see https://github.com/cxrlos/ratatoist#quick-start")
    );
    assert_eq!(Channel::Aur.upgrade(), None);
    assert_eq!(Channel::Nix.upgrade(), None);
}

/// A release cache in a fresh directory for one test, removed with it.
struct ScratchCache(ReleaseCache);

impl ScratchCache {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "release-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        Self(ReleaseCache::new(dir))
    }
}

impl Deref for ScratchCache {
    type Target = ReleaseCache;

    fn deref(&self) -> &ReleaseCache {
        &self.0
    }
}

impl Drop for ScratchCache {
    fn drop(&mut self) {
        if let Some(dir) = self.0.path().parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// A fake GitHub answering `response` to `expected` requests for the
/// latest release.
async fn github(response: ResponseTemplate, expected: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LATEST_PATH))
        .respond_with(response)
        .expect(expected)
        .mount(&server)
        .await;
    server
}

fn tagged(tag: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "tag_name": tag, "draft": false }))
}

fn source(base: &str) -> ReleaseSource {
    ReleaseSource::new(format!("{base}{LATEST_PATH}")).unwrap()
}

#[tokio::test]
async fn the_source_reads_the_latest_tag_with_a_plain_user_agent_and_no_token() {
    let server = github(tagged("v0.3.1"), 1).await;

    let latest = source(&server.uri()).latest().await.unwrap();

    assert_eq!(latest, version("0.3.1"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    for request in requests {
        assert!(!request.headers.contains_key("authorization"));
        assert!(!request.headers.contains_key("cookie"));
        let agent = request.headers.get("user-agent").unwrap();
        assert_eq!(
            agent.to_str().unwrap(),
            concat!("ratatoist/", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[tokio::test]
async fn a_pre_release_tag_is_never_a_newer_version() {
    let server = github(tagged("v99.0.0-rc.1"), 1).await;

    assert!(source(&server.uri()).latest().await.is_err());
}

#[tokio::test]
async fn a_malformed_answer_is_an_error() {
    let server = github(ResponseTemplate::new(200).set_body_string("<html>"), 1).await;

    assert!(source(&server.uri()).latest().await.is_err());
}

#[tokio::test]
async fn a_rate_limited_answer_is_an_error() {
    let limited = ResponseTemplate::new(403).insert_header("x-ratelimit-remaining", "0");
    let server = github(limited, 1).await;

    let error = source(&server.uri()).latest().await.unwrap_err();

    assert_eq!(format!("{error:#}"), "GitHub answered 403 Forbidden");
}

#[tokio::test]
async fn being_offline_is_an_error() {
    let error = source(OFFLINE).latest().await.unwrap_err();

    assert!(format!("{error:#}").starts_with("could not reach GitHub"));
}

/// Runs the TUI with the check on and `record` in the cache against a
/// GitHub at `base`, until it settles.
async fn run_checking(base: &str, record: &Record) -> (App<TestBackend>, ScratchCache) {
    let cache = ScratchCache::new();
    cache.save(record).unwrap();
    let check = ReleaseCheck {
        source: source(base),
        cache: cache.0.clone(),
    };
    let mut state = State::new(now());
    state.release = ReleaseState::new(true, Channel::CargoBin, cache.load());
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    runtime::run_checking(
        &mut app,
        &FakeClient::new(),
        stream::pending(),
        tokio::time::sleep(Duration::from_millis(500)),
        None,
        Some(&check),
    )
    .await
    .unwrap();
    (app, cache)
}

/// Midnight today on the runtime's clock: a check under a day old for a
/// session running now.
fn earlier_today() -> DateTime<Utc> {
    let midnight = runtime::today().and_hms_opt(0, 0, 0).unwrap();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .unwrap()
        .with_timezone(&Utc)
}

#[tokio::test]
async fn a_newer_release_on_github_shows_and_is_cached() {
    let server = github(tagged("v99.0.0"), 1).await;

    let (app, cache) = run_checking(&server.uri(), &Record::default()).await;

    assert!(tab_row(&app).contains(NOTICE));
    let saved = cache.load();
    assert_eq!(saved.latest, Some(newer()));
    assert!(saved.checked_at.is_some());
}

#[tokio::test]
async fn the_same_or_an_older_release_on_github_shows_nothing() {
    for tag in [format!("v{}", release::running()), "v0.0.1".to_owned()] {
        let server = github(tagged(&tag), 1).await;

        let (app, cache) = run_checking(&server.uri(), &Record::default()).await;

        assert!(!has_notice(&app));
        assert!(cache.load().checked_at.is_some());
    }
}

#[tokio::test]
async fn a_fresh_cache_asks_github_nothing() {
    let server = github(tagged("v99.0.0"), 0).await;
    let record = Record {
        checked_at: Some(earlier_today()),
        latest: Some(release::running()),
        skipped: None,
    };

    let (app, cache) = run_checking(&server.uri(), &record).await;

    assert!(!has_notice(&app));
    assert_eq!(cache.load(), record);
}

#[tokio::test]
async fn a_stale_cache_asks_github_again() {
    let server = github(tagged("v99.0.0"), 1).await;
    let record = Record {
        checked_at: Some(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
        latest: Some(release::running()),
        skipped: None,
    };

    let (app, cache) = run_checking(&server.uri(), &record).await;

    assert!(tab_row(&app).contains(NOTICE));
    assert_eq!(cache.load().latest, Some(newer()));
}

#[tokio::test]
async fn a_skipped_release_stays_hidden_after_a_check() {
    let server = github(tagged("v99.0.0"), 1).await;
    let record = Record {
        checked_at: None,
        latest: Some(newer()),
        skipped: Some(newer()),
    };

    let (app, cache) = run_checking(&server.uri(), &record).await;

    assert!(!has_notice(&app));
    assert_eq!(cache.load().skipped, Some(newer()));
}

#[tokio::test]
async fn failed_checks_show_nothing_and_wait_a_day() {
    let rate_limited = github(ResponseTemplate::new(429), 1).await;
    let malformed = github(ResponseTemplate::new(200).set_body_string("{"), 1).await;
    let pre_release = github(tagged("v99.0.0-beta.1"), 1).await;
    for base in [
        OFFLINE.to_owned(),
        rate_limited.uri(),
        malformed.uri(),
        pre_release.uri(),
    ] {
        let (app, cache) = run_checking(&base, &Record::default()).await;

        assert!(!has_notice(&app));
        assert!(app.state().failures.is_empty());
        assert!(app.state().notice.is_none());
        let saved = cache.load();
        assert!(saved.checked_at.is_some());
        assert_eq!(saved.latest, None);
    }
}

#[test]
fn the_check_is_on_unless_config_toml_turns_it_off() {
    use ratatoist::config;

    assert!(config::parse("", &[]).updates.check);
    assert!(
        config::parse("[updates]\ncheck = true\n", &[])
            .updates
            .check
    );
    let off = config::parse("[updates]\ncheck = false\n", &[]);
    assert!(!off.updates.check);
    assert!(off.warnings.is_empty());
}

#[test]
fn a_bad_updates_value_warns_and_keeps_the_check_on() {
    use ratatoist::config;

    let config = config::parse("[updates]\ncheck = \"no\"\n", &[]);

    assert!(config.updates.check);
    assert_eq!(
        config.warnings,
        vec![
            "config.toml:2:9: `updates.check` must be true or false; [updates] uses its defaults"
                .to_owned()
        ]
    );
}

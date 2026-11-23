//! The frame: the tab row with the sync indicator, the body of each screen,
//! the loading line and the error screens, driven through the app.

#![cfg(test)]

use chrono::{DateTime, Duration, Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, LoadError, Modal, Pane, Screen, State};
use ratatoist::auth::Source;
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::sync::SyncResponse;
use ratatoist::todoist::{ClientError, LiveMessage, demo};
use ratatui::backend::TestBackend;

fn now() -> DateTime<Local> {
    Local.from_local_datetime(&demo::NOW).earliest().unwrap()
}

fn demo_sync() -> Box<SyncResponse> {
    Box::new(serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap())
}

/// The app at `columns` x `rows` before anything answered.
fn started(columns: u16, rows: u16) -> App<TestBackend> {
    App::new(TestBackend::new(columns, rows), State::new(now())).unwrap()
}

/// The app at `columns` x `rows` on the demo account.
fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = started(columns, rows);
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();
    app
}

fn app() -> App<TestBackend> {
    sized(120, 36)
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn tick(seconds: i64) -> Event {
    Event::Tick {
        now: now() + Duration::seconds(seconds),
    }
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
    events
        .into_iter()
        .flat_map(|event| app.handle(event).unwrap())
        .collect()
}

fn rows(app: &App<TestBackend>) -> Vec<String> {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn tab_row(app: &App<TestBackend>) -> String {
    rows(app).swap_remove(0)
}

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    rows(app).iter().any(|row| row.contains(text))
}

#[test]
fn the_tab_row_names_every_screen_with_its_key() {
    let app = app();

    let tabs = tab_row(&app);

    assert!(
        tabs.contains("[1] Inbox │ [2] Today │ [3] Upcoming │ [4] Projects │ [5] Labels"),
        "{tabs}"
    );
}

#[test]
fn number_keys_switch_screens() {
    let mut app = app();
    assert_eq!(app.state().screen, Screen::Today);

    for (c, screen) in [
        ('1', Screen::Inbox),
        ('3', Screen::Upcoming),
        ('4', Screen::Projects),
        ('5', Screen::Labels),
        ('2', Screen::Today),
    ] {
        send(&mut app, [key(c)]);
        assert_eq!(app.state().screen, screen);
    }
}

#[test]
fn the_tab_row_follows_remapped_keys() {
    let config = ratatoist::config::parse("[keys.global]\nshow-upcoming = \"U\"\n", &[]);
    assert_eq!(config.warnings, Vec::<String>::new());
    let state = initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    );
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();

    assert!(tab_row(&app).contains("[U] Upcoming"));
    send(&mut app, [key('U')]);
    assert_eq!(app.state().screen, Screen::Upcoming);
}

#[test]
fn the_tab_row_shrinks_to_the_keys_at_80_columns() {
    let app = sized(80, 24);

    let tabs = tab_row(&app);

    assert!(tabs.contains("[1] │ [2] Today │ [3] │ [4] │ [5]"), "{tabs}");
}

#[test]
fn the_sync_indicator_follows_sync_events() {
    let mut states = Vec::new();
    let mut app = app();
    send(&mut app, [tick(4)]);
    states.push(tab_row(&app));

    send(
        &mut app,
        [Event::Synced(Err(ClientError::Network(
            "connection refused".to_owned(),
        )))],
    );
    states.push(tab_row(&app));

    send(
        &mut app,
        [Event::Synced(Err(ClientError::Http {
            status: 500,
            message: "internal error".to_owned(),
        }))],
    );
    states.push(tab_row(&app));

    let mut app = sized(120, 36);
    send(&mut app, [Event::Live(LiveMessage::Disconnected)]);
    states.push(tab_row(&app));
    send(&mut app, [Event::Live(LiveMessage::Connected)]);
    states.push(tab_row(&app));

    let mut app = sized(120, 36);
    send(&mut app, [tick(10 * 60)]);
    states.push(tab_row(&app));

    assert_snapshot!(states.join("\n"));
}

#[test]
fn the_first_frame_shows_the_loading_line_and_keys_work() {
    let mut app = started(80, 24);
    assert!(shows(&app, "Loading your tasks from Todoist…"));

    send(&mut app, [key('?')]);
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Cheatsheet { .. })
    ));
    send(&mut app, [press(KeyCode::Esc)]);
    assert_eq!(app.state().top_modal(), None);

    let effects = send(&mut app, [key('q')]);
    assert!(effects.contains(&Effect::Quit));
}

#[test]
fn loading_at_80x24() {
    let app = started(80, 24);

    assert_snapshot!(app.backend());
}

#[test]
fn not_logged_in_at_80x24() {
    let mut state = State::new(now());
    state.not_loaded(LoadError::NoToken);
    let mut app = App::new(TestBackend::new(80, 24), state).unwrap();

    assert!(send(&mut app, [Event::Start]).is_empty());
    assert_snapshot!(app.backend());
}

#[test]
fn not_logged_in_at_120x36() {
    let mut state = State::new(now());
    state.not_loaded(LoadError::NoToken);
    let app = App::new(TestBackend::new(120, 36), state).unwrap();

    assert_snapshot!(app.backend());
}

#[test]
fn token_rejected_at_80x24() {
    let mut state = State::new(now());
    state.token_source = Some(Source::Keyring);
    let mut app = App::new(TestBackend::new(80, 24), state).unwrap();

    send(&mut app, [Event::Synced(Err(ClientError::Unauthorized))]);

    assert_eq!(app.state().load_error, Some(LoadError::TokenRejected));
    assert_snapshot!(app.backend());
}

#[test]
fn the_error_screens_keep_the_bar_live() {
    let mut state = State::new(now());
    state.not_loaded(LoadError::NoToken);
    let mut app = App::new(TestBackend::new(80, 24), state).unwrap();

    send(&mut app, [key('?')]);
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Cheatsheet { .. })
    ));
}

#[test]
fn upcoming_groups_the_next_seven_days_by_day() {
    let mut app = app();

    send(&mut app, [key('3')]);

    assert!(shows(&app, "Mon 23 Nov ("));
    assert_snapshot!(app.backend());
}

#[test]
fn projects_enter_opens_the_project_and_esc_returns_to_the_same_row() {
    let mut app = app();
    send(&mut app, [key('4'), key('j'), key('j')]);
    let focused = app.state().selected_project;

    send(&mut app, [press(KeyCode::Enter)]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    let name = app.state().projects[focused].name.clone();
    assert!(rows(&app)[1].contains(&name));

    send(&mut app, [press(KeyCode::Esc)]);
    assert_eq!(app.state().active_pane, Pane::Projects);
    assert_eq!(app.state().selected_project, focused);
}

#[test]
fn the_projects_tree_has_no_today_entry() {
    let mut app = app();

    send(&mut app, [key('4')]);

    let body: Vec<String> = rows(&app)[1..].to_vec();
    assert!(!body.iter().any(|row| row.contains("Today")), "{body:#?}");
}

#[test]
fn enter_in_the_inbox_opens_the_detail_and_esc_goes_back() {
    let mut app = app();
    send(&mut app, [key('1'), press(KeyCode::Enter)]);
    assert_eq!(app.state().active_pane, Pane::Detail);

    send(&mut app, [press(KeyCode::Esc)]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    assert_eq!(app.state().screen, Screen::Inbox);
}

fn screen_at(screen: char, columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = sized(columns, rows);
    send(&mut app, [tick(4), key(screen)]);
    app
}

#[test]
fn inbox_at_120x36() {
    assert_snapshot!(screen_at('1', 120, 36).backend());
}

#[test]
fn inbox_at_80x24() {
    assert_snapshot!(screen_at('1', 80, 24).backend());
}

#[test]
fn today_at_120x36() {
    assert_snapshot!(screen_at('2', 120, 36).backend());
}

#[test]
fn today_at_80x24() {
    assert_snapshot!(screen_at('2', 80, 24).backend());
}

#[test]
fn upcoming_at_80x24() {
    assert_snapshot!(screen_at('3', 80, 24).backend());
}

#[test]
fn projects_at_120x36() {
    assert_snapshot!(screen_at('4', 120, 36).backend());
}

#[test]
fn projects_at_80x24() {
    assert_snapshot!(screen_at('4', 80, 24).backend());
}

#[test]
fn a_project_s_tasks_at_80x24() {
    let mut app = screen_at('4', 80, 24);
    send(&mut app, [key('j'), press(KeyCode::Enter)]);

    assert_snapshot!(app.backend());
}

#[test]
fn labels_at_120x36() {
    assert_snapshot!(screen_at('5', 120, 36).backend());
}

#[test]
fn labels_at_80x24() {
    assert_snapshot!(screen_at('5', 80, 24).backend());
}

#[test]
fn a_task_s_detail_at_80x24() {
    let mut app = screen_at('2', 80, 24);
    send(&mut app, [press(KeyCode::Enter)]);

    assert_snapshot!(app.backend());
}

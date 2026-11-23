//! The which-key popup: after the first key of a chord, the keys that can
//! follow, bottom-right of the body.

#![cfg(test)]

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Event, Modal, State};
use ratatoist::keys::Key;
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
}

/// The Inbox's task list at `columns` x `rows`.
fn list(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
    let sync: Box<SyncResponse> = Box::new(serde_json::from_str(FULL_SYNC).unwrap());
    app.handle(Event::Synced(Ok(sync))).unwrap();
    send(&mut app, [key('1')]);
    app
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) {
    for event in events {
        app.handle(event).unwrap();
    }
}

fn screen(app: &App<TestBackend>) -> Vec<String> {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn text(app: &App<TestBackend>) -> String {
    screen(app).join("\n")
}

#[test]
fn after_z_the_popup_lists_the_folds() {
    let mut app = list(120, 36);

    send(&mut app, [key('z')]);

    assert_eq!(
        app.state().modals,
        [Modal::WhichKey {
            prefix: vec![Key::char('z')]
        }]
    );
    let text = text(&app);
    for line in [
        " a  Fold or unfold the focused row ",
        " R  Unfold every row ",
        " M  Fold every row ",
    ] {
        assert!(text.contains(line), "{line} in\n{text}");
    }
}

#[test]
fn the_popup_sits_bottom_right_of_the_body() {
    let mut app = list(120, 36);

    send(&mut app, [key('z')]);

    let rows = screen(&app);
    let bottom = rows.iter().rposition(|row| row.contains('┘')).unwrap();
    let bar_top = usize::from(36 - ratatoist::ui::hints::ROWS);
    assert_eq!(bottom, bar_top - 2, "inside the body's bottom border");
    assert!(rows[bottom].ends_with("┘│"), "{}", rows[bottom]);
}

#[test]
fn esc_cancels_the_chord() {
    let mut app = list(120, 36);
    send(&mut app, [key('z')]);

    send(&mut app, [press(KeyCode::Esc)]);

    assert!(app.state().modals.is_empty());
    assert!(!text(&app).contains("Unfold every row"));
}

#[test]
fn an_unknown_key_cancels_the_chord() {
    let mut app = list(120, 36);
    send(&mut app, [key('z')]);

    send(&mut app, [key('q')]);

    assert!(app.state().modals.is_empty());
    assert!(!text(&app).contains("Unfold every row"));
}

#[test]
fn which_key_after_z_at_120x36() {
    let mut app = list(120, 36);
    send(&mut app, [key('z')]);

    assert_snapshot!(text(&app));
}

#[test]
fn which_key_after_z_at_80x24() {
    let mut app = list(80, 24);
    send(&mut app, [key('z')]);

    assert_snapshot!(text(&app));
}

/// The foreground of the key in the popup's line for `description`.
fn key_fg(app: &App<TestBackend>, description: &str) -> Option<Color> {
    let rows = screen(app);
    let (y, line) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains(description))?;
    let x = line[..line.find(description)?].chars().count() - 3;
    let cell = app
        .backend()
        .buffer()
        .cell((u16::try_from(x).ok()?, u16::try_from(y).ok()?))?;
    Some(cell.fg)
}

#[test]
fn a_continuation_that_cannot_run_now_is_muted() {
    let config = ratatoist::config::parse("[keys.list]\ntoggle-complete = \"gx\"\n", &[]);
    assert_eq!(config.warnings, Vec::<String>::new());
    let mut state = list(120, 36).state().clone();
    state.keymap = initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    )
    .keymap;
    let with_tasks = state.clone();
    state.tasks.clear();
    let complete = "Complete the task, or reopen it";

    let mut empty = App::new(TestBackend::new(120, 36), state).unwrap();
    send(&mut empty, [key('g')]);
    assert_eq!(
        key_fg(&empty, complete),
        Some(empty.state().theme().muted),
        "no task to complete"
    );

    let mut app = App::new(TestBackend::new(120, 36), with_tasks).unwrap();
    send(&mut app, [key('g')]);
    assert_eq!(key_fg(&app, complete), Some(app.state().theme().accent));
}

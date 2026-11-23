//! `NO_COLOR`: when set and not empty, ratatoist draws with no colour at
//! all, and modifiers alone carry focus and emphasis.

// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::ffi::OsStr;

use chrono::{Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatoist::app::{App, Event};
use ratatoist::config::Config;
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::demo;
use ratatoist::ui::theme::{Theme, no_color_requested};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// The demo account with `[theme] name = "nord"` and `NO_COLOR` set.
fn no_color_demo() -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let config = Config {
        theme: Some("Nord".to_owned()),
        ..Config::default()
    };
    let state = initial_state_from(&config, &UiSettings::default(), "*".to_owned(), now, true);
    let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
    let sync = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    app
}

fn press(app: &mut App<TestBackend>, code: KeyCode) {
    app.handle(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}

fn assert_colourless(buffer: &Buffer) {
    for cell in buffer.content() {
        assert!(
            cell.fg == Color::Reset && cell.bg == Color::Reset,
            "a cell has a colour"
        );
    }
}

/// The row whose text contains `text`, and whether every cell from `from`
/// to `to` on it is reversed.
fn reversed(buffer: &Buffer, text: &str, from: u16, to: u16) -> bool {
    let width = usize::from(buffer.area.width);
    let y = buffer
        .content()
        .chunks(width)
        .position(|row| {
            row.iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .contains(text)
        })
        .unwrap();
    let y = u16::try_from(y).unwrap();
    (from..to).all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
}

#[test]
fn no_color_counts_only_when_set_and_not_empty() {
    assert!(!no_color_requested(None));
    assert!(!no_color_requested(Some(OsStr::new(""))));
    assert!(no_color_requested(Some(OsStr::new("1"))));
}

#[test]
fn the_no_color_theme_resets_every_role() {
    let theme = Theme::no_color();

    for colour in [
        theme.background,
        theme.text,
        theme.muted,
        theme.accent,
        theme.error,
        theme.border,
        theme.priorities.p1,
        theme.priorities.p4,
        theme.due.overdue,
        theme.due.later,
        theme.color_for("berry_red"),
    ] {
        assert_eq!(colour, Color::Reset);
    }
    assert_eq!(theme.selection.add_modifier, Modifier::REVERSED);
}

#[test]
fn the_main_screen_draws_no_colour_and_reverses_the_focused_row() {
    let mut app = no_color_demo();
    press(&mut app, KeyCode::Char('4'));
    let buffer = app.backend().buffer();

    assert_colourless(buffer);
    assert!(reversed(buffer, "Inbox    ", 3, 20));
    assert!(!reversed(buffer, "Reading", 3, 20));
}

#[test]
fn the_focused_task_row_stays_reversed_in_the_task_list() {
    let mut app = no_color_demo();
    press(&mut app, KeyCode::Char('1'));
    let buffer = app.backend().buffer();

    assert_colourless(buffer);
    assert!(reversed(buffer, "Call the plumber", 26, 70));
    assert!(!reversed(buffer, "Return the library", 26, 70));
}

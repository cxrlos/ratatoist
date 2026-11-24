#![cfg(test)]

use insta::assert_snapshot;
use jiff::tz::TimeZone;
use ratatoist::app::{App, Event, State};
use ratatoist::todoist::demo;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

/// The demo account at 80x24 on the Inbox screen, with `edit` applied to
/// the account before it loads.
fn demo_inbox(edit: impl FnOnce(&mut serde_json::Value)) -> App<TestBackend> {
    let now = demo::NOW.to_zoned(TimeZone::UTC).unwrap();
    let mut app = App::new(TestBackend::new(80, 24), State::new(now)).unwrap();
    let mut sync = demo::account(&demo::NOW.to_zoned(TimeZone::UTC).unwrap()).sync;
    edit(&mut sync);
    let sync = serde_json::from_value(sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    let one = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('1'));
    app.handle(Event::Key(one)).unwrap();
    app
}

/// Renames the demo task `id` to `content`.
fn rename(sync: &mut serde_json::Value, id: &str, content: &str) {
    let task = sync["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|task| task["id"] == id)
        .unwrap();
    task["content"] = content.into();
}

/// The index of the row of `buffer` that holds `needle`.
fn row_with(buffer: &Buffer, needle: &str) -> u16 {
    (0..buffer.area.height)
        .find(|&y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .contains(needle)
        })
        .unwrap()
}

/// The column of the priority label (`P1` to `P4`) in row `y`.
fn priority_column(buffer: &Buffer, y: u16) -> Option<u16> {
    (1..buffer.area.width.saturating_sub(1)).rev().find(|&x| {
        buffer[(x - 1, y)].symbol() == " "
            && buffer[(x, y)].symbol() == "P"
            && buffer[(x + 1, y)].symbol().parse::<u8>().is_ok()
    })
}

/// The row holding `needle` is cut with `…` before its priority, which
/// lines up with every other task row's.
fn assert_cut_at_the_pane_edge(app: &App<TestBackend>, needle: &str) {
    let buffer = app.backend().buffer();
    let y = row_with(buffer, needle);
    let row: String = (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect();
    assert!(row.contains('…'));
    let other = row_with(buffer, "P4");
    assert!(priority_column(buffer, y).is_some());
    assert_eq!(priority_column(buffer, y), priority_column(buffer, other));
}

#[test]
fn a_cjk_task_row_ends_in_an_ellipsis_at_the_pane_edge() {
    let app = demo_inbox(|sync| {
        rename(
            sync,
            "t-shopping",
            "週末3日分の買い物リストを作って、冷蔵庫のドアに貼っておく",
        )
    });

    assert_cut_at_the_pane_edge(&app, "週");
    assert_snapshot!(app.backend());
}

#[test]
fn an_emoji_task_row_ends_in_an_ellipsis_at_the_pane_edge() {
    let app = demo_inbox(|sync| {
        rename(
            sync,
            "t-cake",
            "🎂 Order a cake for Saturday, and balloons 🎈🎈🎈🎈🎈🎈🎈🎈🎈🎈🎈🎈🎈🎈",
        )
    });

    assert_cut_at_the_pane_edge(&app, "🎂");
    assert_snapshot!(app.backend());
}

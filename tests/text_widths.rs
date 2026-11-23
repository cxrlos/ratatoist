#![cfg(test)]

use chrono::{Local, TimeZone};
use insta::assert_snapshot;
use ratatoist::app::{App, Event, State};
use ratatoist::todoist::demo;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

/// The demo account at 80x24 on the inbox, the first screen after the
/// first sync, with `edit` applied to the account before it loads.
fn demo_inbox(edit: impl FnOnce(&mut serde_json::Value)) -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let mut app = App::new(TestBackend::new(80, 24), State::new(now)).unwrap();
    let mut sync = demo::account(demo::NOW.date()).sync;
    edit(&mut sync);
    let sync = serde_json::from_value(sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
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

/// The row of `buffer` that holds `needle`, as the symbols of its cells.
fn row_with(buffer: &Buffer, needle: &str) -> (u16, String) {
    (0..buffer.area.height)
        .map(|y| {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            (y, row)
        })
        .find(|(_, row)| row.contains(needle))
        .unwrap()
}

/// The last column text may take in the tasks pane at 80 columns: the
/// pane's right border is at 79 and its padding at 78.
const PANE_EDGE: u16 = 77;

fn assert_cut_at_the_pane_edge(app: &App<TestBackend>, needle: &str) {
    let buffer = app.backend().buffer();
    let (y, row) = row_with(buffer, needle);
    assert_eq!(buffer[(PANE_EDGE, y)].symbol(), "…", "row {y}: {row}");
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
            "🎂 Order a cake for Saturday, and balloons 🎈🎈🎈🎈",
        )
    });

    assert_cut_at_the_pane_edge(&app, "🎂");
    assert_snapshot!(app.backend());
}

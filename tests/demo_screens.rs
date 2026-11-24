#![cfg(test)]

use chrono::{Local, TimeZone};
use insta::assert_snapshot;
use ratatoist::app::{App, Event, Screen, State};
use ratatoist::todoist::demo;
use ratatui::backend::TestBackend;

#[test]
fn demo_today_at_80x24() {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let mut app = App::new(TestBackend::new(80, 24), State::new(now)).unwrap();
    let sync = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();

    assert_eq!(app.state().screen, Screen::Today);
    assert_snapshot!(app.backend());
}

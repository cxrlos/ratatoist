#![cfg(test)]

use insta::assert_snapshot;
use jiff::tz::TimeZone;
use ratatoist::app::{App, Event, Screen, State};
use ratatoist::todoist::demo;
use ratatui::backend::TestBackend;

#[test]
fn demo_today_at_80x24() {
    let now = demo::NOW.to_zoned(TimeZone::UTC).unwrap();
    let mut app = App::new(TestBackend::new(80, 24), State::new(now)).unwrap();
    let sync =
        serde_json::from_value(demo::account(&demo::NOW.to_zoned(TimeZone::UTC).unwrap()).sync)
            .unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();

    assert_eq!(app.state().screen, Screen::Today);
    assert_snapshot!(app.backend());
}

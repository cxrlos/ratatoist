#![cfg(test)]

//! Due dates read, count and filter the same way whether they carry a time
//! or not, and every count matches the rows its list shows.

use insta::assert_snapshot;
use jiff::Zoned;
use jiff::civil::date;
use jiff::tz::{self, TimeZone};
use ratatoist::app::{App, Event, ListFilter, OverviewStats, Screen, State, update};
use ratatoist::todoist::model::Priority;
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;

/// 2026-11-22 10:00 local time, a Sunday.
fn now() -> Zoned {
    date(2026, 11, 22)
        .at(10, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
}

/// A hypothetical full sync, not recorded from Todoist. One inbox holds a
/// due of every kind: timed and all-day, on each side of today and of the
/// week's end, checked, a subtask, and one assigned to someone else.
const HYPOTHETICAL_SYNC: &str = r#"{
  "full_sync": true,
  "sync_token": "due-dates-1",
  "user": { "id": "u1", "full_name": "Ada Example", "email": "ada@example.com" },
  "collaborators": [{ "id": "u2", "full_name": "Grace Example" }],
  "projects": [
    { "id": "p-inbox", "name": "Inbox", "color": "charcoal", "child_order": 0,
      "is_shared": true, "is_favorite": false, "inbox_project": true }
  ],
  "items": [
    { "id": "t-standup", "content": "Write the stand-up notes", "child_order": 1, "priority": 4,
      "project_id": "p-inbox", "due": { "date": "2026-11-22T09:00:00" } },
    { "id": "t-call", "content": "Return the call", "child_order": 2, "priority": 3,
      "project_id": "p-inbox", "due": { "date": "2026-11-21T18:30:00" } },
    { "id": "t-bill", "content": "Pay the bill", "child_order": 3, "priority": 2,
      "project_id": "p-inbox", "due": { "date": "2026-11-20" } },
    { "id": "t-flight", "content": "Check in for the flight", "child_order": 4, "priority": 1,
      "project_id": "p-inbox",
      "due": { "date": "2026-11-21T23:00:00Z", "timezone": "Europe/Madrid" } },
    { "id": "t-done", "content": "Sweep the floor", "child_order": 5, "priority": 1,
      "project_id": "p-inbox", "checked": true, "due": { "date": "2026-11-22" } },
    { "id": "t-review", "content": "Review the draft", "child_order": 6, "priority": 4,
      "project_id": "p-inbox", "due": { "date": "2026-11-25T08:00:00" } },
    { "id": "t-sub", "content": "Mark up chapter one", "child_order": 1, "priority": 1,
      "project_id": "p-inbox", "parent_id": "t-review", "due": { "date": "2026-11-22" } },
    { "id": "t-edge", "content": "Send the invoice", "child_order": 7, "priority": 1,
      "project_id": "p-inbox", "due": { "date": "2026-11-28" } },
    { "id": "t-later", "content": "Book the dentist", "child_order": 8, "priority": 1,
      "project_id": "p-inbox", "due": { "date": "2026-11-29T12:00:00" } },
    { "id": "t-theirs", "content": "Grace's handover", "child_order": 9, "priority": 4,
      "project_id": "p-inbox", "responsible_uid": "u2", "due": { "date": "2026-11-22" } },
    { "id": "t-checked-week", "content": "Water the plants", "child_order": 10, "priority": 2,
      "project_id": "p-inbox", "checked": true, "due": { "date": "2026-11-24" } }
  ]
}"#;

fn synced() -> State {
    synced_at(now())
}

fn synced_at(now: Zoned) -> State {
    let mut state = State::new(now);
    let response: SyncResponse = serde_json::from_str(HYPOTHETICAL_SYNC).unwrap();
    update(&mut state, Event::Synced(Ok(Box::new(response))));
    state
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

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    screen(app).iter().any(|line| line.contains(text))
}

fn count(stats: &OverviewStats, filter: ListFilter) -> u32 {
    match filter {
        ListFilter::Overdue => stats.overdue,
        ListFilter::DueToday => stats.due_today,
        ListFilter::DueThisWeek => stats.due_week,
        ListFilter::Priority(p) => stats.by_priority[usize::from(p.api())],
        ListFilter::Open | ListFilter::Completed | ListFilter::All => 0,
    }
}

/// The count for `item` and the ids of the rows its list filter
/// shows in the inbox, which holds every task.
fn count_and_rows(item: ListFilter) -> (u32, Vec<String>) {
    let mut state = synced();
    let shown = count(&state.overview_stats(), item);
    state.screen = Screen::Inbox;
    state.list_filter = item;
    let rows = state.visible_tasks().iter().map(|t| t.id.clone()).collect();
    (shown, rows)
}

fn assert_count_matches_rows(item: ListFilter, expected: &[&str]) {
    let (shown, mut rows) = count_and_rows(item);
    rows.sort();
    let mut expected: Vec<String> = expected.iter().map(|id| (*id).to_owned()).collect();
    expected.sort();
    assert_eq!(rows, expected, "{item:?} rows");
    assert_eq!(shown as usize, rows.len(), "{item:?} count");
}

#[test]
fn a_timed_task_due_earlier_today_shows_as_due_today() {
    let mut state = synced();
    // Off the first row, so the selection highlight does not hide its style.
    state.selected_task = 1;
    let app = App::new(TestBackend::new(120, 36), state).unwrap();

    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    let row = screen(&app)
        .iter()
        .position(|line| line.contains("Write the stand-up notes"))
        .unwrap();
    let cells = &buffer.content()[row * width..(row + 1) * width];
    let today = cells
        .windows(5)
        .find(|w| w.iter().map(|c| c.symbol()).collect::<String>() == "today")
        .unwrap();
    assert_eq!(today[0].fg, app.state().theme().due_today().fg.unwrap());
    assert_snapshot!(app.backend());
}

#[test]
fn the_overdue_count_matches_its_rows() {
    assert_count_matches_rows(ListFilter::Overdue, &["t-call", "t-bill", "t-flight"]);
}

#[test]
fn the_due_today_count_matches_its_rows() {
    assert_count_matches_rows(ListFilter::DueToday, &["t-standup"]);
}

#[test]
fn the_due_this_week_count_matches_its_rows() {
    assert_count_matches_rows(
        ListFilter::DueThisWeek,
        &["t-standup", "t-review", "t-edge"],
    );
}

#[test]
fn the_p1_count_matches_its_rows() {
    assert_count_matches_rows(
        ListFilter::Priority(Priority::P1),
        &["t-standup", "t-review"],
    );
}

#[test]
fn the_p2_count_matches_its_rows() {
    assert_count_matches_rows(ListFilter::Priority(Priority::P2), &["t-call"]);
}

#[test]
fn the_p3_count_matches_its_rows() {
    assert_count_matches_rows(ListFilter::Priority(Priority::P3), &["t-bill"]);
}

#[test]
fn the_p4_count_matches_its_rows() {
    assert_count_matches_rows(
        ListFilter::Priority(Priority::P4),
        &["t-flight", "t-edge", "t-later"],
    );
}

#[test]
fn the_today_counts_match_the_today_list() {
    let state = synced();

    let app = App::new(TestBackend::new(120, 36), state).unwrap();

    let rows = app.state().visible_tasks().len();
    assert_eq!(rows, 4);
    assert!(shows(&app, "Overdue (3)"));
}

/// A hypothetical delta, not recorded from Todoist: two tasks with a fixed
/// timezone, which Todoist sends as instants in UTC, one at 23:30 on
/// 2026-11-22 and one at 00:30 on 2026-11-23 in UTC-6.
fn near_midnight() -> Box<SyncResponse> {
    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "due-dates-2",
        "items": [
            { "id": "t-tonight", "content": "Submit the form tonight", "child_order": 11,
              "priority": 1, "project_id": "p-inbox",
              "due": { "date": "2026-11-23T05:30:00Z", "timezone": "America/Mexico_City" } },
            { "id": "t-after-midnight", "content": "Catch the night bus", "child_order": 12,
              "priority": 1, "project_id": "p-inbox",
              "due": { "date": "2026-11-23T06:30:00Z", "timezone": "America/Mexico_City" } }
        ]
    });
    Box::new(serde_json::from_value(delta).unwrap())
}

#[test]
fn a_fixed_timezone_task_late_tonight_shows_in_today_and_its_count() {
    // 10:00 in UTC-6, as in America/Mexico_City.
    let west = date(2026, 11, 22)
        .at(10, 0, 0, 0)
        .to_zoned(TimeZone::fixed(tz::offset(-6)))
        .unwrap();
    let mut state = synced_at(west);
    update(&mut state, Event::Synced(Ok(near_midnight())));

    state.screen = Screen::Today;

    let app = App::new(TestBackend::new(120, 36), state).unwrap();

    let today: Vec<&str> = app
        .state()
        .visible_tasks()
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert!(today.contains(&"t-tonight"), "{today:?}");
    assert!(!today.contains(&"t-after-midnight"), "{today:?}");
    assert_eq!(app.state().overview_stats().due_today, 2);
    assert!(shows(&app, "Submit the form tonight"));
    assert!(!shows(&app, "Catch the night bus"));
}

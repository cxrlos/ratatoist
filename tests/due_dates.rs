#![cfg(test)]

//! Due dates read, count and filter the same way whether they carry a time
//! or not, and every count matches the rows its list shows.

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use insta::assert_snapshot;
use ratatoist::app::{App, DockItem, Event, OverviewStats, State, update};
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;

/// 2026-11-22 10:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(10, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
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
    let mut state = State::new(now());
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

fn count(stats: &OverviewStats, item: DockItem) -> u32 {
    match item {
        DockItem::DueOverdue => stats.overdue,
        DockItem::DueToday => stats.due_today,
        DockItem::DueWeek => stats.due_week,
        DockItem::Priority(p) => stats.by_priority[usize::from(p)],
    }
}

/// The dock's count for `item` and the ids of the rows its filter shows.
fn count_and_rows(item: DockItem) -> (u32, Vec<String>) {
    let mut state = synced();
    let shown = count(&state.overview_stats(), item);
    state.dock_filter = Some(item);
    let rows = state.visible_tasks().iter().map(|t| t.id.clone()).collect();
    (shown, rows)
}

fn assert_count_matches_rows(item: DockItem, expected: &[&str]) {
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
    assert_count_matches_rows(DockItem::DueOverdue, &["t-call", "t-bill", "t-flight"]);
}

#[test]
fn the_due_today_count_matches_its_rows() {
    assert_count_matches_rows(DockItem::DueToday, &["t-standup"]);
}

#[test]
fn the_due_this_week_count_matches_its_rows() {
    assert_count_matches_rows(DockItem::DueWeek, &["t-standup", "t-review", "t-edge"]);
}

#[test]
fn the_p1_count_matches_its_rows() {
    assert_count_matches_rows(DockItem::Priority(4), &["t-standup", "t-review"]);
}

#[test]
fn the_p2_count_matches_its_rows() {
    assert_count_matches_rows(DockItem::Priority(3), &["t-call"]);
}

#[test]
fn the_p3_count_matches_its_rows() {
    assert_count_matches_rows(DockItem::Priority(2), &["t-bill"]);
}

#[test]
fn the_p4_count_matches_its_rows() {
    assert_count_matches_rows(DockItem::Priority(1), &["t-flight", "t-edge", "t-later"]);
}

#[test]
fn the_today_counts_match_the_today_list() {
    let mut state = synced();
    state.today_view_active = true;

    let app = App::new(TestBackend::new(120, 36), state).unwrap();

    let rows = app.state().visible_tasks().len();
    assert_eq!(rows, 4);
    assert!(shows(&app, "◆ Today  4"));
    assert!(shows(&app, "Overdue  (3)"));
}

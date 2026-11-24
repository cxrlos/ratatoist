//! The store behind the task lists: parents that loop back show at top
//! level and are logged once, and a frame's cost grows with the tasks, not
//! with their square.

#![cfg(test)]

use std::time::{Duration, Instant};

use jiff::Zoned;
use jiff::tz::TimeZone;
use ratatoist::app::{App, ErrorKind, Event, ListFilter, ListRow, Screen, State};
use ratatoist::todoist::model::{Project, Task};
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;

fn now() -> Zoned {
    jiff::civil::date(2026, 11, 24)
        .at(9, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
}

fn inbox() -> Project {
    Project {
        id: "inbox".to_owned(),
        name: "Inbox".to_owned(),
        inbox_project: Some(true),
        ..Project::default()
    }
}

fn project(id: &str, parent: Option<&str>) -> Project {
    Project {
        id: id.to_owned(),
        name: id.to_owned(),
        parent_id: parent.map(str::to_owned),
        ..Project::default()
    }
}

fn task(id: &str, parent: Option<&str>) -> Task {
    Task {
        id: id.to_owned(),
        content: format!("Task {id}"),
        project_id: "inbox".to_owned(),
        parent_id: parent.map(str::to_owned),
        ..Task::default()
    }
}

fn sync(full: bool, tasks: Vec<Task>, projects: Vec<Project>) -> Event {
    Event::Synced(Ok(Box::new(SyncResponse {
        items: Some(tasks),
        projects: Some(projects),
        ..SyncResponse::empty(full)
    })))
}

fn inbox_app(columns: u16, rows: u16) -> App<TestBackend> {
    let mut state = State::new(now());
    state.screen = Screen::Inbox;
    App::new(TestBackend::new(columns, rows), state).unwrap()
}

/// The list's tasks with their depth, as `id@depth`.
fn listed(app: &App<TestBackend>) -> Vec<String> {
    app.state()
        .list_rows()
        .iter()
        .filter_map(|row| match row {
            ListRow::Task { task, depth } => Some(format!("{}@{depth}", task.id)),
            ListRow::Header(_) => None,
        })
        .collect()
}

#[test]
fn tasks_whose_parents_loop_show_at_top_level_and_are_logged_once() {
    let mut app = inbox_app(80, 24);
    app.handle(sync(
        true,
        vec![
            task("a", Some("b")),
            task("b", Some("a")),
            task("c", Some("a")),
        ],
        vec![inbox()],
    ))
    .unwrap();

    assert_eq!(listed(&app), ["a@0", "c@1", "b@0"]);
    let failures = &app.state().failures;
    assert_eq!(failures.len(), 1);
    let failure = failures.newest().unwrap();
    assert_eq!(failure.kind, ErrorKind::ParentCycle);
    assert_eq!(failure.action, "show \"Task a\" under its parent task");
    assert!(!app.state().show_failure, "a cycle is logged, not a notice");

    app.handle(sync(false, vec![task("d", None)], vec![]))
        .unwrap();
    assert_eq!(app.state().failures.len(), 1, "the same cycle again");

    app.handle(sync(false, vec![task("b", None)], vec![]))
        .unwrap();
    assert_eq!(listed(&app), ["b@0", "a@1", "c@2", "d@0"]);
}

#[test]
fn projects_whose_parents_loop_show_at_top_level_and_are_logged() {
    let mut app = inbox_app(80, 24);
    app.handle(sync(
        true,
        vec![],
        vec![inbox(), project("x", Some("y")), project("y", Some("x"))],
    ))
    .unwrap();

    let state = app.state();
    assert_eq!(state.project_depth("x"), 0);
    assert_eq!(state.project_depth("y"), 0);
    assert_eq!(
        state.failures.newest().map(|failure| failure.kind.clone()),
        Some(ErrorKind::ParentCycle)
    );
}

/// `count` tasks in the Inbox: top-level tasks with a chain of three
/// subtasks each, every other one completed, so the Completed list filter
/// walks every ancestry.
fn many_tasks(count: usize) -> Vec<Task> {
    (0..count)
        .map(|n| {
            let parent = (n % 4 != 0).then(|| format!("t{}", n - 1));
            Task {
                checked: n % 2 == 1,
                child_order: i32::try_from(n).unwrap(),
                ..task(&format!("t{n}"), parent.as_deref())
            }
        })
        .collect()
}

/// The fastest of a few frames after a sync changes one task: the merge,
/// the rebuilt lists and the drawing.
fn frame_cost(count: usize) -> Duration {
    let mut app = inbox_app(160, 48);
    app.handle(sync(true, many_tasks(count), vec![inbox()]))
        .unwrap();
    let mut state = app.state().clone();
    state.list_filter = ListFilter::Completed;
    let mut app = App::new(TestBackend::new(160, 48), state).unwrap();
    (0..5)
        .map(|round| {
            let changed = Task {
                content: format!("renamed {round}"),
                ..task("t0", None)
            };
            let started = Instant::now();
            app.handle(sync(false, vec![changed], vec![])).unwrap();
            started.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn a_frame_costs_about_linear_time_in_the_tasks_up_to_5000() {
    let small = frame_cost(1250);
    let large = frame_cost(5000);
    let ratio = large.as_secs_f64() / small.as_secs_f64();

    // Four times the tasks: linear work costs about four times as much,
    // quadratic work sixteen times.
    assert!(
        ratio < 9.0,
        "1250 tasks: {small:?}, 5000 tasks: {large:?}, ratio {ratio:.1}"
    );
}

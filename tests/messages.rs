//! Failures: the notice row, the ✘ on a row whose write was rolled back,
//! and the `!` messages log, driven through the app.

#![cfg(test)]

use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Zoned};
use ratatoist::app::{App, Effect, ErrorKind, Event, Modal, State};
use ratatoist::todoist::fake::CommandMatch;
use ratatoist::todoist::sync::{SyncCommandError, SyncRequest, SyncResponse};
use ratatoist::todoist::{
    ClientError, CompletedQuery, CompletedWindow, FakeClient, LiveMessage, TodoistClient, demo,
};
use ratatui::backend::TestBackend;

fn now() -> Zoned {
    demo::NOW.to_zoned(TimeZone::UTC).unwrap()
}

fn demo_sync() -> Box<SyncResponse> {
    Box::new(serde_json::from_value(demo::account(&now()).sync).unwrap())
}

fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
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

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    rows(app).iter().any(|row| row.contains(text))
}

/// The row that shows `text`.
fn row_with(app: &App<TestBackend>, text: &str) -> Option<String> {
    rows(app).into_iter().find(|row| row.contains(text))
}

/// Applies `events` and every event their effects answer with, through
/// `client`.
async fn drive(
    app: &mut App<TestBackend>,
    client: &FakeClient,
    events: impl IntoIterator<Item = Event>,
) {
    let mut queue: VecDeque<Event> = events.into_iter().collect();
    while let Some(event) = queue.pop_front() {
        for effect in app.handle(event).unwrap() {
            let answer = match effect {
                Effect::Sync(request) => Event::Synced(client.sync(request).await.map(Box::new)),
                Effect::SendCommands(commands) => {
                    let uuids = commands.iter().map(|c| c.uuid.clone()).collect();
                    let request = SyncRequest {
                        sync_token: app.state().sync_token.clone(),
                        resource_types: vec![],
                        commands,
                    };
                    let result = client.sync(request).await.map(Box::new);
                    Event::CommandsSent { uuids, result }
                }
                _ => continue,
            };
            queue.push_back(answer);
        }
    }
}

fn offline() -> ClientError {
    ClientError::Network("connection refused".to_owned())
}

/// The demo account through a client that rejects every `item_update`.
fn rejecting_updates() -> FakeClient {
    let client = FakeClient::new().with_full_sync(demo::account(&now()).sync);
    client.fail_command(
        CommandMatch::Type("item_update".to_owned()),
        SyncCommandError {
            error_code: 22,
            error: "Item not found".to_owned(),
        },
    );
    client
}

#[test]
fn no_failure_pushes_a_modal() {
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();
    send(&mut app, [Event::Synced(Err(offline()))]);
    send(&mut app, [Event::Synced(Ok(demo_sync()))]);

    send(
        &mut app,
        [
            Event::Synced(Err(offline())),
            Event::Live(LiveMessage::Disconnected),
            Event::CommandsSent {
                uuids: vec!["not-pending".to_owned()],
                result: Err(offline()),
            },
            Event::Comments {
                task_id: "t-any".to_owned(),
                fetch_seq: 0,
                result: Err(offline()),
            },
            Event::CompletedTasks {
                query: CompletedQuery {
                    project_id: None,
                    window: CompletedWindow::latest(now().date()),
                },
                result: Err(offline()),
            },
        ],
    );

    assert_eq!(app.state().modals, Vec::new());
    assert!(app.state().failures.len() >= 4);
}

#[test]
fn a_failed_first_sync_writes_a_notice_and_a_log_entry() {
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();

    send(&mut app, [Event::Synced(Err(offline()))]);

    assert_eq!(app.state().failures.len(), 1);
    assert!(shows(
        &app,
        "Could not load your tasks: Todoist could not be reached."
    ));
    assert!(app.state().modals.is_empty());
}

#[tokio::test]
async fn a_rejected_item_update_restores_the_task_marks_its_row_and_logs_it() {
    let client = rejecting_updates();
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();
    drive(&mut app, &client, [Event::Start, key('1')]).await;
    let task = app.state().visible_tasks()[0].clone();
    assert!(row_with(&app, &task.content).is_some());

    drive(&mut app, &client, [key('p'), key('1')]).await;

    let restored = app.state().tasks.iter().find(|t| t.id == task.id).unwrap();
    assert_eq!(restored.priority, task.priority);
    let row = row_with(&app, &task.content).unwrap();
    assert!(row.contains('✘'), "the row is marked");
    assert!(shows(&app, "Could not edit"));
    let [failure] = app.state().failures.newest_first().collect::<Vec<_>>()[..] else {
        panic!("one log entry");
    };
    assert_eq!(failure.kind, ErrorKind::Validation);
    assert_eq!(failure.action, format!("edit \"{}\"", task.content));
    assert_eq!(failure.message, "Item not found");
    assert!(app.state().modals.is_empty());

    drive(&mut app, &client, [key('j')]).await;
    assert!(!shows(&app, "Could not edit"), "any key clears the notice");
    assert!(row_with(&app, &task.content).unwrap().contains('✘'));

    drive(&mut app, &client, [key('k'), press(KeyCode::Enter)]).await;
    assert!(shows(&app, "Item not found"), "the detail shows the reason");
}

#[tokio::test]
async fn the_mark_goes_when_the_task_changes_again() {
    let client = rejecting_updates();
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();
    drive(&mut app, &client, [Event::Start, key('1')]).await;
    let task = app.state().visible_tasks()[0].clone();
    drive(&mut app, &client, [key('p'), key('1')]).await;
    assert!(row_with(&app, &task.content).unwrap().contains('✘'));

    let mut changed = serde_json::to_value(&task).unwrap();
    changed["content"] = serde_json::json!("Renamed elsewhere");
    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "demo-2",
        "items": [changed],
    });
    send(
        &mut app,
        [Event::Synced(Ok(Box::new(
            serde_json::from_value(delta).unwrap(),
        )))],
    );

    assert!(!row_with(&app, "Renamed elsewhere").unwrap().contains('✘'));
}

#[tokio::test]
async fn a_full_sync_keeps_the_mark_until_the_task_changes() {
    let client = rejecting_updates();
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();
    drive(&mut app, &client, [Event::Start, key('1')]).await;
    let task = app.state().visible_tasks()[0].clone();
    drive(&mut app, &client, [key('p'), key('1')]).await;

    drive(&mut app, &client, [key('R')]).await;
    assert!(row_with(&app, &task.content).unwrap().contains('✘'));

    let mut account = demo::account(&now()).sync;
    for item in account["items"].as_array_mut().unwrap() {
        if item["id"] == serde_json::json!(task.id) {
            item["content"] = serde_json::json!("Renamed elsewhere");
        }
    }
    send(
        &mut app,
        [Event::Synced(Ok(Box::new(
            serde_json::from_value(account).unwrap(),
        )))],
    );
    assert!(!row_with(&app, "Renamed elsewhere").unwrap().contains('✘'));
}

fn offline_comments() -> Event {
    Event::Comments {
        task_id: "t-any".to_owned(),
        fetch_seq: 0,
        result: Err(offline()),
    }
}

#[test]
fn an_offline_comments_fetch_never_writes_a_notice() {
    let mut app = app();

    send(&mut app, [offline_comments(), offline_comments()]);

    assert!(!app.state().failures.is_empty());
    assert!(!shows(&app, "Could not"));
}

#[test]
fn a_repeated_failure_updates_the_newest_entry_instead_of_adding_one() {
    let mut app = app();
    send(&mut app, [offline_comments()]);
    let later = now() + SignedDuration::from_mins(5);

    send(
        &mut app,
        [Event::Tick { now: later.clone() }, offline_comments()],
    );

    let failures = &app.state().failures;
    assert_eq!(failures.len(), 1);
    assert_eq!(failures.newest().map(|failure| &failure.at), Some(&later));
}

#[test]
fn background_failures_log_without_a_notice_and_accent_the_indicator() {
    let mut app = app();

    send(
        &mut app,
        [
            Event::Synced(Err(offline())),
            Event::Live(LiveMessage::Disconnected),
            Event::Live(LiveMessage::Disconnected),
        ],
    );

    assert_eq!(app.state().failures.len(), 2, "one per drop, not per retry");
    assert!(!shows(&app, "Could not"));
    let indicator = app.state().indicator().unwrap();
    assert!(indicator.alert);
}

#[test]
fn a_plain_notice_shows_until_the_next_key() {
    let mut state = State::new(now());
    state.notice = Some("Theme kept from ui_settings.json.".to_owned());
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();
    assert!(shows(&app, "Theme kept from ui_settings.json."));

    send(&mut app, [key('j')]);

    assert!(!shows(&app, "Theme kept from ui_settings.json."));
}

/// The app with three failures in the log: a rejected edit, a rate limit
/// and an offline comments fetch.
fn three_failures(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = sized(columns, rows);
    let task = app.state().visible_tasks()[0].clone();
    send(&mut app, [key('p')]);
    let effects = send(&mut app, [key('1')]);
    let Some(Effect::SendCommands(commands)) = effects.first() else {
        panic!("one flush");
    };
    let rejection = serde_json::json!({
        "full_sync": false,
        "sync_token": "demo-2",
        "sync_status": {
            commands[0].uuid.clone(): { "error_code": 22, "error": "Item not found" }
        },
    });
    send(
        &mut app,
        [
            Event::CommandsSent {
                uuids: vec![commands[0].uuid.clone()],
                result: Ok(Box::new(serde_json::from_value(rejection).unwrap())),
            },
            Event::Synced(Err(ClientError::RateLimited {
                retry_after: Some(Duration::from_secs(90)),
            })),
            Event::Comments {
                task_id: task.id,
                fetch_seq: 0,
                result: Err(offline()),
            },
        ],
    );
    app
}

#[test]
fn bang_opens_the_log_newest_first() {
    let mut app = three_failures(120, 36);

    send(&mut app, [key('!')]);

    assert_eq!(
        app.state().top_modal(),
        Some(&Modal::Messages { scroll: 0 })
    );
    let offline_row = rows(&app).iter().position(|r| r.contains("offline"));
    let validation_row = rows(&app).iter().position(|r| r.contains("validation"));
    assert!(offline_row < validation_row, "newest first");
}

#[test]
fn messages_at_120x36() {
    let mut app = three_failures(120, 36);

    send(&mut app, [key('!')]);

    assert_snapshot!(app.backend());
}

#[test]
fn messages_at_80x24() {
    let mut app = three_failures(80, 24);

    send(&mut app, [key('!')]);

    assert_snapshot!(app.backend());
}

#[test]
fn the_log_scrolls_with_j_and_k_and_stops_at_its_end() {
    let mut app = three_failures(80, 12);
    send(&mut app, [key('!')]);

    send(&mut app, (0..50).map(|_| key('j')));
    let Some(Modal::Messages { scroll }) = app.state().top_modal().cloned() else {
        panic!("the log is open");
    };
    let max = ratatoist::ui::messages_max_scroll(app.state(), app.state().screen_size());
    assert!(max > 0);
    assert_eq!(scroll, max);

    send(&mut app, [key('k')]);
    assert_eq!(
        app.state().top_modal(),
        Some(&Modal::Messages { scroll: max - 1 })
    );

    send(&mut app, [press(KeyCode::Esc)]);
    assert_eq!(app.state().top_modal(), None);
}

#[test]
fn an_empty_log_says_so() {
    let mut app = app();

    send(&mut app, [key('!')]);

    assert!(shows(&app, "No errors yet."));
}

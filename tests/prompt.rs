#![cfg(test)]

//! The one prompt for a comment and a task's content, description and due
//! date: named for the field and the task, prefilled where it edits, and
//! kept open with the reason when Enter cannot send.

use chrono::{Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, Modal, Prompt, PromptField, State};
use ratatoist::todoist::demo;
use ratatoist::todoist::sync::{SyncCommand, SyncResponse};
use ratatui::backend::TestBackend;

/// The demo account at `columns` x `rows`, on the Inbox's task list.
fn demo_at(columns: u16, rows: u16) -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now)).unwrap();
    let sync: SyncResponse = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    send(&mut app, [key('1')]);
    app
}

fn demo() -> App<TestBackend> {
    demo_at(80, 24)
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn ctrl(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn typed(text: &str) -> Vec<Event> {
    text.chars().map(key).collect()
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
    events
        .into_iter()
        .flat_map(|event| app.handle(event).unwrap())
        .collect()
}

fn prompt(app: &App<TestBackend>) -> Option<&Prompt> {
    match app.state().top_modal() {
        Some(Modal::Prompt(prompt)) => Some(prompt),
        _ => None,
    }
}

fn commands(effects: &[Effect]) -> Vec<&SyncCommand> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendCommands(commands) => Some(commands),
            _ => None,
        })
        .flatten()
        .collect()
}

fn focused(app: &App<TestBackend>) -> (String, String) {
    let task = app.state().selected_task().unwrap();
    (task.id.clone(), task.content.clone())
}

fn screen(app: &App<TestBackend>) -> String {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn each_edit_key_opens_the_prompt_named_for_its_field_and_the_task() {
    for (edit, field, title) in [
        ('c', PromptField::Comment, "Comment on"),
        ('e', PromptField::Content, "Content of"),
        ('E', PromptField::Description, "Description of"),
        ('d', PromptField::DueDate, "Due date of"),
    ] {
        let mut app = demo_at(120, 36);
        let (id, content) = focused(&app);

        send(&mut app, [key(edit)]);

        let prompt = prompt(&app).unwrap();
        assert_eq!(prompt.field, field);
        assert_eq!(prompt.task_id, id);
        assert!(screen(&app).contains(&format!("{title} {content}")));
    }
}

#[test]
fn content_opens_with_its_text_and_typing_adds_at_the_end() {
    let mut app = demo();
    let (_, content) = focused(&app);

    send(&mut app, [key('e')]);
    send(&mut app, typed("!"));

    assert_eq!(app.state().input.text(), format!("{content}!"));
}

#[test]
fn a_due_date_opens_empty_with_the_due_date_in_force() {
    let mut app = demo();

    send(&mut app, [key('d')]);

    assert!(app.state().input.is_empty());
    assert!(screen(&app).contains("Now  "));
}

#[test]
fn the_bar_names_what_enter_does_in_the_prompt() {
    let mut app = demo();

    send(&mut app, [key('c')]);

    assert!(screen(&app).contains("Enter send"));
    assert!(!screen(&app).contains("Enter choose"));
}

#[test]
fn ctrl_u_clears_the_line() {
    let mut app = demo();

    send(&mut app, [key('e'), ctrl('u')]);

    assert!(app.state().input.is_empty());
}

#[test]
fn enter_with_nothing_to_send_keeps_the_prompt_open_with_the_reason() {
    for (edit, reason) in [
        ('e', "A task needs content"),
        ('c', "A comment needs text"),
        ('d', "Type a date, or no date to clear it"),
    ] {
        let mut app = demo();
        send(&mut app, [key(edit), ctrl('u')]);

        let effects = send(&mut app, [press(KeyCode::Enter)]);

        assert!(commands(&effects).is_empty());
        assert_eq!(prompt(&app).unwrap().error, Some(reason));
        assert!(screen(&app).contains(reason));
    }
}

#[test]
fn an_empty_description_clears_it() {
    let mut app = demo();
    let (id, _) = focused(&app);

    send(&mut app, [key('E'), ctrl('u')]);
    let effects = send(&mut app, [press(KeyCode::Enter)]);

    let sent = commands(&effects);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].args["id"], id.as_str());
    assert_eq!(sent[0].args["description"], "");
    assert!(prompt(&app).is_none());
}

#[test]
fn a_due_date_shows_as_typed_until_todoist_answers_with_the_task() {
    let mut app = demo();
    let (id, _) = focused(&app);

    send(&mut app, [key('d')]);
    send(&mut app, typed("next monday"));
    let effects = send(&mut app, [press(KeyCode::Enter)]);

    let sent = commands(&effects);
    assert_eq!(sent[0].args["due_string"], "next monday");
    assert_eq!(app.state().pending_due(&id), Some("next monday"));

    let uuid = sent[0].uuid.clone();
    let answered: SyncResponse = serde_json::from_value(serde_json::json!({
        "sync_token": "after-write",
        "full_sync": false,
        "sync_status": { uuid.clone(): "ok" },
    }))
    .unwrap();
    app.handle(Event::CommandsSent {
        uuids: vec![uuid],
        result: Ok(Box::new(answered)),
    })
    .unwrap();
    assert_eq!(app.state().pending_due(&id), Some("next monday"));

    let mut task = demo::account(demo::NOW.date()).sync["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id.as_str())
        .unwrap()
        .clone();
    task["due"] = serde_json::json!({ "date": "2026-11-30", "string": "next monday" });
    let delta: SyncResponse = serde_json::from_value(serde_json::json!({
        "sync_token": "delta",
        "full_sync": false,
        "items": [task],
    }))
    .unwrap();
    app.handle(Event::Synced(Ok(Box::new(delta)))).unwrap();

    assert_eq!(app.state().pending_due(&id), None);
}

#[test]
fn esc_closes_the_prompt_and_sends_nothing() {
    let mut app = demo();

    send(&mut app, [key('c')]);
    send(&mut app, typed("on my way"));
    let effects = send(&mut app, [press(KeyCode::Esc)]);

    assert!(commands(&effects).is_empty());
    assert!(prompt(&app).is_none());
}

#[test]
fn a_long_line_keeps_the_cursor_in_view() {
    let mut app = demo();

    send(&mut app, [key('c')]);
    send(&mut app, typed(&"word ".repeat(30)));
    send(&mut app, typed("end"));

    assert!(screen(&app).contains("…"));
    assert!(screen(&app).contains("end"));
}

#[test]
fn the_comment_prompt_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('c')]);
    send(&mut app, typed("Booked for Monday"));
    assert_snapshot!(app.backend());
}

#[test]
fn the_content_prompt_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('e')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_description_prompt_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('E')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_due_date_prompt_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('d')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_due_date_prompt_at_120x36() {
    let mut app = demo_at(120, 36);
    send(&mut app, [key('d')]);
    send(&mut app, typed("next monday"));
    assert_snapshot!(app.backend());
}

#[test]
fn a_refused_line_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('e'), ctrl('u'), press(KeyCode::Enter)]);
    assert_snapshot!(app.backend());
}

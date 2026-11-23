#![cfg(test)]

//! Adding a task with the form: the project in view and, on Today, today's
//! date preset; Enter adds from a text row and chooses on the others; Esc
//! sends nothing; the new row shows at once and goes when Todoist refuses.

use chrono::{Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, FormField, Modal, State, TaskForm};
use ratatoist::todoist::demo;
use ratatoist::todoist::sync::{SyncCommand, SyncResponse};
use ratatoist::ui;
use ratatui::Terminal;
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

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn tab() -> Event {
    press(KeyCode::Tab)
}

fn back_tab() -> Event {
    Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
}

fn enter() -> Event {
    press(KeyCode::Enter)
}

fn esc() -> Event {
    press(KeyCode::Esc)
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

fn form(app: &App<TestBackend>) -> Option<&TaskForm> {
    match app.state().top_modal() {
        Some(Modal::Form(form)) => Some(form),
        _ => None,
    }
}

/// The app's state drawn on a fresh terminal of the same size, so a cell
/// hidden behind a wide character carries nothing from an earlier frame.
fn drawn(app: &App<TestBackend>) -> TestBackend {
    let area = app.backend().buffer().area;
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app.state())).unwrap();
    terminal.backend().clone()
}

fn screen(app: &App<TestBackend>) -> String {
    drawn(app).to_string()
}

/// The Projects screen with `name`'s list open.
fn on_project(app: &mut App<TestBackend>, name: &str) {
    send(app, [key('4')]);
    let focused = |app: &App<TestBackend>| app.state().tree_project().map(|p| p.name.clone());
    for _ in 0..app.state().tree_rows().len() {
        if focused(app).as_deref() == Some(name) {
            break;
        }
        send(app, [key('j')]);
    }
    assert_eq!(focused(app).as_deref(), Some(name));
    send(app, [enter()]);
}

fn shows_task(app: &App<TestBackend>, content: &str) -> bool {
    app.state()
        .visible_tasks()
        .iter()
        .any(|task| task.content == content)
}

#[test]
fn a_on_a_project_s_list_adds_to_that_project() {
    let mut app = demo_at(120, 36);
    on_project(&mut app, "Garden");

    send(&mut app, [key('a')]);

    assert_eq!(form(&app).unwrap().project_id, "p-garden");
    assert!(screen(&app).contains("Add task to Garden"));
    send(&mut app, typed("Prune the roses"));
    let effects = send(&mut app, [enter()]);
    assert_eq!(commands(&effects)[0].args["project_id"], "p-garden");
}

#[test]
fn a_on_a_list_across_projects_adds_to_the_inbox() {
    let mut app = demo();
    send(&mut app, [key('3'), key('a')]);

    assert_eq!(form(&app).unwrap().project_id, "p-inbox");
    assert!(screen(&app).contains("Add task to Inbox"));
}

#[test]
fn on_today_the_due_date_is_today_and_the_new_row_shows_there() {
    let mut app = demo();
    send(&mut app, [key('2'), key('a')]);
    assert_eq!(form(&app).unwrap().due_string, "today");

    send(&mut app, typed("Water the ferns"));
    let effects = send(&mut app, [enter()]);

    assert_eq!(commands(&effects)[0].args["due_string"], "today");
    assert!(shows_task(&app, "Water the ferns"));
}

#[test]
fn enter_adds_from_the_content_row() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    send(&mut app, typed("Pay rent"));

    let effects = send(&mut app, [enter()]);

    let sent = commands(&effects);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].r#type, "item_add");
    assert_eq!(sent[0].args["content"], "Pay rent");
    assert!(sent[0].args.get("due_string").is_none());
    assert!(app.state().modals.is_empty());
    assert!(shows_task(&app, "Pay rent"));
}

#[test]
fn enter_adds_from_the_due_row_with_what_was_typed_there() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    send(&mut app, typed("Pay rent"));
    send(&mut app, [tab(), tab()]);
    assert_eq!(form(&app).unwrap().field, FormField::Due);
    send(&mut app, typed("next fri"));

    let effects = send(&mut app, [enter()]);

    let sent = commands(&effects);
    assert_eq!(sent[0].args["content"], "Pay rent");
    assert_eq!(sent[0].args["due_string"], "next fri");
}

#[test]
fn enter_on_the_priority_and_project_rows_opens_their_pickers() {
    let mut app = demo();
    send(&mut app, [key('a'), tab()]);
    assert!(screen(&app).contains("Enter choose"));

    let effects = send(&mut app, [enter()]);

    assert!(commands(&effects).is_empty());
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Picker(picker)) if picker.title == "Priority of the new task"
    ));
    send(&mut app, [esc(), tab(), tab()]);
    assert_eq!(form(&app).unwrap().field, FormField::Project);
    send(&mut app, [enter()]);
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Picker(picker)) if picker.title == "Project of the new task"
    ));
}

#[test]
fn a_text_row_says_enter_adds() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    assert!(screen(&app).contains("Enter add"));

    send(&mut app, [tab(), tab()]);
    assert!(screen(&app).contains("Enter add"));
}

#[test]
fn shift_tab_moves_back_and_wraps() {
    let mut app = demo();
    send(&mut app, [key('a'), back_tab()]);
    assert_eq!(form(&app).unwrap().field, FormField::Project);
    send(&mut app, [back_tab()]);
    assert_eq!(form(&app).unwrap().field, FormField::Due);
}

#[test]
fn esc_cancels_from_every_row_and_sends_nothing() {
    for tabs in 0..4 {
        let mut app = demo();
        let before = app.state().tasks.len();
        let mut effects = send(&mut app, [key('a')]);
        effects.extend(send(&mut app, typed("Pay rent")));
        effects.extend(send(&mut app, (0..tabs).map(|_| tab())));

        effects.extend(send(&mut app, [esc()]));

        assert!(commands(&effects).is_empty());
        assert!(app.state().modals.is_empty());
        assert_eq!(app.state().tasks.len(), before);
    }
}

#[test]
fn enter_on_empty_content_keeps_the_form_open_with_the_reason() {
    let mut app = demo();
    send(&mut app, [key('a'), key(' '), tab(), tab()]);

    let effects = send(&mut app, [enter()]);

    assert!(commands(&effects).is_empty());
    let form = form(&app).unwrap();
    assert_eq!(form.error, Some("A task needs content"));
    assert_eq!(form.field, FormField::Content);
    assert!(screen(&app).contains("A task needs content"));
}

#[test]
fn a_rejected_add_removes_the_row_with_a_notice_and_a_log_entry() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    send(&mut app, typed("Pay rent"));
    let effects = send(&mut app, [enter()]);
    let command = commands(&effects)[0].clone();
    assert!(shows_task(&app, "Pay rent"));

    let rejection = format!(
        r#"{{"full_sync":false,"sync_token":"demo-token-2",
            "sync_status":{{"{}":{{"error_code":22,"error":"Project not found"}}}}}}"#,
        command.uuid
    );
    let response: SyncResponse = serde_json::from_str(&rejection).unwrap();
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Ok(Box::new(response)),
    })
    .unwrap();

    assert!(!shows_task(&app, "Pay rent"));
    assert!(screen(&app).contains("Todoist refused the change"));
    let logged = app.state().failures.newest().map(|f| f.message.as_str());
    assert_eq!(logged, Some("Project not found"));
}

#[test]
fn the_new_row_takes_the_real_id_when_todoist_answers() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    send(&mut app, typed("Pay rent"));
    let effects = send(&mut app, [enter()]);
    let command = commands(&effects)[0].clone();
    let temp_id = command.temp_id.clone().unwrap();

    let answer = format!(
        r#"{{"full_sync":false,"sync_token":"demo-token-2",
            "sync_status":{{"{}":"ok"}},"temp_id_mapping":{{"{temp_id}":"t-rent"}}}}"#,
        command.uuid
    );
    let response: SyncResponse = serde_json::from_str(&answer).unwrap();
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Ok(Box::new(response)),
    })
    .unwrap();

    let ids: Vec<&str> = app
        .state()
        .tasks
        .iter()
        .filter(|task| task.content == "Pay rent")
        .map(|task| task.id.as_str())
        .collect();
    assert_eq!(ids, ["t-rent"]);
}

#[test]
fn the_empty_content_row_makes_no_parsing_claim() {
    let mut app = demo_at(120, 36);
    send(&mut app, [key('a')]);

    let screen = screen(&app);
    assert!(screen.contains("> type the content"));
    for claim in ["p1", "@label", "#project", "parses"] {
        assert!(!screen.contains(claim));
    }
}

fn filled(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = demo_at(columns, rows);
    send(&mut app, [key('a')]);
    send(&mut app, typed("Prune the roses"));
    send(&mut app, [tab(), enter(), key('2'), tab()]);
    send(&mut app, typed("next sat"));
    send(&mut app, [tab(), enter()]);
    send(&mut app, typed("gard"));
    send(&mut app, [key('1'), back_tab()]);
    app
}

#[test]
fn the_empty_form_at_120x36() {
    let mut app = demo_at(120, 36);
    send(&mut app, [key('a')]);
    assert_snapshot!(drawn(&app));
}

#[test]
fn the_empty_form_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    assert_snapshot!(drawn(&app));
}

#[test]
fn a_filled_form_at_120x36() {
    let app = filled(120, 36);
    assert_eq!(form(&app).unwrap().field, FormField::Due);
    assert_snapshot!(drawn(&app));
}

#[test]
fn a_filled_form_at_80x24() {
    let app = filled(80, 24);
    assert_snapshot!(drawn(&app));
}

#[test]
fn the_project_picker_from_the_form_at_120x36() {
    let mut app = filled(120, 36);
    send(&mut app, [tab(), enter()]);
    assert_snapshot!(drawn(&app));
}

#[test]
fn the_project_picker_from_the_form_at_80x24() {
    let mut app = filled(80, 24);
    send(&mut app, [tab(), enter()]);
    assert_snapshot!(drawn(&app));
}

#[test]
fn the_reason_goes_once_the_content_is_typed() {
    let mut app = demo();
    send(&mut app, [key('a'), enter()]);
    assert!(form(&app).unwrap().error.is_some());
    send(&mut app, typed("abc"));
    assert_eq!(form(&app).unwrap().error, None);
}

#[test]
fn the_reason_goes_when_focus_moves() {
    let mut app = demo();
    send(&mut app, [key('a'), enter(), tab()]);
    assert_eq!(form(&app).unwrap().error, None);
    assert!(!screen(&app).contains("A task needs content"));
}

#[test]
fn the_form_leaves_the_list_above_it_undimmed() {
    let mut app = demo();
    let before = drawn(&app);
    send(&mut app, [key('a')]);
    assert!(form(&app).is_some());
    let after = drawn(&app);
    let width = usize::from(before.buffer().area.width);
    let rows_above_the_form = 1..5;
    for row in rows_above_the_form {
        let cells = row * width..(row + 1) * width;
        assert_eq!(
            before.buffer().content()[cells.clone()],
            after.buffer().content()[cells]
        );
    }
}

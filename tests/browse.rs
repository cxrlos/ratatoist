//! The Projects and Labels screens: the trees, their counts, opening a
//! list with Enter and coming back with Esc, folders and favourites,
//! driven through the app on the demo account.

#![cfg(test)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::Zoned;
use jiff::tz::TimeZone;
use ratatoist::app::{App, Effect, Event, Pane, Screen, State, TreeItem};
use ratatoist::todoist::demo;
use ratatoist::todoist::demo::NOW;
use ratatoist::todoist::fake::CommandMatch;
use ratatoist::todoist::sync::{SyncCommandError, SyncRequest, SyncResponse};
use ratatoist::todoist::{FakeClient, TodoistClient};
use ratatui::backend::TestBackend;

fn now() -> Zoned {
    NOW.to_zoned(TimeZone::UTC).unwrap()
}

fn demo_sync() -> Box<SyncResponse> {
    Box::new(serde_json::from_value(demo::account(&now()).sync).unwrap())
}

fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();
    app
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

/// The row of the screen holding `text`.
fn row_with(app: &App<TestBackend>, text: &str) -> String {
    rows(app)
        .into_iter()
        .find(|row| row.contains(text))
        .unwrap_or_default()
}

/// The count at the end of the tree row holding `name`.
fn count_on(app: &App<TestBackend>, name: &str) -> usize {
    let row = row_with(app, name);
    let row = row.trim_end_matches(['│', ' ']);
    let digits: String = row
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().unwrap()
}

/// Moves the tree focus down to the row holding `name`.
fn focus_row(app: &mut App<TestBackend>, name: &str) {
    send(app, [key('g'), key('g')]);
    for _ in 0..30 {
        let focused = row_with(app, name);
        if focused.contains('›') {
            return;
        }
        send(app, [key('j')]);
    }
    panic!("no such row");
}

/// What a [`FakeClient`] that rejects every `project_update` answers to
/// the commands `effects` send.
async fn rejected(effects: &[Effect]) -> Event {
    let client = FakeClient::new().with_full_sync(demo::account(&now()).sync);
    client.fail_command(
        CommandMatch::Type("project_update".to_owned()),
        SyncCommandError {
            error_code: 22,
            error: "Project not found".to_owned(),
        },
    );
    let Some(Effect::SendCommands(commands)) = effects
        .iter()
        .find(|effect| matches!(effect, Effect::SendCommands(_)))
    else {
        panic!("no command sent");
    };
    let uuids = commands
        .iter()
        .map(|command| command.uuid.clone())
        .collect();
    let request = SyncRequest {
        sync_token: "*".to_owned(),
        resource_types: vec![],
        commands: commands.clone(),
    };
    let result = client.sync(request).await.map(Box::new);
    Event::CommandsSent { uuids, result }
}

#[test]
fn projects_shows_the_personal_space_then_each_workspace_without_the_inbox() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);

    let body = rows(&app);
    let at = |text: &str| body.iter().position(|row| row.contains(text)).unwrap();
    assert!(at("Personal") < at("Home"));
    assert!(at("Home") < at("Lantern Studio"));
    assert!(at("Lantern Studio") < at("Client work"));
    assert!(at("Client work") < at("Atlas redesign"));
    assert!(!body[1..].iter().any(|row| row.contains("Inbox")));
    assert!(!body[1..].iter().any(|row| row.contains("Today")));
}

#[test]
fn a_folder_folds_and_unfolds_with_za() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);
    focus_row(&mut app, "Client work");
    assert!(row_with(&app, "Client work").contains("▾ Client work"));

    send(&mut app, [key('z'), key('a')]);
    assert!(row_with(&app, "Client work").contains("▸ Client work"));
    assert!(!shows(&app, "Atlas redesign"));

    send(&mut app, [press(KeyCode::Char(' '))]);
    assert!(shows(&app, "Atlas redesign"));
}

#[test]
fn favourites_carry_the_star_and_others_the_project_glyph() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);

    assert!(row_with(&app, "Home").contains("★ Home"));
    assert!(row_with(&app, "Reading").contains("# Reading"));
}

#[test]
fn enter_on_a_project_opens_its_list_and_esc_returns_with_the_focus_kept() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);
    focus_row(&mut app, "Garden");

    send(&mut app, [press(KeyCode::Enter)]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    assert_eq!(app.state().list_title(), "Garden");
    assert!(rows(&app)[1].contains("Garden"));

    send(&mut app, [press(KeyCode::Esc)]);
    assert_eq!(app.state().active_pane, Pane::Browse);
    assert_eq!(
        app.state().tree_focus,
        Some(TreeItem::Project("p-garden".to_owned()))
    );
    assert!(row_with(&app, "Garden").contains('›'));
}

#[test]
fn each_project_s_count_equals_the_tasks_its_list_shows() {
    for name in ["Home", "Kitchen renovation", "Reading", "Atlas redesign"] {
        let mut app = sized(119, 36);
        send(&mut app, [key('4')]);
        focus_row(&mut app, name);
        let count = count_on(&app, name);

        send(&mut app, [press(KeyCode::Enter)]);

        assert_eq!(app.state().list_title(), name);
        assert_eq!(app.state().visible_tasks().len(), count);
    }
}

#[test]
fn s_marks_a_project_as_a_favourite_at_once() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);
    focus_row(&mut app, "Reading");

    let effects = send(&mut app, [key('s')]);

    assert!(row_with(&app, "Reading").contains("★ Reading"));
    let Some(Effect::SendCommands(commands)) = effects.first() else {
        panic!("no command sent");
    };
    assert_eq!(commands[0].r#type, "project_update");
    assert_eq!(commands[0].args["is_favorite"], true);
}

#[tokio::test]
async fn s_rolls_back_and_marks_the_project_when_todoist_rejects_it() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);
    focus_row(&mut app, "Reading");
    let effects = send(&mut app, [key('s')]);

    let answer = rejected(&effects).await;
    send(&mut app, [answer]);

    let row = row_with(&app, "Reading");
    assert!(row.contains("# Reading"));
    assert!(row.contains('✘'));
    let reading = app.state().projects.iter().find(|p| p.id == "p-reading");
    assert!(reading.is_some_and(|project| !project.is_favorite));
}

#[test]
fn labels_lists_every_label_with_its_open_task_count() {
    let mut app = sized(120, 36);
    send(&mut app, [key('5')]);

    for label in ["errand", "computer", "deep-work", "outdoors"] {
        let open = app
            .state()
            .tasks
            .iter()
            .filter(|t| !t.checked && !t.is_deleted && t.labels.iter().any(|l| l == label))
            .count();
        assert_eq!(count_on(&app, &format!("@{label} ")), open);
    }
}

#[test]
fn each_label_s_count_equals_the_list_enter_opens_grouped_by_project() {
    for label in ["errand", "computer", "outdoors"] {
        let mut app = sized(119, 36);
        send(&mut app, [key('5')]);
        focus_row(&mut app, &format!("@{label} "));
        let count = count_on(&app, &format!("@{label} "));

        send(&mut app, [press(KeyCode::Enter)]);

        let state = app.state();
        assert_eq!(state.active_pane, Pane::Tasks);
        assert_eq!(state.visible_tasks().len(), count);
        assert!(
            state
                .visible_tasks()
                .iter()
                .all(|task| task.labels.iter().any(|l| l == label))
        );
        let projects: Vec<&str> = state
            .visible_tasks()
            .iter()
            .map(|task| task.project_id.as_str())
            .collect();
        let mut grouped = projects.clone();
        grouped.dedup();
        let mut unique = grouped.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(grouped.len(), unique.len());
    }
}

/// A delta adding a task labelled `label` to `project_id`, completed when
/// `checked`.
fn labelled_task(id: &str, project_id: &str, label: &str, checked: bool) -> Box<SyncResponse> {
    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "fixture-token-2",
        "items": [{
            "id": id,
            "content": "A labelled task added by a delta",
            "checked": checked,
            "child_order": 99,
            "priority": 1,
            "project_id": project_id,
            "labels": [label],
            "added_at": "2026-11-19T08:30:00Z"
        }]
    });
    Box::new(serde_json::from_value(delta).unwrap())
}

/// The count on the `@errand` row and the tasks Enter on it lists.
fn errand_count_and_list(mut app: App<TestBackend>) -> (usize, Vec<String>) {
    send(&mut app, [key('5')]);
    focus_row(&mut app, "@errand ");
    let count = count_on(&app, "@errand ");
    send(&mut app, [press(KeyCode::Enter)]);
    let listed = app
        .state()
        .visible_tasks()
        .iter()
        .map(|task| task.id.clone())
        .collect();
    (count, listed)
}

#[test]
fn a_completed_labelled_task_is_left_out_of_the_count_and_the_list() {
    let (before, _) = errand_count_and_list(sized(119, 36));
    let mut app = sized(119, 36);
    let project_id = app
        .state()
        .tasks
        .iter()
        .find(|task| task.labels.iter().any(|l| l == "errand"))
        .map(|task| task.project_id.clone())
        .unwrap();
    app.handle(Event::Synced(Ok(labelled_task(
        "t-done-errand",
        &project_id,
        "errand",
        true,
    ))))
    .unwrap();
    assert!(app.state().tasks.iter().any(|t| t.id == "t-done-errand"));

    let (count, listed) = errand_count_and_list(app);

    assert_eq!(count, before);
    assert_eq!(listed.len(), count);
    assert!(!listed.iter().any(|id| id == "t-done-errand"));
}

#[test]
fn a_labelled_task_in_a_project_not_loaded_is_left_out_of_the_count() {
    let (before, _) = errand_count_and_list(sized(119, 36));
    let mut app = sized(119, 36);
    app.handle(Event::Synced(Ok(labelled_task(
        "t-stray-errand",
        "p-not-loaded",
        "errand",
        false,
    ))))
    .unwrap();
    assert!(app.state().tasks.iter().any(|t| t.id == "t-stray-errand"));

    let (count, listed) = errand_count_and_list(app);

    assert_eq!(count, before);
    assert_eq!(listed.len(), count);
}

#[test]
fn esc_from_a_label_s_list_returns_to_the_same_label() {
    let mut app = sized(120, 36);
    send(&mut app, [key('5')]);
    focus_row(&mut app, "@outdoors ");
    send(&mut app, [press(KeyCode::Enter)]);
    assert_eq!(app.state().list_title(), "@outdoors");

    send(&mut app, [press(KeyCode::Esc)]);

    assert_eq!(app.state().active_pane, Pane::Browse);
    assert_eq!(app.state().screen, Screen::Labels);
    assert!(row_with(&app, "@outdoors ").contains('›'));
}

#[test]
fn projects_at_120x36() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);
    assert_snapshot!(app.backend());
}

#[test]
fn projects_at_80x24() {
    let mut app = sized(80, 24);
    send(&mut app, [key('4')]);
    assert_snapshot!(app.backend());
}

#[test]
fn an_opened_project_at_120x36() {
    let mut app = sized(120, 36);
    send(&mut app, [key('4')]);
    focus_row(&mut app, "Kitchen renovation");
    send(&mut app, [press(KeyCode::Enter)]);
    assert_snapshot!(app.backend());
}

#[test]
fn an_opened_project_at_80x24() {
    let mut app = sized(80, 24);
    send(&mut app, [key('4')]);
    focus_row(&mut app, "Kitchen renovation");
    send(&mut app, [press(KeyCode::Enter)]);
    assert_snapshot!(app.backend());
}

#[test]
fn labels_at_120x36() {
    let mut app = sized(120, 36);
    send(&mut app, [key('5')]);
    assert_snapshot!(app.backend());
}

#[test]
fn labels_at_80x24() {
    let mut app = sized(80, 24);
    send(&mut app, [key('5')]);
    assert_snapshot!(app.backend());
}

#[test]
fn an_opened_label_at_120x36() {
    let mut app = sized(120, 36);
    send(&mut app, [key('5')]);
    focus_row(&mut app, "@errand ");
    send(&mut app, [press(KeyCode::Enter)]);
    assert_snapshot!(app.backend());
}

#[test]
fn an_opened_label_at_80x24() {
    let mut app = sized(80, 24);
    send(&mut app, [key('5')]);
    focus_row(&mut app, "@errand ");
    send(&mut app, [press(KeyCode::Enter)]);
    assert_snapshot!(app.backend());
}

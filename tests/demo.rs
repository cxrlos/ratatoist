#![cfg(test)]

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Days, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatoist::app::{App, Effect, Event, State};
use ratatoist::todoist::demo::{self, DemoClient};
use ratatoist::todoist::model::{CompletedTasksResponse, Task};
use ratatoist::todoist::sync::{SyncCommand, SyncRequest, SyncResponse};
use ratatoist::todoist::{CompletedQuery, TodoistClient};
use ratatui::backend::TestBackend;
use serde_json::json;

fn today() -> NaiveDate {
    demo::NOW.date()
}

fn now() -> DateTime<Local> {
    Local.from_local_datetime(&demo::NOW).earliest().unwrap()
}

fn day(offset: i64) -> String {
    let date = if offset < 0 {
        today() - Days::new(offset.unsigned_abs())
    } else {
        today() + Days::new(offset.unsigned_abs())
    };
    date.format("%Y-%m-%d").to_string()
}

fn sync() -> SyncResponse {
    serde_json::from_value(demo::account(today()).sync).unwrap()
}

fn completed() -> Vec<Task> {
    let response: CompletedTasksResponse =
        serde_json::from_value(demo::account(today()).completed).unwrap();
    response.items.into_iter().map(|r| r.into_task()).collect()
}

fn tasks() -> Vec<Task> {
    sync().items.unwrap()
}

fn due_date(task: &Task) -> Option<&str> {
    task.due.as_ref().map(|d| d.date.split('T').next().unwrap())
}

#[test]
fn two_builds_of_the_account_are_equal() {
    assert_eq!(demo::account(today()), demo::account(today()));
}

#[test]
fn the_account_is_dated_from_the_day_it_is_built_for() {
    let later = today() + Days::new(40);
    let account: SyncResponse = serde_json::from_value(demo::account(later).sync).unwrap();
    let later = later.format("%Y-%m-%d").to_string();

    assert!(
        account
            .items
            .unwrap()
            .iter()
            .any(|t| due_date(t) == Some(later.as_str()))
    );
}

#[test]
fn has_a_personal_inbox() {
    let projects = sync().projects.unwrap();
    assert!(
        projects
            .iter()
            .any(|p| p.is_inbox() && p.workspace_id.is_none())
    );
}

#[test]
fn has_four_or_five_personal_projects_besides_the_inbox() {
    let projects = sync().projects.unwrap();
    let personal = projects
        .iter()
        .filter(|p| !p.is_inbox() && p.workspace_id.is_none())
        .count();
    assert!((4..=5).contains(&personal), "{personal}");
}

#[test]
fn has_a_favourite_personal_project() {
    let projects = sync().projects.unwrap();
    assert!(
        projects
            .iter()
            .any(|p| p.is_favorite && p.workspace_id.is_none())
    );
}

#[test]
fn has_a_personal_project_with_a_parent() {
    let projects = sync().projects.unwrap();
    let ids: HashSet<&str> = projects.iter().map(|p| p.id.as_str()).collect();
    assert!(projects.iter().any(|p| {
        p.workspace_id.is_none() && p.parent_id.as_deref().is_some_and(|id| ids.contains(id))
    }));
}

#[test]
fn has_a_workspace_with_a_folder_holding_two_projects() {
    let account = sync();
    let workspace = &account.workspaces.unwrap()[0];
    let folders = account.folders.unwrap();
    let folder = folders
        .iter()
        .find(|f| f.workspace_id == workspace.id)
        .unwrap();
    let in_folder = account
        .projects
        .unwrap()
        .iter()
        .filter(|p| {
            p.workspace_id.as_deref() == Some(&workspace.id)
                && p.folder_id.as_deref() == Some(&folder.id)
        })
        .count();
    assert_eq!(in_folder, 2);
}

#[test]
fn has_sections_holding_tasks() {
    let account = sync();
    let sections = account.sections.unwrap();
    assert!(sections.len() >= 3);
    let items = account.items.unwrap();
    assert!(
        sections
            .iter()
            .all(|s| items.iter().any(|t| t.section_id.as_deref() == Some(&s.id)))
    );
}

#[test]
fn has_about_a_dozen_labels_all_in_use() {
    let account = sync();
    let labels = account.labels.unwrap();
    assert!((10..=14).contains(&labels.len()), "{}", labels.len());
    let items = account.items.unwrap();
    assert!(
        labels
            .iter()
            .all(|l| items.iter().any(|t| t.labels.contains(&l.name)))
    );
}

#[test]
fn has_about_sixty_tasks() {
    let count = tasks().len() + completed().len();
    assert!((55..=70).contains(&count), "{count}");
}

#[test]
fn has_subtasks_two_levels_deep() {
    let tasks = tasks();
    let parent_of = |id: &str| {
        tasks
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.parent_id.clone())
    };
    assert!(tasks.iter().any(|t| {
        t.parent_id
            .as_deref()
            .and_then(parent_of)
            .is_some_and(|grandparent| parent_of(&grandparent).is_none())
    }));
}

#[test]
fn has_recurring_tasks() {
    assert!(
        tasks()
            .iter()
            .any(|t| t.due.as_ref().is_some_and(|d| d.is_recurring))
    );
}

#[test]
fn has_timed_due_dates() {
    assert!(
        tasks()
            .iter()
            .any(|t| t.due.as_ref().is_some_and(|d| d.date.contains('T')))
    );
}

#[test]
fn has_all_day_due_dates() {
    assert!(
        tasks()
            .iter()
            .any(|t| t.due.as_ref().is_some_and(|d| d.date.len() == 10))
    );
}

#[test]
fn has_overdue_tasks() {
    let today = day(0);
    assert!(
        tasks()
            .iter()
            .any(|t| due_date(t).is_some_and(|d| d < today.as_str()))
    );
}

#[test]
fn has_tasks_due_today() {
    let today = day(0);
    assert!(tasks().iter().any(|t| due_date(t) == Some(today.as_str())));
}

#[test]
fn has_tasks_due_later() {
    let today = day(0);
    assert!(
        tasks()
            .iter()
            .any(|t| due_date(t).is_some_and(|d| d > today.as_str()))
    );
}

#[test]
fn has_every_priority_from_p1_to_p4() {
    let priorities: HashSet<u8> = tasks().iter().map(|t| t.priority).collect();
    assert_eq!(priorities, HashSet::from([1, 2, 3, 4]));
}

#[test]
fn has_comments_from_two_or_three_authors() {
    let account = sync();
    let notes = account.notes.unwrap();
    let authors: HashSet<&str> = notes
        .iter()
        .filter_map(|n| n.posted_by_uid.as_deref())
        .collect();
    assert!((2..=3).contains(&authors.len()), "{authors:?}");
    let items = account.items.unwrap();
    for task in items.iter().filter(|t| t.note_count.unwrap_or(0) > 0) {
        let on_task = notes
            .iter()
            .filter(|n| n.item_id.as_deref() == Some(&task.id))
            .count();
        assert_eq!(Some(on_task as i32), task.note_count, "{}", task.id);
    }
}

#[test]
fn has_completed_tasks_for_the_done_filter() {
    let projects = sync().projects.unwrap();
    let completed = completed();
    assert!(completed.len() >= 4);
    assert!(
        completed
            .iter()
            .all(|t| t.checked && projects.iter().any(|p| p.id == t.project_id))
    );
}

#[test]
fn has_long_content() {
    assert!(tasks().iter().any(|t| t.content.chars().count() > 100));
}

#[test]
fn has_cjk_content() {
    let cjk =
        |c: char| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c);
    assert!(tasks().iter().any(|t| t.content.chars().any(cjk)));
}

#[test]
fn has_emoji_content() {
    let emoji = |c: char| ('\u{1f300}'..='\u{1faff}').contains(&c);
    assert!(tasks().iter().any(|t| t.content.chars().any(emoji)));
}

#[test]
fn has_accented_content() {
    assert!(
        tasks()
            .iter()
            .any(|t| t.content.chars().any(|c| "áéíóúñçüöèàêâ".contains(c)))
    );
}

// Through the app: the account loads by the same path a real sync takes,
// and writes against the demo client stay on screen.

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .any(|line| line.contains(text))
}

/// Feeds `events` to the app and carries out every effect against
/// `client`, as the runtime would, each answer arriving before the next
/// event, until nothing is left to do.
async fn drive(
    app: &mut App<TestBackend>,
    client: &DemoClient,
    events: impl IntoIterator<Item = Event>,
) -> Vec<Effect> {
    let mut seen = Vec::new();
    let mut queue: VecDeque<Event> = events.into_iter().collect();
    while let Some(event) = queue.pop_front() {
        let mut answers = Vec::new();
        for effect in app.handle(event).unwrap() {
            seen.push(effect.clone());
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
                Effect::FetchComments { task_id, fetch_seq } => {
                    let result = client.comments(&task_id).await;
                    Event::Comments {
                        task_id,
                        fetch_seq,
                        result,
                    }
                }
                Effect::FetchCompletedTasks(query) => {
                    let project_id = query.project_id.clone();
                    let result = client.completed_tasks(query).await;
                    Event::CompletedTasks { project_id, result }
                }
                _ => continue,
            };
            answers.push(answer);
        }
        for answer in answers.into_iter().rev() {
            queue.push_front(answer);
        }
    }
    seen
}

async fn started(client: &DemoClient) -> (App<TestBackend>, Vec<Effect>) {
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();
    let effects = drive(&mut app, client, [Event::Start]).await;
    (app, effects)
}

#[test]
fn loads_through_the_normal_sync_path() {
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();

    app.handle(Event::Synced(Ok(Box::new(sync())))).unwrap();

    assert!(!app.state().visible_tasks().is_empty());
    assert!(shows(&app, "Inbox"));
    assert!(shows(&app, "Lantern Studio"));
}

#[tokio::test]
async fn starts_with_tasks_visible_and_never_opens_the_live_connection() {
    let client = demo::client(today());

    let (app, effects) = started(&client).await;

    assert!(!app.state().visible_tasks().is_empty());
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::OpenLive { .. })),
        "{effects:?}"
    );
}

#[tokio::test]
async fn completing_a_task_keeps_it_completed_and_lists_it_under_done() {
    let client = demo::client(today());
    let (mut app, _) = started(&client).await;
    let first = app.state().visible_tasks()[0].clone();
    assert!(shows(&app, &first.content));

    drive(&mut app, &client, [key('l'), key('x')]).await;

    assert!(app.state().error.is_none());
    assert!(!shows(&app, &first.content));
    drive(&mut app, &client, [key('f')]).await;
    assert!(shows(&app, &first.content));
}

#[tokio::test]
async fn an_added_task_stays_under_its_new_id_and_survives_a_full_sync() {
    let client = demo::client(today());
    let (mut app, _) = started(&client).await;

    drive(&mut app, &client, [key('l'), key('a')]).await;
    drive(&mut app, &client, "Feed the parrot".chars().map(key)).await;
    drive(
        &mut app,
        &client,
        [press(KeyCode::Enter), press(KeyCode::Tab)],
    )
    .await;

    let added = app
        .state()
        .tasks
        .iter()
        .find(|t| t.content == "Feed the parrot")
        .unwrap();
    assert!(!added.id.starts_with("tmp_"), "{}", added.id);
    assert!(shows(&app, "Feed the parrot"));
    drive(&mut app, &client, [key('R')]).await;
    assert!(shows(&app, "Feed the parrot"));
}

#[tokio::test]
async fn a_comment_stays_after_the_comments_reload() {
    let client = demo::client(today());
    let (mut app, _) = started(&client).await;

    drive(
        &mut app,
        &client,
        [key('l'), press(KeyCode::Enter), key('c')],
    )
    .await;
    drive(&mut app, &client, "Seen, thanks".chars().map(key)).await;
    let effects = drive(&mut app, &client, [press(KeyCode::Enter)]).await;

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::FetchComments { .. })),
        "the comments reload after the write"
    );
    assert!(app.state().error.is_none());
    assert!(shows(&app, "Seen, thanks"));
}

#[tokio::test]
async fn comments_are_served_per_task() {
    let client = demo::client(today());
    let account = sync();
    let notes = account.notes.unwrap();
    let task_id = notes[0].item_id.clone().unwrap();

    let served = client.comments(&task_id).await.unwrap();

    assert!(!served.is_empty());
    assert!(
        served
            .iter()
            .all(|c| c.item_id.as_deref() == Some(&task_id))
    );
}

#[tokio::test]
async fn completed_tasks_are_served_per_project() {
    let client = demo::client(today());
    let project_id = completed()[0].project_id.clone();

    let served = client
        .completed_tasks(CompletedQuery {
            project_id: Some(project_id.clone()),
            since: None,
        })
        .await
        .unwrap();

    assert!(!served.is_empty());
    assert!(served.iter().all(|t| t.project_id == project_id));
}

// Writes against the demo client, checked through the next full sync.

async fn send(client: &DemoClient, kind: &str, args: serde_json::Value) {
    let request = SyncRequest {
        sync_token: "demo".to_owned(),
        resource_types: vec![],
        commands: vec![SyncCommand {
            r#type: kind.to_owned(),
            temp_id: Some(format!("tmp-{kind}")),
            uuid: format!("uuid-{kind}"),
            args,
        }],
    };
    let response = client.sync(request).await.unwrap();
    assert!(response.sync_status.values().all(|s| !s.is_err()));
}

async fn full_sync(client: &DemoClient) -> Vec<Task> {
    let request = SyncRequest {
        sync_token: "*".to_owned(),
        resource_types: vec![],
        commands: vec![],
    };
    client.sync(request).await.unwrap().items.unwrap()
}

async fn done(client: &DemoClient) -> Vec<Task> {
    let query = CompletedQuery {
        project_id: None,
        since: None,
    };
    client.completed_tasks(query).await.unwrap()
}

#[tokio::test]
async fn reopening_a_seeded_completed_task_brings_it_back_whole() {
    let client = demo::client(today());

    send(&client, "item_reopen", json!({ "id": "t-dentist" })).await;

    let items = full_sync(&client).await;
    let reopened = items.iter().find(|t| t.id == "t-dentist").unwrap();
    assert_eq!(reopened.content, "Book the dentist check-up");
    assert_eq!(reopened.project_id, "p-inbox");
    assert!(!reopened.checked);
    assert!(reopened.completed_at.is_none());
    assert!(!done(&client).await.iter().any(|t| t.id == "t-dentist"));
}

#[tokio::test]
async fn reopening_a_seeded_done_task_in_the_app_keeps_full_syncs_working() {
    let client = demo::client(today());
    let (mut app, _) = started(&client).await;
    drive(&mut app, &client, [key('l'), key('f')]).await;
    assert!(shows(&app, "Book the dentist check-up"));

    drive(&mut app, &client, [key('x'), key('R')]).await;

    assert!(app.state().error.is_none());
    let reopened = app.state().tasks.iter().find(|t| t.id == "t-dentist");
    assert!(reopened.is_some_and(|t| !t.checked), "{reopened:?}");
}

#[tokio::test]
async fn completing_a_parent_completes_its_subtasks() {
    let client = demo::client(today());
    let family = ["t-tiles", "t-samples", "t-grout", "t-worktop"];

    send(&client, "item_close", json!({ "id": "t-tiles" })).await;

    let items = full_sync(&client).await;
    assert!(!items.iter().any(|t| family.contains(&t.id.as_str())));
    let done = done(&client).await;
    for id in family {
        assert!(done.iter().any(|t| t.id == id && t.checked), "{id}");
    }
}

#[tokio::test]
async fn reopening_a_subtask_reopens_its_completed_parents() {
    let client = demo::client(today());
    send(&client, "item_close", json!({ "id": "t-tiles" })).await;

    send(&client, "item_reopen", json!({ "id": "t-grout" })).await;

    let items = full_sync(&client).await;
    let open: Vec<&str> = items.iter().map(|t| t.id.as_str()).collect();
    for id in ["t-tiles", "t-samples", "t-grout"] {
        assert!(open.contains(&id), "{id}");
    }
    assert!(!open.contains(&"t-worktop"));
    assert!(done(&client).await.iter().any(|t| t.id == "t-worktop"));
}

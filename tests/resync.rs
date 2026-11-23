#![cfg(test)]

//! `R` re-reads everything the startup sync reads.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures_util::Stream;
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, State};
use ratatoist::todoist::model::{Comment, Task, UserInfo};
use ratatoist::todoist::sync::{SyncRequest, SyncResponse};
use ratatoist::todoist::{ClientError, CompletedQuery, FakeClient, LiveMessage, TodoistClient};
use ratatui::backend::TestBackend;
use serde_json::Value;

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
}

/// A full sync with one workspace holding one folder, named `folder`.
fn account(folder: &str, sync_token: &str) -> Value {
    serde_json::json!({
      "full_sync": true,
      "sync_token": sync_token,
      "user": { "id": "u1", "full_name": "Ada Example", "email": "ada@example.com" },
      "collaborators": [{ "id": "u2", "full_name": "Grace Example" }],
      "workspaces": [{ "id": "w1", "name": "Acme" }],
      "folders": [{ "id": "f1", "name": folder, "workspace_id": "w1", "child_order": 0 }],
      "projects": [
        { "id": "p-inbox", "name": "Inbox", "color": "charcoal", "child_order": 0,
          "is_shared": false, "is_favorite": false, "inbox_project": true },
        { "id": "p-site", "name": "Website", "color": "blue", "child_order": 1,
          "is_shared": true, "is_favorite": false, "workspace_id": "w1", "folder_id": "f1" }
      ],
      "items": []
    })
}

/// [`FakeClient`] that records every sync request and, like Todoist, answers
/// only with the resource types the request named.
#[derive(Clone)]
struct Recording {
    inner: FakeClient,
    requests: Arc<Mutex<Vec<SyncRequest>>>,
}

impl Recording {
    fn new(inner: FakeClient) -> Self {
        Self {
            inner,
            requests: Arc::default(),
        }
    }

    fn requests(&self) -> Vec<SyncRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl TodoistClient for Recording {
    async fn sync(&self, request: SyncRequest) -> Result<SyncResponse, ClientError> {
        self.requests.lock().unwrap().push(request.clone());
        let wants = |kind: &str| request.resource_types.iter().any(|t| t == kind);
        let mut response = self.inner.sync(request.clone()).await?;
        if !wants("workspaces") {
            response.workspaces = None;
        }
        if !wants("folders") {
            response.folders = None;
        }
        if !wants("user") {
            response.user = None;
        }
        if !wants("collaborators") {
            response.collaborators = None;
        }
        Ok(response)
    }

    async fn user(&self) -> Result<UserInfo, ClientError> {
        self.inner.user().await
    }

    async fn comments(&self, task_id: &str) -> Result<Vec<Comment>, ClientError> {
        self.inner.comments(task_id).await
    }

    async fn completed_tasks(&self, query: CompletedQuery) -> Result<Vec<Task>, ClientError> {
        self.inner.completed_tasks(query).await
    }

    fn live_events(&self, url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static {
        self.inner.live_events(url)
    }
}

fn shift_r() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT))
}

fn sync_request(effects: &[Effect]) -> SyncRequest {
    let requests: Vec<&SyncRequest> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Sync(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "{effects:?}");
    requests[0].clone()
}

/// The app after the startup sync, run against `client`.
async fn started(client: &Recording) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    let request = sync_request(&app.handle(Event::Start).unwrap());
    let response = client.sync(request).await;
    app.handle(Event::Synced(response.map(Box::new))).unwrap();
    app
}

#[tokio::test]
async fn r_asks_for_every_resource_the_startup_sync_reads() {
    let client = Recording::new(FakeClient::new().with_full_sync(account("Marketing", "t1")));
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    let startup = sync_request(&app.handle(Event::Start).unwrap());
    let response = client.sync(startup.clone()).await;
    app.handle(Event::Synced(response.map(Box::new))).unwrap();

    let resync = sync_request(&app.handle(shift_r()).unwrap());

    assert_eq!(resync.sync_token, "*");
    for kind in ["workspaces", "folders", "user", "collaborators"] {
        assert!(
            resync.resource_types.iter().any(|t| t == kind),
            "{kind} missing from {:?}",
            resync.resource_types
        );
    }
    assert_eq!(resync.resource_types, startup.resource_types);
}

#[tokio::test]
async fn the_client_receives_the_full_resource_list_on_r() {
    let client = Recording::new(FakeClient::new().with_full_sync(account("Marketing", "t1")));
    let mut app = started(&client).await;

    let resync = sync_request(&app.handle(shift_r()).unwrap());
    client.sync(resync).await.unwrap();

    let requests = client.requests();
    let [startup, received] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert_eq!(received.sync_token, "*");
    assert_eq!(received.resource_types, startup.resource_types);
}

#[tokio::test]
async fn r_shows_a_folder_renamed_elsewhere() {
    let before = Recording::new(FakeClient::new().with_full_sync(account("Marketing", "t1")));
    let mut app = started(&before).await;
    assert!(format!("{}", app.backend()).contains("Marketing"));
    let after = Recording::new(FakeClient::new().with_full_sync(account("Growth", "t2")));

    let resync = sync_request(&app.handle(shift_r()).unwrap());
    let response = after.sync(resync).await;
    app.handle(Event::Synced(response.map(Box::new))).unwrap();

    let screen = format!("{}", app.backend());
    assert!(screen.contains("Growth") && !screen.contains("Marketing"));
    assert_snapshot!(app.backend());
}

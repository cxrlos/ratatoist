//! A [`TodoistClient`] that never touches the network.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures_util::Stream;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::model::{Comment, CompletedTasksResponse, Paginated, Task, UserInfo};
use super::sync::{SyncCommand, SyncCommandError, SyncCommandResult, SyncRequest, SyncResponse};
use super::{ClientError, CompletedQuery, LiveMessage, TodoistClient};

/// Answers from the JSON it was given (response bodies in Todoist's v1
/// shapes) and with an empty account for anything it was not. It records
/// every command it receives and answers each with `ok`, unless told to fail
/// it. Clones share the fixtures, the failures and the record.
#[derive(Debug, Clone, Default)]
pub struct FakeClient {
    shared: Arc<Mutex<Shared>>,
}

/// Which commands [`FakeClient::fail_command`] fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandMatch {
    Uuid(String),
    /// Every command of this type, such as `item_close`.
    Type(String),
}

#[derive(Debug, Default)]
struct Shared {
    full_sync: Option<Value>,
    incremental_sync: Option<Value>,
    user: Option<Value>,
    comments: Option<Value>,
    completed_tasks: Option<Value>,
    failures: Vec<(CommandMatch, SyncCommandError)>,
    commands: Vec<SyncCommand>,
}

impl FakeClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// The body a full sync (`sync_token` `*`) answers with.
    pub fn with_full_sync(self, body: Value) -> Self {
        self.shared().full_sync = Some(body);
        self
    }

    /// The body every incremental sync answers with.
    pub fn with_incremental_sync(self, body: Value) -> Self {
        self.shared().incremental_sync = Some(body);
        self
    }

    /// The `GET /user` body.
    pub fn with_user(self, body: Value) -> Self {
        self.shared().user = Some(body);
        self
    }

    /// One `GET /comments` page, served for every task.
    pub fn with_comments(self, body: Value) -> Self {
        self.shared().comments = Some(body);
        self
    }

    /// One `GET /tasks/completed/by_completion_date` page, served for every
    /// query.
    pub fn with_completed_tasks(self, body: Value) -> Self {
        self.shared().completed_tasks = Some(body);
        self
    }

    /// From now on, commands that `which` matches get `error` in
    /// `sync_status` instead of `ok`. The first matching failure wins.
    pub fn fail_command(&self, which: CommandMatch, error: SyncCommandError) {
        self.shared().failures.push((which, error));
    }

    /// Every command received so far, oldest first.
    pub fn commands(&self) -> Vec<SyncCommand> {
        self.shared().commands.clone()
    }

    fn shared(&self) -> MutexGuard<'_, Shared> {
        // Plain data with no invariant across fields, so a panic while it
        // was held cannot leave it in a state that matters.
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Shared {
    fn status(&self, command: &SyncCommand) -> SyncCommandResult {
        let failure = self.failures.iter().find(|(which, _)| match which {
            CommandMatch::Uuid(uuid) => *uuid == command.uuid,
            CommandMatch::Type(kind) => *kind == command.r#type,
        });
        match failure {
            Some((_, error)) => SyncCommandResult::Err(error.clone()),
            None => SyncCommandResult::Ok("ok".to_owned()),
        }
    }
}

fn decode<T: DeserializeOwned>(body: Value) -> Result<T, ClientError> {
    serde_json::from_value(body).map_err(|e| ClientError::Decode(e.to_string()))
}

impl TodoistClient for FakeClient {
    async fn sync(&self, request: SyncRequest) -> Result<SyncResponse, ClientError> {
        let mut shared = self.shared();
        // Recorded before the body is decoded, so a malformed fixture still
        // leaves the record of what was sent.
        let statuses: Vec<_> = request
            .commands
            .iter()
            .map(|command| (command.uuid.clone(), shared.status(command)))
            .collect();
        shared.commands.extend(request.commands);
        let full = request.sync_token == "*";
        let body = if full {
            shared.full_sync.clone()
        } else {
            shared.incremental_sync.clone()
        };
        drop(shared);
        let mut response = match body {
            Some(body) => decode(body)?,
            None => SyncResponse::empty(full),
        };
        response.sync_status.extend(statuses);
        Ok(response)
    }

    async fn user(&self) -> Result<UserInfo, ClientError> {
        match self.shared().user.clone() {
            Some(body) => decode(body),
            None => Ok(UserInfo::default()),
        }
    }

    async fn comments(&self, _task_id: &str) -> Result<Vec<Comment>, ClientError> {
        match self.shared().comments.clone() {
            Some(body) => Ok(decode::<Paginated<Comment>>(body)?.results),
            None => Ok(Vec::new()),
        }
    }

    async fn completed_tasks(&self, _query: CompletedQuery) -> Result<Vec<Task>, ClientError> {
        match self.shared().completed_tasks.clone() {
            Some(body) => Ok(decode::<CompletedTasksResponse>(body)?.into_tasks()),
            None => Ok(Vec::new()),
        }
    }

    fn live_events(&self, _url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static {
        futures_util::stream::empty()
    }
}

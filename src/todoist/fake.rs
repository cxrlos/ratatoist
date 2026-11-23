//! A [`TodoistClient`] that never touches the network.

use futures_util::Stream;

use super::model::{Comment, Task, UserInfo};
use super::sync::{SyncRequest, SyncResponse};
use super::{ClientError, CompletedQuery, LiveMessage, TodoistClient};

/// Answers every request with an empty account.
#[derive(Debug, Clone, Default)]
pub struct FakeClient;

impl FakeClient {
    pub fn new() -> Self {
        Self
    }
}

impl TodoistClient for FakeClient {
    async fn sync(&self, request: SyncRequest) -> Result<SyncResponse, ClientError> {
        Ok(SyncResponse::empty(request.sync_token == "*"))
    }

    async fn user(&self) -> Result<UserInfo, ClientError> {
        Ok(UserInfo::default())
    }

    async fn comments(&self, _task_id: &str) -> Result<Vec<Comment>, ClientError> {
        Ok(Vec::new())
    }

    async fn completed_tasks(&self, _query: CompletedQuery) -> Result<Vec<Task>, ClientError> {
        Ok(Vec::new())
    }

    fn live_events(&self, _url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static {
        futures_util::stream::empty()
    }
}

//! Everything ratatoist needs from Todoist goes through [`TodoistClient`], so
//! tests and `--demo` swap the real client for [`FakeClient`].

pub mod fake;
pub mod http;
mod live;
pub mod model;
pub mod sync;

use std::future::Future;
use std::time::Duration;

use futures_util::Stream;

pub use fake::FakeClient;
pub use http::{ClientConfig, HttpClient};
use model::{Comment, Task, UserInfo};
use sync::{SyncRequest, SyncResponse};

/// Reads, writes and live events. Implementations are cheap to clone, so the
/// runtime can hand one to each request it starts.
pub trait TodoistClient: Clone + Send + Sync + 'static {
    /// `POST /sync`: a full (`sync_token` `*`) or incremental read, plus any
    /// commands to apply.
    fn sync(
        &self,
        request: SyncRequest,
    ) -> impl Future<Output = Result<SyncResponse, ClientError>> + Send;

    /// The signed-in user, including the live-updates URL.
    fn user(&self) -> impl Future<Output = Result<UserInfo, ClientError>> + Send;

    /// Every comment on a task, following cursors to the end.
    fn comments(
        &self,
        task_id: &str,
    ) -> impl Future<Output = Result<Vec<Comment>, ClientError>> + Send;

    /// Completed tasks, following cursors to the end.
    fn completed_tasks(
        &self,
        query: CompletedQuery,
    ) -> impl Future<Output = Result<Vec<Task>, ClientError>> + Send;

    /// Messages from the live-updates connection at `url`. The stream ends
    /// when the connection closes.
    fn live_events(&self, url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static;
}

/// Which completed tasks to load.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletedQuery {
    pub project_id: Option<String>,
    /// Only tasks completed after this RFC 3339 time.
    pub since: Option<String>,
}

/// What the live-updates connection reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveMessage {
    Connected,
    /// Something changed on Todoist's side; an incremental sync picks it up.
    Changed,
    Disconnected,
}

/// Why a request to Todoist failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("Todoist rejected the token (401); run \"ratatoist auth login\"")]
    Unauthorized,
    #[error("Todoist is rate limiting requests")]
    RateLimited { retry_after: Option<Duration> },
    #[error("Todoist answered {status}: {message}")]
    Http { status: u16, message: String },
    #[error("cannot reach Todoist: {0}")]
    Network(String),
    #[error("unexpected response from Todoist: {0}")]
    Decode(String),
    #[error("{0}")]
    Other(String),
}

//! Everything ratatoist needs from Todoist goes through [`TodoistClient`], so
//! tests and `--demo` swap the real client for [`FakeClient`].

pub mod demo;
pub mod fake;
pub mod http;
mod live;
pub mod model;
pub mod sync;

use std::future::Future;
use std::time::Duration;

use futures_util::Stream;
use jiff::civil::Date;

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

    /// The tasks completed in one window, following cursors to the end.
    fn completed_tasks(
        &self,
        query: CompletedQuery,
    ) -> impl Future<Output = Result<Vec<Task>, ClientError>> + Send;

    /// Messages from the live-updates connection at `url`. The stream ends
    /// when the connection closes.
    fn live_events(&self, url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static;
}

/// Which completed tasks to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedQuery {
    pub project_id: Option<String>,
    pub window: CompletedWindow,
}

/// The days a completed-tasks load covers: from `since` up to, not
/// including, `until`, both as UTC midnights. Todoist answers windows of at
/// most three months, so history loads one window at a time, newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletedWindow {
    pub since: Date,
    pub until: Date,
}

impl CompletedWindow {
    /// Twelve weeks: shorter than any three calendar months.
    const DAYS: i32 = 84;

    /// The newest window for a user whose day is `today`. It ends two days
    /// later, so a task completed late today counts in any time zone.
    pub fn latest(today: Date) -> Self {
        let until = today
            .checked_add(jiff::Span::new().days(2))
            .unwrap_or(today);
        Self::ending(until)
    }

    /// The window just before this one.
    pub fn before(self) -> Self {
        Self::ending(self.since)
    }

    fn ending(until: Date) -> Self {
        let since = until
            .checked_sub(jiff::Span::new().days(Self::DAYS))
            .unwrap_or(Date::MIN);
        Self { since, until }
    }

    /// Whether a task completed on `day` (UTC) falls in it.
    pub fn contains(self, day: Date) -> bool {
        self.since <= day && day < self.until
    }

    /// `since` and `until` as the RFC 3339 times Todoist takes.
    pub fn bounds(self) -> (String, String) {
        let stamp = |day: Date| format!("{day}T00:00:00Z");
        (stamp(self.since), stamp(self.until))
    }
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

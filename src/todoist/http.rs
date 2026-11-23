//! The real client: Todoist's v1 API over HTTPS with reqwest.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::Stream;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use tracing::{debug, info, warn};

use super::model::{Comment, CompletedTasksResponse, Paginated, Task, UserInfo};
use super::sync::{SyncRequest, SyncResponse};
use super::{ClientError, CompletedQuery, LiveMessage, TodoistClient, live};
use crate::auth::ApiToken;

/// Todoist's v1 API.
pub const BASE_URL: &str = "https://api.todoist.com/api/v1";

/// How many times a rate-limited request is sent again before giving up.
const MAX_RETRIES: u32 = 3;
/// The most pages a cursor-paged list loads.
pub const MAX_PAGES: usize = 50;

/// Where [`HttpClient`] sends requests and how long it waits.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    /// Time allowed to open the connection.
    pub connect_timeout: Duration,
    /// Time allowed for a whole request, connection included.
    pub request_timeout: Duration,
    /// What one second of rate-limit waiting lasts: `Retry-After`, the
    /// backoff and its jitter all count in this unit. One second against
    /// Todoist; tests shrink it.
    pub retry_unit: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: BASE_URL.to_owned(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            retry_unit: Duration::from_secs(1),
        }
    }
}

/// The [`TodoistClient`] that talks to Todoist. Clones share one connection
/// pool.
#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    base_url: String,
    retry_unit: Duration,
}

impl HttpClient {
    pub fn new(token: &ApiToken) -> Result<Self> {
        Self::with_config(token, ClientConfig::default())
    }

    pub fn with_config(token: &ApiToken, config: ClientConfig) -> Result<Self> {
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", token.expose()))
            .context("invalid API token characters")?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.request_timeout)
            .connect_timeout(config.connect_timeout)
            .build()
            .context("failed to build HTTP client")?;

        info!("todoist client initialized");
        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            retry_unit: config.retry_unit,
        })
    }

    /// [`TodoistClient::sync`] with an `anyhow` error, as `app::legacy`
    /// calls it.
    pub async fn sync(&self, req: &SyncRequest) -> Result<SyncResponse> {
        Ok(self.post_sync(req).await?)
    }

    /// [`TodoistClient::comments`] with an `anyhow` error, as `app::legacy`
    /// calls it.
    pub async fn get_comments(&self, task_id: &str) -> Result<Vec<Comment>> {
        Ok(self.fetch_comments(task_id).await?)
    }

    /// [`TodoistClient::completed_tasks`] with an `anyhow` error, as
    /// `app::legacy` calls it.
    pub async fn get_completed_tasks(
        &self,
        project_id: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<Task>> {
        let query = CompletedQuery {
            project_id: project_id.map(str::to_owned),
            since: since.map(str::to_owned),
        };
        Ok(self.fetch_completed_tasks(&query).await?)
    }

    async fn post_sync(&self, body: &SyncRequest) -> Result<SyncResponse, ClientError> {
        let start = Instant::now();
        debug!(
            sync_token = %body.sync_token,
            resource_types = ?body.resource_types,
            command_count = body.commands.len(),
            "POST sync"
        );
        let url = format!("{}/sync", self.base_url);
        let sync_resp: SyncResponse = self.send(|| self.client.post(&url).json(body)).await?;
        info!(
            full_sync = sync_resp.full_sync,
            elapsed_ms = start.elapsed().as_millis() as u64,
            items = sync_resp.items.as_ref().map(Vec::len).unwrap_or(0),
            projects = sync_resp.projects.as_ref().map(Vec::len).unwrap_or(0),
            commands_processed = sync_resp.sync_status.len(),
            "sync complete"
        );
        Ok(sync_resp)
    }

    async fn fetch_comments(&self, task_id: &str) -> Result<Vec<Comment>, ClientError> {
        let start = Instant::now();
        debug!(task_id, "GET comments");
        let base = format!("{}/comments?task_id={task_id}", self.base_url);
        let all = self
            .all_pages(&base, |page: Paginated<Comment>| {
                (page.results, page.next_cursor)
            })
            .await?;
        info!(
            count = all.len(),
            task_id,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "fetched comments"
        );
        Ok(all)
    }

    /// Uses `annotate_items=1` to get the full task object (with parent_id,
    /// priority and the rest) when Todoist has one.
    async fn fetch_completed_tasks(
        &self,
        query: &CompletedQuery,
    ) -> Result<Vec<Task>, ClientError> {
        let start = Instant::now();
        let mut base = format!("{}/tasks/completed?annotate_items=1", self.base_url);
        if let Some(pid) = &query.project_id {
            base = format!("{base}&project_id={pid}");
        }
        if let Some(since) = &query.since {
            base = format!("{base}&since={since}");
        }
        let tasks = self
            .all_pages(&base, |page: CompletedTasksResponse| {
                let tasks = page.items.into_iter().map(|r| r.into_task()).collect();
                (tasks, page.next_cursor)
            })
            .await?;
        info!(
            count = tasks.len(),
            elapsed_ms = start.elapsed().as_millis() as u64,
            "fetched completed tasks"
        );
        Ok(tasks)
    }

    /// GETs `base`, then `base&cursor=...` for each cursor a page returns,
    /// up to [`MAX_PAGES`] pages.
    async fn all_pages<P, T>(
        &self,
        base: &str,
        split: impl Fn(P) -> (Vec<T>, Option<String>),
    ) -> Result<Vec<T>, ClientError>
    where
        P: DeserializeOwned,
    {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let url = match &cursor {
                Some(c) => format!("{base}&cursor={c}"),
                None => base.to_owned(),
            };
            let page: P = self.send(|| self.client.get(&url)).await?;
            let (items, next) = split(page);
            all.extend(items);
            cursor = next;
            if cursor.is_none() {
                break;
            }
        }
        if cursor.is_some() {
            warn!(max_pages = MAX_PAGES, "pagination truncated");
        }
        Ok(all)
    }

    /// Sends the request `build` makes, and again after each 429, waiting
    /// `Retry-After` (or `error_extra.retry_after`, or a doubling backoff)
    /// plus up to two units of jitter, up to [`MAX_RETRIES`] times.
    async fn send<T: DeserializeOwned>(
        &self,
        build: impl Fn() -> RequestBuilder,
    ) -> Result<T, ClientError> {
        let mut backoff_units = 1u64;
        let mut retries = 0;
        loop {
            match self.send_once(build()).await? {
                Attempt::Answered(value) => return Ok(value),
                Attempt::RateLimited { retry_after } if retries == MAX_RETRIES => {
                    return Err(ClientError::RateLimited {
                        retry_after: retry_after.map(Duration::from_secs),
                    });
                }
                Attempt::RateLimited { retry_after } => {
                    retries += 1;
                    let units = retry_after.unwrap_or(backoff_units);
                    let jitter = rand::random::<u64>() % 3;
                    warn!(retries, units, jitter, "rate limited, backing off");
                    let wait = self
                        .retry_unit
                        .saturating_mul(u32::try_from(units + jitter).unwrap_or(u32::MAX));
                    tokio::time::sleep(wait).await;
                    backoff_units = (backoff_units * 2).min(60);
                }
            }
        }
    }

    async fn send_once<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<Attempt<T>, ClientError> {
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let body = response.bytes().await.map_err(transport_error)?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let from_body = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["error_extra"]["retry_after"].as_u64());
            return Ok(Attempt::RateLimited {
                retry_after: retry_after.or(from_body),
            });
        }
        if !status.is_success() {
            warn!(status = status.as_u16(), "todoist api error");
            return Err(ClientError::Http {
                status: status.as_u16(),
                message: String::from_utf8_lossy(&body).trim().to_owned(),
            });
        }
        serde_json::from_slice(&body)
            .map(Attempt::Answered)
            .map_err(|e| ClientError::Decode(e.to_string()))
    }
}

/// What came back from one send.
enum Attempt<T> {
    Answered(T),
    /// A 429, with the wait Todoist asked for in seconds.
    RateLimited {
        retry_after: Option<u64>,
    },
}

/// A request that never got an answer: no connection, a timeout, a dropped
/// body. The URL is left out of the message.
fn transport_error(error: reqwest::Error) -> ClientError {
    if error.is_timeout() {
        return ClientError::Network("the request timed out".to_owned());
    }
    let error = error.without_url();
    // reqwest's own message is generic ("error sending request"); the causes
    // say what went wrong.
    let mut message = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        message = format!("{message}: {cause}");
        source = cause.source();
    }
    ClientError::Network(message)
}

impl TodoistClient for HttpClient {
    async fn sync(&self, request: SyncRequest) -> Result<SyncResponse, ClientError> {
        self.post_sync(&request).await
    }

    async fn user(&self) -> Result<UserInfo, ClientError> {
        let url = format!("{}/user", self.base_url);
        self.send(|| self.client.get(&url)).await
    }

    async fn comments(&self, task_id: &str) -> Result<Vec<Comment>, ClientError> {
        self.fetch_comments(task_id).await
    }

    async fn completed_tasks(&self, query: CompletedQuery) -> Result<Vec<Task>, ClientError> {
        self.fetch_completed_tasks(&query).await
    }

    fn live_events(&self, url: &str) -> impl Stream<Item = LiveMessage> + Send + 'static {
        live::events(url.to_owned())
    }
}

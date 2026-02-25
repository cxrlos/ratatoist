use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use tracing::{debug, error, info, warn};

use super::models::{Comment, CompletedTasksResponse, Paginated, Task, UserInfo};
use super::sync::{SyncRequest, SyncResponse};

const BASE_URL: &str = "https://api.todoist.com/api/v1";
const SYNC_URL: &str = "https://api.todoist.com/api/v1/sync";
const MAX_RETRIES: u32 = 3;

#[derive(Debug)]
struct RateLimitError {
    retry_after_secs: Option<u64>,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.retry_after_secs {
            Some(s) => write!(f, "rate limited (retry after {s}s)"),
            None => write!(f, "rate limited"),
        }
    }
}

impl std::error::Error for RateLimitError {}

pub struct TodoistClient {
    client: reqwest::Client,
}

impl TodoistClient {
    pub fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let auth = format!("Bearer {token}");
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth).context("invalid API token characters")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client")?;

        info!("todoist client initialized");
        Ok(Self { client })
    }

    /// All reads and writes. Retries on 429 with exponential backoff + jitter.
    pub async fn sync(&self, req: &SyncRequest) -> Result<SyncResponse> {
        self.sync_with_retry(req).await
    }

    /// Auth check on startup; also returns websocket_url.
    pub async fn get_user(&self) -> Result<UserInfo> {
        let url = format!("{BASE_URL}/user");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("failed to reach Todoist API")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Todoist API error ({status}): {body}");
        }
        resp.json().await.context("failed to parse user response")
    }

    /// Per-task comment fetch — targeted REST call, not available via Sync.
    pub async fn get_comments(&self, task_id: &str) -> Result<Vec<Comment>> {
        let url = format!("{BASE_URL}/comments?task_id={task_id}");
        let start = Instant::now();

        debug!(task_id, "GET comments");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        let page: Paginated<Comment> = resp
            .json()
            .await
            .context("failed to parse comments response")?;

        info!(
            count = page.results.len(),
            task_id,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "fetched comments"
        );
        Ok(page.results)
    }

    /// Completed tasks are not available through the Sync API.
    /// Uses `annotate_items=1` to get the full Task object (with parent_id, priority, etc.).
    pub async fn get_completed_tasks(
        &self,
        project_id: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<Task>> {
        let start = Instant::now();
        let mut url = format!("{BASE_URL}/tasks/completed?annotate_items=1");

        if let Some(pid) = project_id {
            url = format!("{url}&project_id={pid}");
        }
        if let Some(s) = since {
            url = format!("{url}&since={s}");
        }

        debug!(url = %url, "GET completed tasks");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            error!(
                status = status.as_u16(),
                elapsed_ms = elapsed.as_millis() as u64,
                "completed tasks fetch failed"
            );
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        let wrapper: CompletedTasksResponse = resp
            .json()
            .await
            .context("failed to parse completed tasks response")?;

        let tasks: Vec<Task> = wrapper
            .items
            .into_iter()
            .filter_map(|rec| {
                rec.item_object.or_else(|| {
                    Some(Task {
                        id: rec.task_id,
                        content: rec.content,
                        checked: true,
                        completed_at: Some(rec.completed_at),
                        project_id: rec.project_id,
                        section_id: rec.section_id,
                        note_count: rec.note_count,
                        user_id: rec.user_id,
                        ..Default::default()
                    })
                })
            })
            .collect();

        info!(
            count = tasks.len(),
            elapsed_ms = elapsed.as_millis() as u64,
            "fetched completed tasks"
        );
        Ok(tasks)
    }

    async fn sync_with_retry(&self, body: &SyncRequest) -> Result<SyncResponse> {
        let mut base_delay = Duration::from_secs(1);
        for attempt in 0..=MAX_RETRIES {
            match self.post_sync_once(body).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    if let Some(rate_limit) = e.downcast_ref::<RateLimitError>() {
                        let retry_secs =
                            rate_limit.retry_after_secs.unwrap_or(base_delay.as_secs());
                        let jitter: u64 = rand::random::<u64>() % 3;
                        warn!(attempt, retry_secs, jitter, "rate limited, backing off");
                        tokio::time::sleep(Duration::from_secs(retry_secs + jitter)).await;
                        base_delay = Duration::from_secs((base_delay.as_secs() * 2).min(60));
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        anyhow::bail!("rate limited after {} retries", MAX_RETRIES + 1)
    }

    async fn post_sync_once(&self, body: &SyncRequest) -> Result<SyncResponse> {
        let start = Instant::now();
        debug!(
            sync_token = %body.sync_token,
            resource_types = ?body.resource_types,
            command_count = body.commands.len(),
            "POST sync"
        );

        let resp = self
            .client
            .post(SYNC_URL)
            .json(body)
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());

            let body_text = resp.text().await.unwrap_or_default();
            let from_body = serde_json::from_str::<serde_json::Value>(&body_text)
                .ok()
                .and_then(|v| v["error_extra"]["retry_after"].as_u64());

            return Err(anyhow::Error::new(RateLimitError {
                retry_after_secs: retry_after.or(from_body),
            }));
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            error!(
                status = status.as_u16(),
                elapsed_ms = elapsed.as_millis() as u64,
                "sync api error"
            );
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        let sync_resp: SyncResponse = resp.json().await.context("failed to parse sync response")?;

        info!(
            full_sync = sync_resp.full_sync,
            elapsed_ms = elapsed.as_millis() as u64,
            items = sync_resp.items.as_ref().map(|v| v.len()).unwrap_or(0),
            projects = sync_resp.projects.as_ref().map(|v| v.len()).unwrap_or(0),
            commands_processed = sync_resp.sync_status.len(),
            "sync complete"
        );

        Ok(sync_resp)
    }
}

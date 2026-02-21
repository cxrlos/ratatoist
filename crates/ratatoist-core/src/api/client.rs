use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use tracing::{debug, error, info};

use super::models::{
    Collaborator, Comment, CreateComment, CreateLabel, CreateProject, CreateTask, Label, Paginated,
    Project, Section, Task, UpdateProject, UpdateTask,
};

const BASE_URL: &str = "https://api.todoist.com/api/v1";

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

    pub async fn get_user(&self) -> Result<super::models::UserInfo> {
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

    pub async fn get_projects(&self) -> Result<Vec<Project>> {
        let results: Vec<Project> = self.get_all_pages("projects").await?;
        info!(count = results.len(), "fetched projects");
        Ok(results)
    }

    pub async fn create_project(&self, body: &CreateProject) -> Result<Project> {
        let result: Project = self.post_json("projects", body).await?;
        info!(project_id = %result.id, "created project");
        Ok(result)
    }

    pub async fn update_project(&self, project_id: &str, body: &UpdateProject) -> Result<Project> {
        let result: Project = self
            .post_json(&format!("projects/{project_id}"), body)
            .await?;
        info!(project_id, "updated project");
        Ok(result)
    }

    pub async fn delete_project(&self, project_id: &str) -> Result<()> {
        self.delete_resource(&format!("projects/{project_id}"))
            .await?;
        info!(project_id, "deleted project");
        Ok(())
    }

    pub async fn get_collaborators(&self, project_id: &str) -> Result<Vec<Collaborator>> {
        let results: Vec<Collaborator> = self
            .get_all_pages(&format!("projects/{project_id}/collaborators"))
            .await?;
        info!(count = results.len(), project_id, "fetched collaborators");
        Ok(results)
    }

    pub async fn get_tasks(&self, project_id: Option<&str>) -> Result<Vec<Task>> {
        let endpoint = match project_id {
            Some(pid) => format!("tasks?project_id={pid}"),
            None => "tasks".to_string(),
        };
        let results: Vec<Task> = self.get_all_pages(&endpoint).await?;
        info!(
            count = results.len(),
            project_id = project_id.unwrap_or("all"),
            "fetched tasks"
        );
        Ok(results)
    }

    pub async fn create_task(&self, body: &CreateTask) -> Result<Task> {
        let result: Task = self.post_json("tasks", body).await?;
        info!(
            task_id = %result.id,
            priority = result.priority,
            has_due = result.due.is_some(),
            label_count = result.labels.len(),
            content_len = result.content.len(),
            "created task"
        );
        Ok(result)
    }

    pub async fn update_task(&self, task_id: &str, body: &UpdateTask) -> Result<Task> {
        let result: Task = self.post_json(&format!("tasks/{task_id}"), body).await?;
        info!(task_id, "updated task");
        Ok(result)
    }

    pub async fn close_task(&self, task_id: &str) -> Result<()> {
        self.post_empty(&format!("tasks/{task_id}/close")).await?;
        info!(task_id, "closed task");
        Ok(())
    }

    pub async fn reopen_task(&self, task_id: &str) -> Result<()> {
        self.post_empty(&format!("tasks/{task_id}/reopen")).await?;
        info!(task_id, "reopened task");
        Ok(())
    }

    pub async fn get_labels(&self) -> Result<Vec<Label>> {
        let results: Vec<Label> = self.get_all_pages("labels").await?;
        info!(count = results.len(), "fetched labels");
        Ok(results)
    }

    pub async fn create_label(&self, body: &CreateLabel) -> Result<Label> {
        let result: Label = self.post_json("labels", body).await?;
        info!(label_id = %result.id, "created label");
        Ok(result)
    }

    pub async fn delete_label(&self, label_id: &str) -> Result<()> {
        self.delete_resource(&format!("labels/{label_id}")).await?;
        info!(label_id, "deleted label");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_sections(&self, project_id: Option<&str>) -> Result<Vec<Section>> {
        let endpoint = match project_id {
            Some(pid) => format!("sections?project_id={pid}"),
            None => "sections".to_string(),
        };
        let results: Vec<Section> = self.get_all_pages(&endpoint).await?;
        info!(count = results.len(), "fetched sections");
        Ok(results)
    }

    pub async fn get_comments(&self, task_id: &str) -> Result<Vec<Comment>> {
        let results: Vec<Comment> = self
            .get_all_pages(&format!("comments?task_id={task_id}"))
            .await?;
        info!(count = results.len(), task_id, "fetched comments");
        Ok(results)
    }

    pub async fn create_comment(&self, body: &CreateComment) -> Result<Comment> {
        let result: Comment = self.post_json("comments", body).await?;
        info!(
            comment_id = %result.id,
            content_len = result.content.len(),
            "created comment"
        );
        Ok(result)
    }

    async fn get_all_pages<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
    ) -> Result<Vec<T>> {
        let mut all_results = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page_num = 0u32;
        let start = Instant::now();

        loop {
            page_num += 1;
            let mut url = format!("{BASE_URL}/{endpoint}");
            if let Some(ref c) = cursor {
                let sep = if url.contains('?') { '&' } else { '?' };
                url = format!("{url}{sep}cursor={c}");
            }

            debug!(endpoint, page = page_num, "requesting page");

            let resp = self
                .client
                .get(&url)
                .send()
                .await
                .context("failed to reach Todoist API")?;

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                let body_len = body.len();
                error!(
                    endpoint,
                    status = status.as_u16(),
                    response_len = body_len,
                    "api error"
                );
                anyhow::bail!("Todoist API error ({status}): {body}");
            }

            let page: Paginated<T> = resp
                .json()
                .await
                .context("failed to parse paginated response")?;

            let page_count = page.results.len();
            debug!(
                endpoint,
                page = page_num,
                items = page_count,
                "page received"
            );

            all_results.extend(page.results);

            match page.next_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }

        let elapsed = start.elapsed();
        debug!(
            endpoint,
            pages = page_num,
            total = all_results.len(),
            elapsed_ms = elapsed.as_millis() as u64,
            "pagination complete"
        );

        Ok(all_results)
    }

    async fn post_json<B: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &B,
    ) -> Result<R> {
        let url = format!("{BASE_URL}/{endpoint}");
        let start = Instant::now();

        debug!(endpoint, "POST request");

        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let body_len = body.len();
            error!(
                endpoint,
                status = status.as_u16(),
                elapsed_ms = elapsed.as_millis() as u64,
                response_len = body_len,
                "api error on POST"
            );
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        debug!(
            endpoint,
            status = status.as_u16(),
            elapsed_ms = elapsed.as_millis() as u64,
            "POST success"
        );

        resp.json().await.context("failed to parse response")
    }

    async fn post_empty(&self, endpoint: &str) -> Result<()> {
        let url = format!("{BASE_URL}/{endpoint}");
        let start = Instant::now();

        debug!(endpoint, "POST (empty body)");

        let resp = self
            .client
            .post(&url)
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let body_len = body.len();
            error!(
                endpoint,
                status = status.as_u16(),
                elapsed_ms = elapsed.as_millis() as u64,
                response_len = body_len,
                "api error on POST (empty)"
            );
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        debug!(
            endpoint,
            status = status.as_u16(),
            elapsed_ms = elapsed.as_millis() as u64,
            "POST (empty) success"
        );

        Ok(())
    }

    async fn delete_resource(&self, endpoint: &str) -> Result<()> {
        let url = format!("{BASE_URL}/{endpoint}");
        let start = Instant::now();

        debug!(endpoint, "DELETE request");

        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let body_len = body.len();
            error!(
                endpoint,
                status = status.as_u16(),
                elapsed_ms = elapsed.as_millis() as u64,
                response_len = body_len,
                "api error on DELETE"
            );
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        debug!(
            endpoint,
            status = status.as_u16(),
            elapsed_ms = elapsed.as_millis() as u64,
            "DELETE success"
        );

        Ok(())
    }
}

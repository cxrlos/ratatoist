use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use super::models::{Label, Project, Task};

const BASE_URL: &str = "https://api.todoist.com/rest/v2";

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

        Ok(Self { client })
    }

    pub async fn get_projects(&self) -> Result<Vec<Project>> {
        let resp = self
            .client
            .get(format!("{BASE_URL}/projects"))
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        resp.json()
            .await
            .context("failed to parse projects response")
    }

    pub async fn get_tasks(&self, project_id: Option<&str>) -> Result<Vec<Task>> {
        let mut req = self.client.get(format!("{BASE_URL}/tasks"));
        if let Some(pid) = project_id {
            req = req.query(&[("project_id", pid)]);
        }

        let resp = req.send().await.context("failed to reach Todoist API")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        resp.json().await.context("failed to parse tasks response")
    }

    #[allow(dead_code)]
    pub async fn get_labels(&self) -> Result<Vec<Label>> {
        let resp = self
            .client
            .get(format!("{BASE_URL}/labels"))
            .send()
            .await
            .context("failed to reach Todoist API")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Todoist API error ({status}): {body}");
        }

        resp.json().await.context("failed to parse labels response")
    }
}

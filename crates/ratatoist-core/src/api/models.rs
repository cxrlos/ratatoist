use serde::{Deserialize, Serialize};

// Todoist API v1 wraps list responses in a paginated envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct Paginated<T> {
    pub results: Vec<T>,
    pub next_cursor: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: String,
    pub parent_id: Option<String>,
    pub child_order: i32,
    pub is_shared: bool,
    pub is_favorite: bool,
    pub inbox_project: Option<bool>,
    pub is_archived: Option<bool>,
    pub is_deleted: Option<bool>,
    pub view_style: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl Project {
    pub fn is_inbox(&self) -> bool {
        self.inbox_project.unwrap_or(false)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub id: String,
    pub content: String,
    pub description: String,
    pub checked: bool,
    pub child_order: i32,
    pub priority: u8,
    pub project_id: String,
    pub section_id: Option<String>,
    pub parent_id: Option<String>,
    pub labels: Vec<String>,
    pub due: Option<Due>,
    pub deadline: Option<serde_json::Value>,
    pub duration: Option<serde_json::Value>,
    pub added_by_uid: Option<String>,
    pub added_at: Option<String>,
    pub responsible_uid: Option<String>,
    pub assigned_by_uid: Option<String>,
    pub note_count: Option<i32>,
    pub user_id: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Due {
    pub date: String,
    #[serde(default)]
    pub is_recurring: bool,
    pub timezone: Option<String>,
    pub string: Option<String>,
    pub datetime: Option<String>,
    pub lang: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
    pub item_order: Option<i32>,
    pub is_favorite: bool,
    pub is_deleted: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Section {
    pub id: String,
    pub project_id: String,
    pub section_order: Option<i32>,
    pub name: String,
    pub is_archived: Option<bool>,
    pub is_deleted: Option<bool>,
    pub is_collapsed: Option<bool>,
    pub added_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Comment {
    pub id: String,
    #[serde(default)]
    pub content: String,
    pub posted_at: Option<String>,
    #[serde(alias = "posted_uid")]
    pub posted_by_uid: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    #[serde(alias = "item_id")]
    pub item_id: Option<String>,
    #[serde(alias = "file_attachment")]
    pub attachment: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub full_name: Option<String>,
    pub email: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Collaborator {
    pub id: String,
    #[serde(alias = "full_name")]
    pub name: Option<String>,
    pub email: Option<String>,
}

// --- Request bodies for creating/updating resources ---

#[derive(Debug, Serialize)]
pub struct CreateProject {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateTask {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct CreateLabel {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateComment {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateProject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UpdateTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

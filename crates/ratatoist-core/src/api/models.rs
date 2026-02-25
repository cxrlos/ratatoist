use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
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
    pub is_collapsed: Option<bool>,
    pub view_style: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub creator_uid: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub workspace_id: Option<String>,
    pub folder_id: Option<String>,
}

impl Project {
    pub fn is_inbox(&self) -> bool {
        self.inbox_project.unwrap_or(false)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Task {
    pub id: String,
    pub content: String,
    pub description: String,
    #[serde(default)]
    pub checked: bool,
    pub child_order: i32,
    pub priority: u8,
    pub project_id: String,
    pub section_id: Option<String>,
    pub parent_id: Option<String>,
    #[serde(default)]
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
    pub updated_at: Option<String>,
    #[serde(default)]
    pub is_deleted: bool,
    pub completed_at: Option<String>,
    pub completed_by_uid: Option<String>,
    pub day_order: Option<i32>,
    #[serde(default)]
    pub is_collapsed: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
    pub item_order: Option<i32>,
    #[serde(default)]
    pub is_favorite: bool,
    pub is_deleted: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
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
#[derive(Debug, Clone, Default, Deserialize)]
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
    #[serde(default)]
    pub is_deleted: bool,
    pub reactions: Option<serde_json::Value>,
    pub uids_to_notify: Option<Vec<String>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub full_name: Option<String>,
    pub email: Option<String>,
    pub websocket_url: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Collaborator {
    pub id: String,
    #[serde(alias = "full_name")]
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompletedRecord {
    pub task_id: String,
    pub content: String,
    pub completed_at: String,
    pub project_id: String,
    pub section_id: Option<String>,
    pub note_count: Option<i32>,
    pub user_id: Option<String>,
    pub item_object: Option<Task>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub is_deleted: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub workspace_id: String,
    pub child_order: i32,
    #[serde(default)]
    pub is_deleted: bool,
}

// Priority metadata shared across all display sites.
pub const PRIORITY_LABELS: &[(u8, &str)] = &[
    (4, "P1  Urgent"),
    (3, "P2  High"),
    (2, "P3  Medium"),
    (1, "P4  Normal"),
];

pub fn priority_label(p: u8) -> &'static str {
    PRIORITY_LABELS
        .iter()
        .find(|(v, _)| *v == p)
        .map(|(_, l)| *l)
        .unwrap_or("P4  Normal")
}

// Completed tasks endpoint returns a wrapper.
#[derive(Debug, Deserialize)]
pub struct CompletedTasksResponse {
    pub items: Vec<CompletedRecord>,
}

// Paginated REST response — still used by get_completed_tasks.
#[derive(Debug, Clone, Deserialize)]
pub struct Paginated<T> {
    pub results: Vec<T>,
    pub next_cursor: Option<String>,
}

// Serializable args types used to build SyncCommand.args values.
#[derive(Debug, Serialize)]
pub struct ItemAddArgs {
    pub content: String,
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
}

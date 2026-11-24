//! The entities Todoist's v1 API sends. Every field but an entity's id has a
//! default and every enum an `Other` variant, so a field Todoist drops,
//! renames or extends never fails the sync that carries it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub child_order: i32,
    #[serde(default)]
    pub is_shared: bool,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub inbox_project: Option<bool>,
    #[serde(default)]
    pub is_archived: Option<bool>,
    #[serde(default)]
    pub is_deleted: Option<bool>,
    #[serde(default)]
    pub is_collapsed: Option<bool>,
    #[serde(default)]
    pub view_style: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub creator_uid: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
}

impl Project {
    pub fn is_inbox(&self) -> bool {
        self.inbox_project.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub child_order: i32,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub due: Option<Due>,
    #[serde(default)]
    pub deadline: Option<Deadline>,
    #[serde(default)]
    pub duration: Option<Duration>,
    #[serde(default)]
    pub added_by_uid: Option<String>,
    #[serde(default)]
    pub added_at: Option<String>,
    #[serde(default)]
    pub responsible_uid: Option<String>,
    #[serde(default)]
    pub assigned_by_uid: Option<String>,
    #[serde(default)]
    pub note_count: Option<i32>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub completed_by_uid: Option<String>,
    #[serde(default)]
    pub day_order: Option<i32>,
    #[serde(default)]
    pub is_collapsed: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Due {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub is_recurring: bool,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub string: Option<String>,
    #[serde(default)]
    pub datetime: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

/// The day a task must be done by, apart from the day it is due.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Deadline {
    /// `YYYY-MM-DD`, with no time.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub lang: Option<String>,
}

/// How long a task takes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Duration {
    #[serde(default)]
    pub amount: u32,
    #[serde(default)]
    pub unit: DurationUnit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DurationUnit {
    Minute,
    Day,
    /// A unit this version does not know.
    #[default]
    #[serde(other)]
    Other,
}

/// A reminder on a task. Which fields mean something depends on its kind:
/// `due` for an absolute one, `minute_offset` before the task's due time
/// for a relative one, the `loc_` fields for a location one.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Reminder {
    pub id: String,
    #[serde(default)]
    pub item_id: String,
    #[serde(default, rename = "type")]
    pub kind: ReminderKind,
    #[serde(default)]
    pub notify_uid: Option<String>,
    #[serde(default)]
    pub due: Option<Due>,
    #[serde(default)]
    pub minute_offset: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub loc_lat: Option<String>,
    #[serde(default)]
    pub loc_long: Option<String>,
    #[serde(default)]
    pub loc_trigger: Option<String>,
    #[serde(default)]
    pub radius: Option<i64>,
    #[serde(default)]
    pub is_urgent: bool,
    #[serde(default)]
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReminderKind {
    Relative,
    Absolute,
    Location,
    /// A kind this version does not know.
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Label {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub item_order: Option<i32>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub is_deleted: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Section {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub section_order: Option<i32>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_archived: Option<bool>,
    #[serde(default)]
    pub is_deleted: Option<bool>,
    #[serde(default)]
    pub is_collapsed: Option<bool>,
    #[serde(default)]
    pub added_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Comment {
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub posted_at: Option<String>,
    #[serde(default, alias = "posted_uid")]
    pub posted_by_uid: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default, alias = "file_attachment")]
    pub attachment: Option<serde_json::Value>,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub reactions: Option<serde_json::Value>,
    #[serde(default)]
    pub uids_to_notify: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserInfo {
    pub id: String,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub websocket_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Collaborator {
    pub id: String,
    #[serde(default, alias = "full_name")]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Workspace {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Folder {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
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

/// A task's priority as the screen shows it: p1 is the most urgent and p4
/// the default. The API numbers it the other way round (`4` is p1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    P1,
    P2,
    P3,
    P4,
}

impl Priority {
    /// Every priority, most urgent first.
    pub const ALL: [Priority; 4] = [Priority::P1, Priority::P2, Priority::P3, Priority::P4];

    /// The priority the API's `priority` number means. Anything outside 1
    /// to 4 (a missing field reads as 0) is p4.
    pub fn from_api(value: u8) -> Self {
        match value {
            4 => Priority::P1,
            3 => Priority::P2,
            2 => Priority::P3,
            _ => Priority::P4,
        }
    }

    /// The API's `priority` number for it.
    pub fn api(self) -> u8 {
        match self {
            Priority::P1 => 4,
            Priority::P2 => 3,
            Priority::P3 => 2,
            Priority::P4 => 1,
        }
    }

    /// `P1` to `P4`.
    pub fn label(self) -> &'static str {
        match self {
            Priority::P1 => "P1",
            Priority::P2 => "P2",
            Priority::P3 => "P3",
            Priority::P4 => "P4",
        }
    }
}

pub fn priority_label(p: u8) -> &'static str {
    PRIORITY_LABELS
        .iter()
        .find(|(v, _)| *v == p)
        .map(|(_, l)| *l)
        .unwrap_or("P4  Normal")
}

/// One page of `GET /tasks/completed/by_completion_date`.
#[derive(Debug, Deserialize)]
pub struct CompletedTasksResponse {
    pub items: Vec<Task>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl CompletedTasksResponse {
    /// Its tasks, each marked checked: the route lists only completed
    /// tasks, whatever their `checked` field says.
    pub fn into_tasks(self) -> Vec<Task> {
        self.items
            .into_iter()
            .map(|task| Task {
                checked: true,
                ..task
            })
            .collect()
    }
}

// One page of a cursor-paged REST list, as GET /comments returns it.
#[derive(Debug, Clone, Deserialize)]
pub struct Paginated<T> {
    pub results: Vec<T>,
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_tolerates_missing_scalar_fields() {
        // A partial/drifted item with only `id` must not abort the whole sync delta.
        let task: Task = serde_json::from_str(r#"{"id":"123"}"#).unwrap();
        assert_eq!(task.id, "123");
        assert_eq!(task.content, "");
        assert_eq!(task.description, "");
        assert_eq!(task.priority, 0);
        assert_eq!(task.child_order, 0);
        assert_eq!(task.project_id, "");
    }

    #[test]
    fn completed_response_next_cursor_is_optional() {
        let no_cursor: CompletedTasksResponse = serde_json::from_str(r#"{"items":[]}"#).unwrap();
        assert!(no_cursor.next_cursor.is_none());

        let with_cursor: CompletedTasksResponse =
            serde_json::from_str(r#"{"items":[],"next_cursor":"abc"}"#).unwrap();
        assert_eq!(with_cursor.next_cursor.as_deref(), Some("abc"));
    }

    #[test]
    fn completed_tasks_are_checked_whatever_the_item_says() {
        let page: CompletedTasksResponse =
            serde_json::from_str(r#"{"items":[{"id":"1","checked":false}]}"#).unwrap();
        assert!(page.into_tasks()[0].checked);
    }
}

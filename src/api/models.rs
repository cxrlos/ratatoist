use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: String,
    pub parent_id: Option<String>,
    pub order: i32,
    pub comment_count: i32,
    pub is_shared: bool,
    pub is_favorite: bool,
    pub is_inbox_project: bool,
    pub is_team_inbox: bool,
    pub view_style: String,
    pub url: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: String,
    pub content: String,
    pub description: String,
    pub comment_count: i32,
    pub is_completed: bool,
    pub order: i32,
    pub priority: u8,
    pub project_id: String,
    pub section_id: Option<String>,
    pub parent_id: Option<String>,
    pub labels: Vec<String>,
    pub due: Option<Due>,
    pub deadline: Option<Deadline>,
    pub duration: Option<TaskDuration>,
    pub creator_id: String,
    pub created_at: String,
    pub assignee_id: Option<String>,
    pub assigner_id: Option<String>,
    pub url: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Due {
    pub string: String,
    pub date: String,
    pub is_recurring: bool,
    pub datetime: Option<String>,
    pub timezone: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Deadline {
    pub date: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct TaskDuration {
    pub amount: u32,
    pub unit: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
    pub order: i32,
    pub is_favorite: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Section {
    pub id: String,
    pub project_id: String,
    pub order: i32,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Comment {
    pub id: String,
    pub content: String,
    pub posted_at: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub attachment: Option<Attachment>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    pub file_name: Option<String>,
    pub file_type: Option<String>,
    pub file_url: Option<String>,
    pub resource_type: String,
}

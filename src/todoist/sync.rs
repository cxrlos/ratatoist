use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SyncRequest {
    pub sync_token: String,
    pub resource_types: Vec<String>,
    pub commands: Vec<SyncCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SyncCommand {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_id: Option<String>,
    pub uuid: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
pub struct SyncResponse {
    pub full_sync: bool,
    /// When a full sync's data was generated, which on a big account may be
    /// a while before the answer.
    pub full_sync_date_utc: Option<String>,
    pub sync_token: String,
    pub items: Option<Vec<super::model::Task>>,
    pub projects: Option<Vec<super::model::Project>>,
    pub sections: Option<Vec<super::model::Section>>,
    pub labels: Option<Vec<super::model::Label>>,
    pub notes: Option<Vec<super::model::Comment>>,
    pub collaborators: Option<Vec<super::model::Collaborator>>,
    pub workspaces: Option<Vec<super::model::Workspace>>,
    pub folders: Option<Vec<super::model::Folder>>,
    pub reminders: Option<Vec<super::model::Reminder>>,
    pub collaborator_states: Option<Vec<CollaboratorState>>,
    pub user: Option<super::model::UserInfo>,
    #[serde(default)]
    pub sync_status: HashMap<String, SyncCommandResult>,
    #[serde(default)]
    pub temp_id_mapping: HashMap<String, String>,
}

impl SyncResponse {
    /// A response that changes nothing, carrying a placeholder sync token.
    pub fn empty(full_sync: bool) -> Self {
        Self {
            full_sync,
            sync_token: "fake".to_owned(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SyncCommandResult {
    Ok(String),
    Err(SyncCommandError),
}

impl SyncCommandResult {
    pub fn is_err(&self) -> bool {
        matches!(self, SyncCommandResult::Err(_))
    }

    pub fn error_message(&self) -> Option<&str> {
        match self {
            SyncCommandResult::Err(e) => Some(&e.error),
            SyncCommandResult::Ok(_) => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncCommandError {
    pub error_code: i32,
    pub error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CollaboratorState {
    pub project_id: String,
    pub user_id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub is_deleted: bool,
}

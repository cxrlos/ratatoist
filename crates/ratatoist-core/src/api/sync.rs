use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SyncRequest {
    pub sync_token: String,
    pub resource_types: Vec<String>,
    pub commands: Vec<SyncCommand>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncCommand {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_id: Option<String>,
    pub uuid: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct SyncResponse {
    pub full_sync: bool,
    pub sync_token: String,
    pub items: Option<Vec<super::models::Task>>,
    pub projects: Option<Vec<super::models::Project>>,
    pub sections: Option<Vec<super::models::Section>>,
    pub labels: Option<Vec<super::models::Label>>,
    pub notes: Option<Vec<super::models::Comment>>,
    pub collaborators: Option<Vec<super::models::Collaborator>>,
    pub workspaces: Option<Vec<super::models::Workspace>>,
    pub folders: Option<Vec<super::models::Folder>>,
    pub collaborator_states: Option<Vec<CollaboratorState>>,
    pub user: Option<super::models::UserInfo>,
    #[serde(default)]
    pub sync_status: HashMap<String, SyncCommandResult>,
    #[serde(default)]
    pub temp_id_mapping: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct SyncCommandError {
    pub error_code: i32,
    pub error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CollaboratorState {
    pub project_id: String,
    pub user_id: String,
    pub state: String,
    #[serde(default)]
    pub is_deleted: bool,
}

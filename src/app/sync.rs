//! Merging sync responses into [`State`], and optimistic writes with their
//! rollback.

use std::collections::HashSet;

use tracing::error;

use super::Effect;
use super::state::{AppError, State, UserRecord};
use crate::todoist::ClientError;
use crate::todoist::model::{Folder, Project};
use crate::todoist::sync::{SyncCommand, SyncRequest, SyncResponse};

/// What local state looked like before an optimistic write, so it can be
/// rolled back if Todoist rejects the command or it never arrives.
#[derive(Debug, Clone)]
pub enum OptimisticOp {
    TaskAdded {
        temp_id: String,
    },
    TaskUpdated {
        task_id: String,
        before: Box<crate::todoist::model::Task>,
    },
    CommentAdded {
        temp_id: String,
        task_id: String,
    },
    ProjectUpdated {
        project_id: String,
        before: Box<Project>,
    },
}

/// What a full sync reads: at startup and on a forced re-sync.
const FULL_SYNC_RESOURCES: [&str; 9] = [
    "items",
    "projects",
    "sections",
    "labels",
    "notes",
    "collaborators",
    "workspaces",
    "folders",
    "user",
];

/// What an incremental sync reads.
const INCREMENTAL_SYNC_RESOURCES: [&str; 5] = ["items", "projects", "sections", "labels", "notes"];

/// Everything, from sync token `*`.
pub(crate) fn full_sync_request() -> SyncRequest {
    SyncRequest {
        sync_token: "*".to_string(),
        resource_types: FULL_SYNC_RESOURCES.map(String::from).to_vec(),
        commands: vec![],
    }
}

/// What changed since `sync_token`.
pub(crate) fn incremental_sync_request(sync_token: &str) -> SyncRequest {
    SyncRequest {
        sync_token: sync_token.to_owned(),
        resource_types: INCREMENTAL_SYNC_RESOURCES.map(String::from).to_vec(),
        commands: vec![],
    }
}

/// A random version 4 uuid, so a command never repeats one Todoist has seen.
pub(crate) fn new_uuid() -> String {
    let bits: u128 = rand::random();
    let bits = (bits & !(0xf << 76)) | (0x4 << 76);
    let bits = (bits & !(0x3 << 62)) | (0x2 << 62);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        bits >> 96,
        (bits >> 80) & 0xffff,
        (bits >> 64) & 0xffff,
        (bits >> 48) & 0xffff,
        bits & 0xffff_ffff_ffff
    )
}

impl State {
    pub(crate) fn new_temp_id(&mut self) -> String {
        self.next_temp_id += 1;
        format!("tmp_{}", self.next_temp_id)
    }

    /// Records `op` under the command's uuid and asks the runtime to send the
    /// command, alone. One command per flush: the rollback snapshots are
    /// absolute `before` values that do not compose when two edits of the
    /// same task share a flush.
    pub(crate) fn send_command(
        &mut self,
        command: SyncCommand,
        op: Option<OptimisticOp>,
        effects: &mut Vec<Effect>,
    ) {
        if let Some(op) = op {
            self.temp_id_pending.insert(command.uuid.clone(), op);
        }
        effects.push(Effect::SendCommands(vec![command]));
    }

    /// True if an optimistic op for this task is still awaiting its command result.
    pub(crate) fn task_has_pending_op(&self, task_id: &str) -> bool {
        self.temp_id_pending.values().any(|op| match op {
            OptimisticOp::TaskUpdated { task_id: id, .. } => id == task_id,
            OptimisticOp::TaskAdded { temp_id } => temp_id == task_id,
            OptimisticOp::CommentAdded { .. } | OptimisticOp::ProjectUpdated { .. } => false,
        })
    }

    /// Whether a command with `uuid` is still waiting for Todoist.
    pub fn is_pending(&self, uuid: &str) -> bool {
        self.temp_id_pending.contains_key(uuid)
    }

    pub(crate) fn synced(
        &mut self,
        result: Result<Box<SyncResponse>, ClientError>,
        effects: &mut Vec<Effect>,
    ) {
        let first = std::mem::replace(&mut self.loading, false);
        match result {
            Ok(resp) => {
                self.apply_sync_delta(*resp, effects);
                if first
                    && !self.live_opened
                    && let Some(url) = self.websocket_url.clone()
                {
                    self.live_opened = true;
                    effects.push(Effect::OpenLive { url });
                }
            }
            Err(e) if first => self.set_error(&e, "Initial sync"),
            Err(e) => error!(error = %e, "incremental sync failed"),
        }
    }

    pub(crate) fn apply_sync_delta(&mut self, resp: SyncResponse, effects: &mut Vec<Effect>) {
        if resp.full_sync {
            if let Some(projects) = resp.projects {
                self.projects = projects
                    .into_iter()
                    .filter(|p| !p.is_deleted.unwrap_or(false))
                    .collect();
                self.sort_projects();
            }
            if let Some(items) = resp.items {
                self.tasks = items.into_iter().filter(|t| !t.is_deleted).collect();
            }
            if let Some(labels) = resp.labels {
                self.labels = labels
                    .into_iter()
                    .filter(|l| !l.is_deleted.unwrap_or(false))
                    .collect();
            }
            if let Some(sections) = resp.sections {
                self.sections = sections
                    .into_iter()
                    .filter(|s| !s.is_deleted.unwrap_or(false))
                    .collect();
            }
            if let Some(notes) = resp.notes {
                self.comments_by_task.clear();
                for note in notes {
                    if !note.is_deleted {
                        let tid = note
                            .item_id
                            .clone()
                            .or_else(|| note.task_id.clone())
                            .unwrap_or_default();
                        self.comments_by_task.entry(tid).or_default().push(note);
                    }
                }
            }
            if let Some(collabs) = resp.collaborators {
                for c in collabs {
                    self.user_names
                        .entry(c.id.clone())
                        .or_insert_with(|| UserRecord::new(c.id, c.name, c.email));
                }
            }
            if let Some(workspaces) = resp.workspaces {
                self.workspaces = workspaces.into_iter().filter(|w| !w.is_deleted).collect();
            }
            if let Some(folders) = resp.folders {
                self.folders = folders.into_iter().filter(|f| !f.is_deleted).collect();
            }
            if let Some(user) = resp.user {
                self.current_user_id = Some(user.id.clone());
                self.websocket_url = user.websocket_url;
                if let Some(name) = &user.full_name {
                    self.current_user_name = Some(name.clone());
                }
                self.user_names
                    .entry(user.id.clone())
                    .or_insert_with(|| UserRecord::new(user.id, user.full_name, user.email));
            }
        } else {
            if let Some(projects) = resp.projects {
                for p in projects {
                    if p.is_deleted.unwrap_or(false) {
                        self.projects.retain(|e| e.id != p.id);
                    } else if let Some(e) = self.projects.iter_mut().find(|e| e.id == p.id) {
                        *e = p;
                    } else {
                        self.projects.push(p);
                    }
                }
                self.sort_projects();
            }
            if let Some(items) = resp.items {
                for item in items {
                    // A racing server delta must not clobber a task the user is still
                    // editing optimistically: skip it until the command resolves.
                    if self.task_has_pending_op(&item.id) {
                        continue;
                    }
                    if item.is_deleted {
                        self.tasks.retain(|t| t.id != item.id);
                    } else if let Some(e) = self.tasks.iter_mut().find(|t| t.id == item.id) {
                        *e = item;
                    } else {
                        self.tasks.push(item);
                    }
                }
            }
            if let Some(labels) = resp.labels {
                for l in labels {
                    if l.is_deleted.unwrap_or(false) {
                        self.labels.retain(|e| e.id != l.id);
                    } else if let Some(e) = self.labels.iter_mut().find(|e| e.id == l.id) {
                        *e = l;
                    } else {
                        self.labels.push(l);
                    }
                }
            }
            if let Some(sections) = resp.sections {
                for s in sections {
                    if s.is_deleted.unwrap_or(false) {
                        self.sections.retain(|e| e.id != s.id);
                    } else if let Some(e) = self.sections.iter_mut().find(|e| e.id == s.id) {
                        *e = s;
                    } else {
                        self.sections.push(s);
                    }
                }
            }
            if let Some(notes) = resp.notes {
                let open_task_id = self.selected_task().map(|t| t.id.clone());
                let mut affected_task: Option<String> = None;
                for note in notes {
                    let tid = note
                        .item_id
                        .clone()
                        .or_else(|| note.task_id.clone())
                        .unwrap_or_default();
                    if note.is_deleted {
                        if let Some(list) = self.comments_by_task.get_mut(&tid) {
                            list.retain(|c| c.id != note.id);
                        }
                    } else if let Some(list) = self.comments_by_task.get_mut(&tid) {
                        if let Some(c) = list.iter_mut().find(|c| c.id == note.id) {
                            *c = note;
                        } else {
                            list.push(note);
                        }
                    } else {
                        self.comments_by_task.insert(tid.clone(), vec![note]);
                    }
                    if open_task_id.as_deref() == Some(&tid) {
                        affected_task = Some(tid);
                    }
                }
                if let Some(tid) = affected_task
                    && let Some(updated) = self.comments_by_task.get(&tid)
                {
                    self.comments = updated.clone();
                }
            }
        }

        self.take_sync_token(resp.sync_token, effects);
        self.last_sync_at = Some(self.now);
        self.clamp_selected_task();
    }

    fn take_sync_token(&mut self, token: String, effects: &mut Vec<Effect>) {
        if !token.is_empty() {
            self.sync_token = token.clone();
            effects.push(Effect::PersistSyncToken(token));
        }
    }

    pub(crate) fn clamp_selected_task(&mut self) {
        let visible_len = self.visible_tasks().len();
        if visible_len == 0 {
            self.selected_task = 0;
        } else if self.selected_task >= visible_len {
            self.selected_task = visible_len - 1;
        }
    }

    pub(crate) fn commands_sent(
        &mut self,
        uuids: &[String],
        result: Result<Box<SyncResponse>, ClientError>,
        effects: &mut Vec<Effect>,
    ) {
        match result {
            Ok(resp) => self.command_results(*resp, effects),
            Err(e) => {
                error!(error = %e, "command flush failed");
                self.command_failed(uuids);
            }
        }
    }

    fn command_results(&mut self, resp: SyncResponse, effects: &mut Vec<Effect>) {
        let mut refresh_comments_for: Option<String> = None;
        for (uuid, status) in &resp.sync_status {
            if status.is_err() {
                if let Some(op) = self.temp_id_pending.remove(uuid) {
                    self.revert_optimistic(op);
                }
                let msg = status
                    .error_message()
                    .unwrap_or("unknown error")
                    .to_string();
                error!(uuid, error = %msg, "command rejected by server");
                self.error = Some(AppError {
                    title: "Command failed".to_string(),
                    message: msg,
                    suggestion: None,
                    recoverable: true,
                });
            } else if let Some(op) = self.temp_id_pending.remove(uuid)
                && let OptimisticOp::CommentAdded { task_id, .. } = &op
            {
                let current = self.selected_task().map(|t| t.id.clone());
                if current.as_deref() == Some(task_id.as_str()) {
                    refresh_comments_for = Some(task_id.clone());
                }
            }
        }
        for (temp_id, real_id) in &resp.temp_id_mapping {
            self.apply_temp_id_mapping(temp_id, real_id);
        }
        self.take_sync_token(resp.sync_token, effects);
        if let Some(tid) = refresh_comments_for {
            self.fetch_comments(tid, effects);
        }
    }

    fn command_failed(&mut self, uuids: &[String]) {
        let mut reverted = false;
        for uuid in uuids {
            if let Some(op) = self.temp_id_pending.remove(uuid) {
                self.revert_optimistic(op);
                reverted = true;
            }
        }
        if reverted {
            self.error = Some(AppError {
                title: "Sync failed".to_string(),
                message: "Couldn't reach Todoist: your change was reverted.".to_string(),
                suggestion: Some("Check your connection and try again.".to_string()),
                recoverable: true,
            });
        }
    }

    fn apply_temp_id_mapping(&mut self, temp_id: &str, real_id: &str) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == temp_id) {
            t.id = real_id.to_string();
        }
        for c in &mut self.comments {
            if c.id == temp_id {
                c.id = real_id.to_string();
            }
            if c.item_id.as_deref() == Some(temp_id) {
                c.item_id = Some(real_id.to_string());
            }
        }
    }

    fn revert_optimistic(&mut self, op: OptimisticOp) {
        match op {
            OptimisticOp::TaskAdded { temp_id } => {
                self.tasks.retain(|t| t.id != temp_id);
            }
            OptimisticOp::TaskUpdated { task_id, before } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    *t = *before;
                }
            }
            OptimisticOp::CommentAdded { temp_id, task_id } => {
                let current = self.selected_task().map(|t| t.id.clone());
                if current.as_deref() == Some(&task_id) {
                    self.comments.retain(|c| c.id != temp_id);
                }
            }
            OptimisticOp::ProjectUpdated { project_id, before } => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
                    *p = *before;
                }
                self.sort_projects();
            }
        }
    }

    /// Recovery path for a suspected desync: abandon any in-flight optimistic
    /// state and read everything the startup sync reads. Dropping
    /// `temp_id_pending` is deliberate: the incoming full sync replaces the
    /// task list wholesale, so a late command result must not revert against it.
    pub(crate) fn force_full_resync(&mut self, effects: &mut Vec<Effect>) {
        self.temp_id_pending.clear();
        let request = full_sync_request();
        self.sync_token = request.sync_token.clone();
        effects.push(Effect::PersistSyncToken(self.sync_token.clone()));
        effects.push(Effect::Sync(request));
    }

    pub(crate) fn fetch_comments(&mut self, task_id: String, effects: &mut Vec<Effect>) {
        self.comments_fetch_seq += 1;
        effects.push(Effect::FetchComments {
            task_id,
            fetch_seq: self.comments_fetch_seq,
        });
    }

    pub(crate) fn comments_loaded(
        &mut self,
        task_id: String,
        fetch_seq: u64,
        result: Result<Vec<crate::todoist::model::Comment>, ClientError>,
    ) {
        match result {
            Ok(c) => {
                let count = i32::try_from(c.len()).unwrap_or(i32::MAX);
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.note_count = Some(count);
                }
                self.comments_by_task.insert(task_id.clone(), c.clone());
                let current_tid = self.selected_task().map(|t| t.id.clone());
                if current_tid.as_deref() == Some(&task_id) && fetch_seq == self.comments_fetch_seq
                {
                    self.comments = c;
                }
            }
            Err(e) => self.set_error(&e, "Load comments"),
        }
    }

    pub(crate) fn completed_loaded(
        &mut self,
        project_id: Option<String>,
        result: Result<Vec<crate::todoist::model::Task>, ClientError>,
    ) {
        match result {
            Ok(r) => {
                self.completed_cache
                    .insert(project_id.unwrap_or_default(), r);
            }
            Err(e) => self.set_error(&e, "Load completed tasks"),
        }
    }

    pub(crate) fn set_error(&mut self, err: &ClientError, context: &str) {
        let app_err = AppError::from_client(err, context);
        error!(context, error = %app_err.message, "app error");
        self.error = Some(app_err);
    }

    pub(crate) fn sort_projects(&mut self) {
        let selected_id = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.clone());
        let source = self.projects.clone();
        let mut ordered: Vec<Project> = Vec::with_capacity(source.len());

        let personal: Vec<Project> = source
            .iter()
            .filter(|p| p.workspace_id.is_none())
            .cloned()
            .collect();
        collect_project_subtree(None, &personal, &mut ordered);

        for ws in &self.workspaces {
            let ws_projects: Vec<Project> = source
                .iter()
                .filter(|p| p.workspace_id.as_deref() == Some(ws.id.as_str()))
                .cloned()
                .collect();
            if ws_projects.is_empty() {
                continue;
            }

            let no_folder: Vec<Project> = ws_projects
                .iter()
                .filter(|p| p.folder_id.is_none())
                .cloned()
                .collect();
            collect_project_subtree(None, &no_folder, &mut ordered);

            let mut ws_folders: Vec<&Folder> = self
                .folders
                .iter()
                .filter(|f| f.workspace_id == ws.id)
                .collect();
            ws_folders.sort_by_key(|f| f.child_order);

            for folder in ws_folders {
                let in_folder: Vec<Project> = ws_projects
                    .iter()
                    .filter(|p| p.folder_id.as_deref() == Some(folder.id.as_str()))
                    .cloned()
                    .collect();
                collect_project_subtree(None, &in_folder, &mut ordered);
            }
        }

        let ordered_ids: HashSet<String> = ordered.iter().map(|p| p.id.clone()).collect();
        for p in &source {
            if !ordered_ids.contains(&p.id) {
                ordered.push(p.clone());
            }
        }

        self.projects = ordered;
        if let Some(id) = selected_id
            && let Some(pos) = self.projects.iter().position(|p| p.id == id)
        {
            self.selected_project = pos;
        }
    }
}

fn collect_project_subtree(parent_id: Option<&str>, all: &[Project], out: &mut Vec<Project>) {
    let mut children: Vec<&Project> = all
        .iter()
        .filter(|p| p.parent_id.as_deref() == parent_id)
        .collect();
    children.sort_by(|a, b| {
        let a_pin = a.is_inbox() || a.is_favorite;
        let b_pin = b.is_inbox() || b.is_favorite;
        b_pin.cmp(&a_pin).then(a.child_order.cmp(&b.child_order))
    });
    for child in children {
        out.push(child.clone());
        collect_project_subtree(Some(&child.id), all, out);
    }
}

#[cfg(test)]
mod tests {
    use super::new_uuid;

    #[test]
    fn uuids_are_version_4_and_differ() {
        let a = new_uuid();
        let b = new_uuid();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
    }
}

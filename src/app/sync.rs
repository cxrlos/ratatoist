//! Merging sync responses into [`State`], and optimistic writes with their
//! rollback.

use std::collections::HashSet;

use tracing::error;

use super::Effect;
use super::failure::{ErrorKind, Failure, LoadError};
use super::state::{Pane, State, SyncHealth, UserRecord};
use crate::todoist::model::{Folder, Project, Task};
use crate::todoist::sync::{SyncCommand, SyncCommandResult, SyncRequest, SyncResponse};
use crate::todoist::{ClientError, CompletedQuery};

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
    /// same task share a flush. A new write to a task clears its failed
    /// mark.
    pub(crate) fn send_command(
        &mut self,
        command: SyncCommand,
        op: Option<OptimisticOp>,
        effects: &mut Vec<Effect>,
    ) {
        let action = self.describe(&command, op.as_ref());
        if let Some(task_id) = command_task(&command) {
            self.rolled_back.remove(task_id);
        }
        self.pending_actions.insert(command.uuid.clone(), action);
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
        self.sync_health = match &result {
            Ok(_) => SyncHealth::Fine,
            Err(ClientError::Network(_)) => SyncHealth::Offline,
            Err(_) => SyncHealth::Failing,
        };
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
            Err(e) if first && ErrorKind::of(&e, self.now) == ErrorKind::TokenRejected => {
                error!("todoist rejected the token");
                self.not_loaded(LoadError::TokenRejected);
            }
            Err(e) if first => self.report(Failure::from_client("load your tasks", &e, self.now)),
            Err(e) => self.log_failure(Failure::from_client("sync with Todoist", &e, self.now)),
        }
    }

    pub(crate) fn apply_sync_delta(&mut self, resp: SyncResponse, effects: &mut Vec<Effect>) {
        let focused = self.selected_task().map(|task| task.id.clone());
        if resp.full_sync {
            if let Some(projects) = resp.projects {
                self.projects = projects
                    .into_iter()
                    .filter(|p| !p.is_deleted.unwrap_or(false))
                    .collect();
                self.sort_projects();
            }
            if let Some(items) = resp.items {
                let before = std::mem::take(&mut self.tasks);
                self.tasks = items.into_iter().filter(|t| !t.is_deleted).collect();
                self.keep_marks_of_unchanged_tasks(&before);
                // A full sync carries every due date as Todoist parsed it.
                self.pending_due.clear();
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
                    self.rolled_back.remove(&item.id);
                    self.pending_due.remove(&item.id);
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
                let open_task_id = focused.clone();
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
        self.keep_focus_on(focused, effects);
    }

    /// Focuses the task `focused` again when it is still visible, else the
    /// nearest row. When the detail shows and now shows another task, its
    /// comments load as opening it would.
    fn keep_focus_on(&mut self, focused: Option<String>, effects: &mut Vec<Effect>) {
        let position = focused
            .as_ref()
            .and_then(|id| self.visible_tasks().iter().position(|task| &task.id == id));
        match position {
            Some(position) => self.selected_task = position,
            None => self.clamp_selected_task(),
        }
        let detail_shows = self.shows_panes() || self.active_pane == Pane::Detail;
        let now = self.selected_task().map(|task| task.id.clone());
        if !detail_shows || focused.is_none() || now == focused {
            return;
        }
        let Some(task_id) = now else {
            return;
        };
        self.detail_scroll = 0;
        match self.comments_by_task.get(&task_id) {
            Some(cached) => self.comments = cached.clone(),
            None => self.comments.clear(),
        }
        self.fetch_comments(task_id, effects);
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
            Err(e) => self.command_failed(uuids, &e),
        }
    }

    fn command_results(&mut self, resp: SyncResponse, effects: &mut Vec<Effect>) {
        let mut refresh_comments_for: Option<String> = None;
        let mut rejected = Vec::new();
        for (uuid, status) in &resp.sync_status {
            let action = self.pending_actions.remove(uuid);
            match status {
                SyncCommandResult::Err(error) => {
                    let action = action.unwrap_or_else(|| "send a change".to_owned());
                    rejected.push((uuid.clone(), Failure::from_command(action, error, self.now)));
                }
                SyncCommandResult::Ok(_) => {
                    if let Some(op) = self.temp_id_pending.remove(uuid)
                        && let OptimisticOp::CommentAdded { task_id, .. } = &op
                    {
                        let current = self.selected_task().map(|t| t.id.clone());
                        if current.as_deref() == Some(task_id.as_str()) {
                            refresh_comments_for = Some(task_id.clone());
                        }
                    }
                }
            }
        }
        for (uuid, failure) in rejected {
            self.roll_back(&uuid, &failure);
            self.report(failure);
        }
        for (temp_id, real_id) in &resp.temp_id_mapping {
            self.apply_temp_id_mapping(temp_id, real_id);
        }
        self.take_sync_token(resp.sync_token, effects);
        if let Some(tid) = refresh_comments_for {
            self.fetch_comments(tid, effects);
        }
    }

    /// The flush carrying `uuids` failed with `error`: each of its writes
    /// rolls back and becomes the notice and a log entry.
    fn command_failed(&mut self, uuids: &[String], error: &ClientError) {
        let mut failures = Vec::new();
        for uuid in uuids {
            let action = self
                .pending_actions
                .remove(uuid)
                .unwrap_or_else(|| "send a change".to_owned());
            let failure = Failure::from_client(action, error, self.now);
            self.roll_back(uuid, &failure);
            failures.push(failure);
        }
        if failures.is_empty() {
            failures.push(Failure::from_client("send a change", error, self.now));
        }
        for failure in failures {
            self.report(failure);
        }
    }

    /// Restores what the write with `uuid` changed, and marks its task or
    /// project with `failure`.
    fn roll_back(&mut self, uuid: &str, failure: &Failure) {
        let Some(op) = self.temp_id_pending.remove(uuid) else {
            return;
        };
        if let OptimisticOp::ProjectUpdated { project_id, .. } = &op {
            self.rolled_back_projects
                .insert(project_id.clone(), failure.clone());
        }
        if let Some(task_id) = self.revert_optimistic(op) {
            self.pending_due.remove(&task_id);
            self.rolled_back.insert(task_id, failure.clone());
        }
    }

    /// The command as the log names it, such as `complete "Buy oat milk"`,
    /// from the task or project as it was before `op` changed it.
    fn describe(&self, command: &SyncCommand, op: Option<&OptimisticOp>) -> String {
        let quoted = |name: &str| format!("\"{name}\"");
        if command.r#type == "item_add" {
            let content = command.args["content"].as_str().unwrap_or_default();
            return format!("add {}", quoted(content));
        }
        if command.r#type == "project_update" {
            let name = match op {
                Some(OptimisticOp::ProjectUpdated { before, .. }) => Some(before.name.clone()),
                _ => command.args["id"]
                    .as_str()
                    .and_then(|id| self.projects.iter().find(|p| p.id == id))
                    .map(|p| p.name.clone()),
            };
            return name.map_or_else(
                || "edit a project".to_owned(),
                |name| format!("edit project {}", quoted(&name)),
            );
        }
        let verb = match command.r#type.as_str() {
            "item_close" | "item_complete" => "complete",
            "item_reopen" | "item_uncomplete" => "reopen",
            "item_update" => "edit",
            "item_delete" => "delete",
            "item_move" => "move",
            "note_add" => "comment on",
            other => return other.replace('_', " "),
        };
        let content = match op {
            Some(OptimisticOp::TaskUpdated { before, .. }) => Some(before.content.clone()),
            _ => command_task(command)
                .and_then(|id| self.tasks.iter().find(|t| t.id == id))
                .map(|t| t.content.clone()),
        };
        match content {
            Some(content) => format!("{verb} {}", quoted(&content)),
            None => format!("{verb} a task"),
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

    /// Undoes `op`; the task whose row it changed, if it still has one.
    fn revert_optimistic(&mut self, op: OptimisticOp) -> Option<String> {
        match op {
            OptimisticOp::TaskAdded { temp_id } => {
                self.tasks.retain(|t| t.id != temp_id);
                None
            }
            OptimisticOp::TaskUpdated { task_id, before } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    *t = *before;
                }
                Some(task_id)
            }
            OptimisticOp::CommentAdded { temp_id, task_id } => {
                let current = self.selected_task().map(|t| t.id.clone());
                if current.as_deref() == Some(&task_id) {
                    self.comments.retain(|c| c.id != temp_id);
                }
                Some(task_id)
            }
            OptimisticOp::ProjectUpdated { project_id, before } => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
                    *p = *before;
                }
                self.sort_projects();
                None
            }
        }
    }

    /// Drops the failed mark of every task a full sync changed or removed,
    /// comparing each task with its copy in `before`.
    fn keep_marks_of_unchanged_tasks(&mut self, before: &[Task]) {
        let tasks = &self.tasks;
        self.rolled_back.retain(|id, _| {
            let old = before.iter().find(|t| &t.id == id);
            let new = tasks.iter().find(|t| &t.id == id);
            match (old, new) {
                (Some(old), Some(new)) => {
                    serde_json::to_value(old).ok() == serde_json::to_value(new).ok()
                }
                _ => false,
            }
        });
    }

    /// Recovery path for a suspected desync: abandon any in-flight optimistic
    /// state and read everything the startup sync reads. Dropping
    /// `temp_id_pending` is deliberate: the incoming full sync replaces the
    /// task list wholesale, so a late command result must not revert against it.
    pub(crate) fn force_full_resync(&mut self, effects: &mut Vec<Effect>) {
        self.temp_id_pending.clear();
        self.pending_actions.clear();
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
            Err(e) => {
                let action = match self.tasks.iter().find(|t| t.id == task_id) {
                    Some(task) => format!("load the comments on \"{}\"", task.content),
                    None => "load comments".to_owned(),
                };
                self.log_failure(Failure::from_client(action, &e, self.now));
            }
        }
    }

    /// Adds the tasks of one window to the project's completed tasks, and
    /// says how far back they go when the window is an older one.
    pub(crate) fn completed_loaded(
        &mut self,
        query: CompletedQuery,
        result: Result<Vec<crate::todoist::model::Task>, ClientError>,
    ) {
        match result {
            Ok(loaded) => {
                let pid = query.project_id.unwrap_or_default();
                let tasks = self.completed_cache.entry(pid.clone()).or_default();
                for task in loaded {
                    if !tasks.iter().any(|t| t.id == task.id) {
                        tasks.push(task);
                    }
                }
                let oldest = self.oldest_completed.entry(pid).or_insert(query.window);
                if query.window.since < oldest.since {
                    *oldest = query.window;
                    self.notice = Some(format!(
                        "Completed tasks loaded back to {}",
                        query.window.since
                    ));
                }
            }
            Err(e) => {
                self.log_failure(Failure::from_client("load completed tasks", &e, self.now));
            }
        }
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

/// The task `command` writes to, if it names one.
fn command_task(command: &SyncCommand) -> Option<&str> {
    let args = &command.args;
    args.get("item_id")
        .or_else(|| args.get("id"))
        .and_then(serde_json::Value::as_str)
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

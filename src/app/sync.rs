//! Merging sync responses into [`State`], and optimistic writes with their
//! rollback.

use std::collections::HashSet;

use tracing::{error, info};

use super::Effect;
use super::failure::{ErrorKind, Failure, LoadError};
use super::state::{Pane, State, SyncHealth};
use super::store::{Store, Table};
use super::view::{cycles, parents};
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

impl OptimisticOp {
    /// The task whose row a pending sync delta must not overwrite.
    fn task(&self) -> Option<&str> {
        match self {
            OptimisticOp::TaskUpdated { task_id, .. } => Some(task_id),
            OptimisticOp::TaskAdded { temp_id } => Some(temp_id),
            OptimisticOp::CommentAdded { .. } | OptimisticOp::ProjectUpdated { .. } => None,
        }
    }
}

/// What a full sync reads: at startup and on a forced re-sync.
const FULL_SYNC_RESOURCES: [&str; 10] = [
    "items",
    "projects",
    "sections",
    "labels",
    "notes",
    "collaborators",
    "workspaces",
    "folders",
    "reminders",
    "user",
];

/// What an incremental sync reads.
const INCREMENTAL_SYNC_RESOURCES: [&str; 6] = [
    "items",
    "projects",
    "sections",
    "labels",
    "notes",
    "reminders",
];

/// How long after a merge the cache is written, so a burst of merges
/// writes it once.
pub const CACHE_DEBOUNCE: jiff::SignedDuration = jiff::SignedDuration::from_secs(2);

/// Everything, from sync token `*`.
pub(crate) fn full_sync_request() -> SyncRequest {
    startup_sync_request("*")
}

/// What the first sync reads: everything a full sync does, since
/// `sync_token`, which is `*` unless the cache painted the screen.
pub(crate) fn startup_sync_request(sync_token: &str) -> SyncRequest {
    SyncRequest {
        sync_token: sync_token.to_owned(),
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

    /// Whether a command with `uuid` is still waiting for Todoist.
    pub fn is_pending(&self, uuid: &str) -> bool {
        self.temp_id_pending.contains_key(uuid)
    }

    pub(crate) fn synced(
        &mut self,
        result: Result<Box<SyncResponse>, ClientError>,
        effects: &mut Vec<Effect>,
    ) {
        let cached_user = self.cache_user_id.take();
        let first = std::mem::replace(&mut self.loading, false) || cached_user.is_some();
        self.sync_health = match &result {
            Ok(_) => SyncHealth::Fine,
            Err(ClientError::Network(_)) => SyncHealth::Offline,
            Err(_) => SyncHealth::Failing,
        };
        match result {
            Ok(resp) => {
                if let Some(cached) = cached_user
                    && resp.user.as_ref().is_some_and(|user| user.id != cached)
                {
                    info!("the cache belongs to another user; full sync starting");
                    self.discard_cache(effects);
                    return;
                }
                // A big account's full sync may carry data generated a while
                // before; one incremental sync catches up.
                let dated = resp.full_sync && resp.full_sync_date_utc.is_some();
                let first_success = self.last_sync_at.is_none();
                self.apply_sync_delta(*resp, effects);
                if dated {
                    effects.push(Effect::Sync(incremental_sync_request(&self.sync_token)));
                }
                if first_success {
                    effects.push(Effect::ForgetOldSyncState);
                }
                if first
                    && !self.live_opened
                    && let Some(url) = self.websocket_url.clone()
                {
                    self.live_opened = true;
                    effects.push(Effect::OpenLive { url });
                }
            }
            Err(e) if first && ErrorKind::of(&e, &self.now) == ErrorKind::TokenRejected => {
                error!("todoist rejected the token");
                self.not_loaded(LoadError::TokenRejected);
            }
            Err(e) if first => self.report(Failure::from_client("load your tasks", &e, &self.now)),
            Err(e) => self.log_failure(Failure::from_client("sync with Todoist", &e, &self.now)),
        }
    }

    pub(crate) fn apply_sync_delta(&mut self, mut resp: SyncResponse, effects: &mut Vec<Effect>) {
        let focused = self.selected_task().map(|task| task.id.clone());
        let full = resp.full_sync;
        let token = std::mem::take(&mut resp.sync_token);
        if let Some(user) = &resp.user {
            self.websocket_url = user.websocket_url.clone();
        }
        let ops = &self.temp_id_pending;
        let in_flight = |id: &str| !full && ops.values().any(|op| op.task() == Some(id));
        let merged = self.store.merge(resp, in_flight);
        if merged.projects {
            self.sort_projects();
        }
        if let Some(before) = &merged.replaced_tasks {
            self.keep_marks_of_unchanged_tasks(before);
            // A full sync carries every due date as Todoist parsed it.
            self.pending_due.clear();
        }
        for id in &merged.tasks {
            self.rolled_back.remove(id);
            self.pending_due.remove(id);
        }
        if let Some(task_id) = focused.as_ref()
            && merged.comments.contains(task_id)
            && let Some(updated) = self.store.comments_by_task.get(task_id)
        {
            self.comments = updated.clone();
        }
        self.report_cycles();

        self.take_sync_token(token);
        self.last_sync_at = Some(self.now.clone());
        self.keep_focus_on(focused, effects);
    }

    /// Logs each loop in the parents of the tasks or of the projects that
    /// was not there after the last merge. The tasks and projects on it show
    /// at top level.
    fn report_cycles(&mut self) {
        let tasks = &self.store.tasks;
        let projects = &self.store.projects;
        let mut found: Vec<(String, String)> = Vec::new();
        for cycle in cycles(&parents(tasks, |task| task.parent_id.as_deref())) {
            let members: Vec<&str> = cycle
                .iter()
                .filter_map(|&at| tasks.get(at))
                .map(|task| task.id.as_str())
                .collect();
            if let Some(first) = cycle.first().and_then(|&at| tasks.get(at)) {
                found.push((
                    members.join(","),
                    format!("show \"{}\" under its parent task", first.content),
                ));
            }
        }
        for cycle in cycles(&parents(projects, |project| project.parent_id.as_deref())) {
            let members: Vec<&str> = cycle
                .iter()
                .filter_map(|&at| projects.get(at))
                .map(|project| project.id.as_str())
                .collect();
            if let Some(first) = cycle.first().and_then(|&at| projects.get(at)) {
                found.push((
                    members.join(","),
                    format!("show project \"{}\" under its parent", first.name),
                ));
            }
        }
        let now: HashSet<String> = found.iter().map(|(members, _)| members.clone()).collect();
        for (members, action) in found {
            if !self.reported_cycles.contains(&members) {
                let message = format!("the parents of {members} loop back to each other");
                self.log_failure(Failure::new(
                    ErrorKind::ParentCycle,
                    action,
                    message,
                    &self.now,
                ));
            }
        }
        self.reported_cycles = now;
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
        match self.store.comments_by_task.get(&task_id) {
            Some(cached) => self.comments = cached.clone(),
            None => self.comments.clear(),
        }
        self.fetch_comments(task_id, effects);
    }

    fn take_sync_token(&mut self, token: String) {
        if !token.is_empty() {
            self.sync_token = token;
        }
        self.mark_cache_dirty();
    }

    fn mark_cache_dirty(&mut self) {
        self.cache_dirty_since
            .get_or_insert_with(|| self.now.timestamp());
    }

    /// Shows what the cache holds at once, before the first sync answers;
    /// that sync then asks only for what changed since `sync_token`, and
    /// is checked against `user_id`.
    pub fn paint_from_cache(&mut self, store: Store, sync_token: String, user_id: String) {
        self.store = store;
        self.sync_token = sync_token;
        self.cache_user_id = Some(user_id);
        self.loading = false;
    }

    /// Drops what the cache painted, which belongs to another user, and
    /// reads everything again behind the loading screen.
    fn discard_cache(&mut self, effects: &mut Vec<Effect>) {
        self.store = Store::default();
        self.comments.clear();
        self.completed_cache.clear();
        self.oldest_completed.clear();
        self.rolled_back.clear();
        self.pending_due.clear();
        self.loading = true;
        self.selected_task = 0;
        let request = full_sync_request();
        self.sync_token = request.sync_token.clone();
        self.mark_cache_dirty();
        effects.push(Effect::Sync(request));
    }

    /// The cache write due by `now`: once [`CACHE_DEBOUNCE`] has passed
    /// since a merge changed the store, and never while a command is
    /// unanswered, since the store then holds a write Todoist may still
    /// reject.
    pub(crate) fn cache_save_due(&mut self, now: jiff::Timestamp) -> Option<Effect> {
        let since = self.cache_dirty_since?;
        if now.duration_since(since) < CACHE_DEBOUNCE {
            return None;
        }
        let save = self.unsaved_cache();
        if save.is_some() {
            self.cache_dirty_since = None;
        }
        save
    }

    /// The cache write the store still needs, whatever the debounce says:
    /// the runtime asks when the app stops.
    pub fn unsaved_cache(&self) -> Option<Effect> {
        (self.cache_dirty_since.is_some() && self.temp_id_pending.is_empty())
            .then_some(Effect::SaveCache)
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
                    rejected.push((
                        uuid.clone(),
                        Failure::from_command(action, error, &self.now),
                    ));
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
        self.take_sync_token(resp.sync_token);
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
            let failure = Failure::from_client(action, error, &self.now);
            self.roll_back(uuid, &failure);
            failures.push(failure);
        }
        if failures.is_empty() {
            failures.push(Failure::from_client("send a change", error, &self.now));
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
                    .and_then(|id| self.store.projects.by_id(id))
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
                .and_then(|id| self.store.tasks.by_id(id))
                .map(|t| t.content.clone()),
        };
        match content {
            Some(content) => format!("{verb} {}", quoted(&content)),
            None => format!("{verb} a task"),
        }
    }

    fn apply_temp_id_mapping(&mut self, temp_id: &str, real_id: &str) {
        self.store
            .tasks
            .rekey(temp_id, real_id, |task, id| task.id = id.to_owned());
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
                self.store.tasks.retain(|t| t.id != temp_id);
                None
            }
            OptimisticOp::TaskUpdated { task_id, before } => {
                if let Some(t) = self.store.tasks.by_id_mut(&task_id) {
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
                if let Some(p) = self.store.projects.by_id_mut(&project_id) {
                    *p = *before;
                }
                self.sort_projects();
                None
            }
        }
    }

    /// Drops the failed mark of every task a full sync changed or removed,
    /// comparing each task with its copy in `before`.
    fn keep_marks_of_unchanged_tasks(&mut self, before: &Table<Task>) {
        let tasks = &self.store.tasks;
        self.rolled_back.retain(|id, _| {
            let old = before.by_id(id);
            let new = tasks.by_id(id);
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
        // Written with token `*`, the cache is deleted: what it holds may be
        // what `R` was pressed to get rid of.
        self.mark_cache_dirty();
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
                if let Some(t) = self.store.tasks.by_id_mut(&task_id) {
                    t.note_count = Some(count);
                }
                self.store
                    .comments_by_task
                    .insert(task_id.clone(), c.clone());
                let current_tid = self.selected_task().map(|t| t.id.clone());
                if current_tid.as_deref() == Some(&task_id) && fetch_seq == self.comments_fetch_seq
                {
                    self.comments = c;
                }
            }
            Err(e) => {
                let action = match self.store.tasks.by_id(&task_id) {
                    Some(task) => format!("load the comments on \"{}\"", task.content),
                    None => "load comments".to_owned(),
                };
                self.log_failure(Failure::from_client(action, &e, &self.now));
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
                self.log_failure(Failure::from_client("load completed tasks", &e, &self.now));
            }
        }
    }

    pub(crate) fn sort_projects(&mut self) {
        let selected_id = self
            .store
            .projects
            .get(self.selected_project)
            .map(|p| p.id.clone());
        let source = self.store.projects.to_vec();
        let mut ordered: Vec<Project> = Vec::with_capacity(source.len());

        let personal: Vec<Project> = source
            .iter()
            .filter(|p| p.workspace_id.is_none())
            .cloned()
            .collect();
        collect_project_subtree(None, &personal, &mut ordered);

        for ws in &self.store.workspaces {
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
                .store
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

        self.store.projects.replace_all(ordered);
        if let Some(id) = selected_id
            && let Some(pos) = self.store.projects.iter().position(|p| p.id == id)
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
    use super::{full_sync_request, incremental_sync_request, new_uuid};

    #[test]
    fn every_sync_reads_reminders() {
        let reads = |types: Vec<String>| types.iter().any(|t| t == "reminders");

        assert!(reads(full_sync_request().resource_types));
        assert!(reads(incremental_sync_request("token").resource_types));
    }

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

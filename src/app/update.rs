use std::collections::HashSet;

use crossterm::event::KeyEvent;
use tracing::info;

use super::modal::{InputTarget, Modal, TaskForm};
use super::state::{InputMode, Pane, TaskFilter, VimState};
use super::sync::{OptimisticOp, full_sync_request, incremental_sync_request, new_uuid};
use super::{Effect, Event, State};
use crate::keys::{self, KeyAction};
use crate::todoist::model::{Comment, Task};
use crate::todoist::sync::SyncCommand;
use crate::todoist::{CompletedQuery, LiveMessage};

/// What one event did: whether the screen changed, and what to ask the
/// outside world for.
#[derive(Debug, Default, PartialEq)]
pub struct Update {
    pub redraw: bool,
    pub effects: Vec<Effect>,
}

impl Update {
    fn quit() -> Self {
        Self {
            redraw: false,
            effects: vec![Effect::Quit],
        }
    }

    fn redraw(effects: Vec<Effect>) -> Self {
        Self {
            redraw: true,
            effects,
        }
    }
}

pub fn update(state: &mut State, event: Event) -> Update {
    let mut effects = Vec::new();
    match event {
        Event::Start => {
            info!("full sync starting");
            Update {
                redraw: false,
                effects: vec![Effect::Sync(full_sync_request())],
            }
        }
        Event::Key(key) => on_key(state, key),
        Event::Shutdown => Update::quit(),
        Event::Resize { .. } => Update::redraw(effects),
        Event::Tick { now } => {
            let before = (state.is_idle(), state.today);
            state.now = now;
            state.today = now.date_naive();
            Update {
                redraw: before != (state.is_idle(), state.today),
                effects,
            }
        }
        Event::Synced(result) => {
            state.synced(result, &mut effects);
            Update::redraw(effects)
        }
        Event::CommandsSent { uuids, result } => {
            state.commands_sent(&uuids, result, &mut effects);
            Update::redraw(effects)
        }
        Event::Comments {
            task_id,
            fetch_seq,
            result,
        } => {
            state.comments_loaded(task_id, fetch_seq, result);
            Update::redraw(effects)
        }
        Event::CompletedTasks { project_id, result } => {
            state.completed_loaded(project_id, result);
            Update::redraw(effects)
        }
        Event::Live(message) => {
            let was_connected = state.websocket_connected;
            match message {
                LiveMessage::Connected => state.websocket_connected = true,
                LiveMessage::Changed => {
                    state.websocket_connected = true;
                    if state.is_idle() {
                        state.pending_ws_sync = true;
                    } else {
                        effects.push(Effect::Sync(incremental_sync_request(&state.sync_token)));
                    }
                }
                LiveMessage::Disconnected => state.websocket_connected = false,
            }
            Update {
                redraw: was_connected != state.websocket_connected,
                effects,
            }
        }
    }
}

fn on_key(state: &mut State, key: KeyEvent) -> Update {
    if state.loading {
        return match keys::handle_key_while_loading(key) {
            KeyAction::Quit => Update::quit(),
            _ => Update::default(),
        };
    }

    let mut effects = Vec::new();
    let was_idle = state.is_idle();
    state.last_activity = state.now;
    // Leaving idle changes the status bar.
    let mut redraw = was_idle;
    if was_idle && state.pending_ws_sync {
        state.pending_ws_sync = false;
        effects.push(Effect::Sync(incremental_sync_request(&state.sync_token)));
    }

    if let Some(err) = state.error.take() {
        if !err.recoverable {
            info!("unrecoverable error dismissed, exiting");
            effects.push(Effect::Quit);
        }
        return Update::redraw(effects);
    }

    let prev_pane = state.active_pane;
    match keys::handle_key(state, key) {
        KeyAction::None => {}
        KeyAction::Quit => {
            info!("quit requested");
            effects.push(Effect::Quit);
        }
        action => {
            redraw = true;
            perform(state, action, &mut effects);
        }
    }
    if prev_pane == Pane::Tasks && state.active_pane != Pane::Tasks {
        state.dock_filter = None;
    }
    Update { redraw, effects }
}

fn perform(state: &mut State, action: KeyAction, effects: &mut Vec<Effect>) {
    match action {
        KeyAction::Quit | KeyAction::Consumed | KeyAction::None => {}
        KeyAction::ProjectChanged => state.switch_to_project_tasks(),
        KeyAction::TodayViewSelected => state.activate_today_view(),
        KeyAction::ToggleOverdueSection => state.toggle_overdue_section(),
        KeyAction::OpenDetail => state.open_detail(effects),
        KeyAction::CloseDetail => {
            state.active_pane = Pane::Tasks;
            state.detail_scroll = 0;
        }
        KeyAction::ToggleSettings => {
            state.active_pane = if state.settings_open() {
                Pane::Projects
            } else {
                Pane::Settings
            };
        }
        KeyAction::ToggleHelp => {
            if state.top_modal() == Some(&Modal::Help) {
                state.modals.pop();
            } else {
                state.modals.push(Modal::Help);
            }
        }
        KeyAction::ToggleMode => state.toggle_input_mode(),
        KeyAction::ToggleCollapse => state.toggle_collapse(),
        KeyAction::ToggleFolderCollapse => state.toggle_folder_collapse(),
        KeyAction::OpenAllFolds => state.collapsed.clear(),
        KeyAction::CloseAllFolds => state.close_all_folds(),
        KeyAction::CompleteTask => state.complete_selected_task(effects),
        KeyAction::SelectPriority => {
            let Some(Modal::PriorityPicker { selection }) = state.modals.pop() else {
                return;
            };
            if let Some(Modal::TaskForm(form)) = state.modals.last_mut() {
                form.priority = selection;
            } else {
                state.apply_priority(selection, effects);
            }
        }
        KeyAction::StarProject => state.star_selected_project(effects),
        KeyAction::ForceResync => state.force_full_resync(effects),
        KeyAction::CycleFilter => state.cycle_task_filter(effects),
        KeyAction::CycleSort => {
            state.sort_mode = state.sort_mode.next();
            info!(sort = state.sort_mode.label(), "sort mode changed");
        }
        KeyAction::StartInput => state.start_input(),
        KeyAction::StartCommentInput => state.start_input_for(InputTarget::Comment, String::new()),
        KeyAction::StartFieldEdit => state.start_field_edit(),
        KeyAction::SubmitInput | KeyAction::FormEscNormal => state.submit_input(effects),
        KeyAction::SubmitForm => state.submit_task_form(effects),
        KeyAction::FormFieldUp => state.form_field_up(),
        KeyAction::FormFieldDown => state.form_field_down(),
        KeyAction::FormEditField => state.form_edit_field(),
        KeyAction::CancelInput => state.cancel_input(),
        KeyAction::DetailFieldUp => state.move_detail_field(-1),
        KeyAction::DetailFieldDown => state.move_detail_field(1),
        KeyAction::OpenThemePicker => state.modals.push(Modal::ThemePicker {
            selection: state.theme_idx,
        }),
        KeyAction::SelectTheme => {
            if let Some(Modal::ThemePicker { selection }) = state.modals.pop() {
                state.theme_idx = selection;
                effects.push(state.save_ui_settings());
            }
        }
        KeyAction::CloseThemePicker => {
            state.modals.pop();
        }
        KeyAction::CycleIdleTimeout => {
            state.cycle_idle_timeout();
            effects.push(state.save_ui_settings());
        }
    }
}

impl State {
    fn save_ui_settings(&self) -> Effect {
        Effect::SaveUiSettings {
            theme: self.theme().name.clone(),
            idle_timeout_secs: self.idle_timeout_secs,
        }
    }

    fn cycle_idle_timeout(&mut self) {
        const OPTIONS: &[u64] = &[60, 120, 300, 600, 900, 1800];
        const DEBUG_OPTIONS: &[u64] = &[5, 60, 120, 300, 600, 900, 1800];
        let options = if self.idle_forcer {
            DEBUG_OPTIONS
        } else {
            OPTIONS
        };
        let pos = options
            .iter()
            .position(|&v| v == self.idle_timeout_secs)
            .unwrap_or(2);
        self.idle_timeout_secs = options[(pos + 1) % options.len()];
    }

    fn cycle_task_filter(&mut self, effects: &mut Vec<Effect>) {
        self.task_filter = self.task_filter.next();
        if matches!(self.task_filter, TaskFilter::Done | TaskFilter::Both)
            && let Some(pid) = self
                .projects
                .get(self.selected_project)
                .map(|p| p.id.clone())
            && !self.completed_cache.contains_key(&pid)
        {
            effects.push(Effect::FetchCompletedTasks(CompletedQuery {
                project_id: Some(pid),
                since: None,
            }));
        }
        self.clamp_selected_task();
    }

    fn open_detail(&mut self, effects: &mut Vec<Effect>) {
        let visible = self.visible_tasks();
        let Some(task) = visible.get(self.selected_task) else {
            return;
        };
        let task_id = task.id.clone();
        let task_project_id = task.project_id.clone();

        if self.dock_filter.is_some()
            && let Some(pos) = self.projects.iter().position(|p| p.id == task_project_id)
        {
            self.selected_project = pos;
        }

        self.active_pane = Pane::Detail;
        self.detail_scroll = 0;
        self.detail_field = 0;

        // Serve cached comments immediately, refresh in background.
        if let Some(cached) = self.comments_by_task.get(&task_id) {
            self.comments = cached.clone();
        } else {
            self.comments.clear();
        }
        self.fetch_comments(task_id, effects);
    }

    fn switch_to_project_tasks(&mut self) {
        self.today_view_active = false;
        self.selected_task = 0;
        self.detail_scroll = 0;
    }

    fn activate_today_view(&mut self) {
        tracing::debug!("today view activated");
        self.today_view_active = true;
        self.overdue_section_collapsed = false;
        self.selected_task = 0;
        self.detail_scroll = 0;
    }

    fn toggle_overdue_section(&mut self) {
        self.overdue_section_collapsed = !self.overdue_section_collapsed;
        if self.overdue_section_collapsed {
            self.selected_task = 0;
        }
    }

    fn complete_selected_task(&mut self, effects: &mut Vec<Effect>) {
        let (task_id, was_checked, is_recurring) = {
            let visible = self.visible_tasks();
            let Some(task) = visible.get(self.selected_task) else {
                return;
            };
            (
                task.id.clone(),
                task.checked,
                task.due.as_ref().map(|d| d.is_recurring).unwrap_or(false),
            )
        };

        let before = self.tasks.iter().find(|t| t.id == task_id).cloned();
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.checked = !was_checked;
        }

        let new_len = self.visible_tasks().len();
        if new_len > 0 && self.selected_task >= new_len {
            self.selected_task = new_len - 1;
        }

        let cmd_type = if was_checked {
            "item_reopen"
        } else if is_recurring {
            // item_complete advances the series; item_close would end it.
            "item_complete"
        } else {
            "item_close"
        };

        let command = SyncCommand {
            r#type: cmd_type.to_string(),
            temp_id: None,
            uuid: new_uuid(),
            args: serde_json::json!({ "id": task_id }),
        };
        let op = before.map(|snapshot| OptimisticOp::TaskUpdated {
            task_id,
            before: Box::new(snapshot),
        });
        self.send_command(command, op, effects);
    }

    fn enter_insert(&mut self) {
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Insert);
        }
    }

    fn leave_insert(&mut self) {
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Normal);
        }
    }

    fn start_input(&mut self) {
        let project_id = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.modals.push(Modal::TaskForm(TaskForm::new(project_id)));
        self.input_buffer.clear();
        self.enter_insert();
    }

    fn start_input_for(&mut self, target: InputTarget, prefill: String) {
        self.modals.push(Modal::Input(target));
        self.input_buffer = prefill;
        self.enter_insert();
    }

    fn submit_input(&mut self, effects: &mut Vec<Effect>) {
        let content = self.input_buffer.trim().to_string();

        if let Some(Modal::Input(target)) = self.top_modal() {
            if !content.is_empty() {
                match target {
                    InputTarget::Comment => self.submit_comment(content, effects),
                    InputTarget::Field => self.submit_field_edit(content, effects),
                }
            }
            self.cancel_input();
            return;
        }

        match self.modals.last_mut() {
            Some(Modal::TaskForm(form)) if form.editing => {
                match form.active_field {
                    // Content goes verbatim; the API parses any inline
                    // natural-language dates or priorities.
                    0 => form.content = content,
                    2 => form.due_string = content,
                    _ => {}
                }
                form.editing = false;
                self.input_buffer.clear();
                self.leave_insert();
            }
            _ => self.cancel_input(),
        }
    }

    fn submit_task_form(&mut self, effects: &mut Vec<Effect>) {
        let Some(Modal::TaskForm(form)) = self.modals.last().cloned() else {
            return;
        };

        if form.content.trim().is_empty() {
            self.cancel_input();
            return;
        }

        let project_id = form.project_id.clone();
        let temp_id = self.new_temp_id();

        let optimistic = Task {
            id: temp_id.clone(),
            content: form.content.clone(),
            project_id: project_id.clone(),
            priority: form.priority,
            ..Task::default()
        };
        self.tasks.push(optimistic);

        let mut args = serde_json::json!({
            "content": form.content,
            "project_id": project_id,
        });
        if !form.due_string.is_empty() {
            args["due_string"] = serde_json::Value::String(form.due_string);
        }
        if form.priority > 1 {
            args["priority"] = serde_json::Value::Number(serde_json::Number::from(form.priority));
        }

        let command = SyncCommand {
            r#type: "item_add".to_string(),
            temp_id: Some(temp_id.clone()),
            uuid: new_uuid(),
            args,
        };
        self.send_command(command, Some(OptimisticOp::TaskAdded { temp_id }), effects);
        self.cancel_input();
    }

    fn submit_comment(&mut self, content: String, effects: &mut Vec<Effect>) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id.clone();
        let temp_id = self.new_temp_id();

        let optimistic = Comment {
            id: temp_id.clone(),
            content: content.clone(),
            posted_at: Some(self.now.with_timezone(&chrono::Utc).to_rfc3339()),
            posted_by_uid: self.current_user_id.clone(),
            task_id: Some(task_id.clone()),
            item_id: Some(task_id.clone()),
            ..Comment::default()
        };
        self.comments.push(optimistic);
        self.comments_fetch_seq += 1;

        let command = SyncCommand {
            r#type: "note_add".to_string(),
            temp_id: Some(temp_id.clone()),
            uuid: new_uuid(),
            args: serde_json::json!({ "item_id": task_id, "content": content }),
        };
        let op = OptimisticOp::CommentAdded { temp_id, task_id };
        self.send_command(command, Some(op), effects);
    }

    fn submit_field_edit(&mut self, value: String, effects: &mut Vec<Effect>) {
        let (task_id, before) = {
            let Some(task) = self.selected_task() else {
                return;
            };
            (task.id.clone(), task.clone())
        };

        let args = match self.detail_field {
            0 => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.content = value.clone();
                }
                serde_json::json!({ "id": task_id, "content": value })
            }
            2 => {
                // Due string: the server parses it and returns the Due object,
                // so there is nothing to show optimistically.
                serde_json::json!({ "id": task_id, "due_string": value })
            }
            3 => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.description = value.clone();
                }
                serde_json::json!({ "id": task_id, "description": value })
            }
            _ => return,
        };

        let command = SyncCommand {
            r#type: "item_update".to_string(),
            temp_id: None,
            uuid: new_uuid(),
            args,
        };
        let op = OptimisticOp::TaskUpdated {
            task_id,
            before: Box::new(before),
        };
        self.send_command(command, Some(op), effects);
    }

    fn form_field_up(&mut self) {
        if let Some(Modal::TaskForm(form)) = self.modals.last_mut()
            && !form.editing
        {
            let count = TaskForm::field_count();
            form.active_field = if form.active_field == 0 {
                count - 1
            } else {
                form.active_field - 1
            };
        }
    }

    fn form_field_down(&mut self) {
        if let Some(Modal::TaskForm(form)) = self.modals.last_mut()
            && !form.editing
        {
            form.active_field = (form.active_field + 1) % TaskForm::field_count();
        }
    }

    fn form_edit_field(&mut self) {
        let project_ids: Vec<String> = self.projects.iter().map(|p| p.id.clone()).collect();
        let Some(Modal::TaskForm(form)) = self.modals.last_mut() else {
            return;
        };
        match form.active_field {
            0 | 2 => {
                let prefill = if form.active_field == 0 {
                    form.content.clone()
                } else {
                    form.due_string.clone()
                };
                form.editing = true;
                self.input_buffer = prefill;
                self.enter_insert();
            }
            1 => {
                let selection = form.priority;
                self.modals.push(Modal::PriorityPicker { selection });
            }
            3 => {
                let cur = project_ids
                    .iter()
                    .position(|id| *id == form.project_id)
                    .unwrap_or(0);
                let next = (cur + 1) % project_ids.len().max(1);
                if let Some(id) = project_ids.get(next) {
                    form.project_id = id.clone();
                }
            }
            _ => {}
        }
    }

    fn cancel_input(&mut self) {
        self.modals
            .retain(|m| !matches!(m, Modal::Input(_) | Modal::TaskForm(_)));
        self.input_buffer.clear();
        self.leave_insert();
    }

    fn star_selected_project(&mut self, effects: &mut Vec<Effect>) {
        let Some(project) = self.projects.get(self.selected_project) else {
            return;
        };
        let pid = project.id.clone();
        let before = project.clone();
        let new_fav = !project.is_favorite;

        if let Some(p) = self.projects.iter_mut().find(|p| p.id == pid) {
            p.is_favorite = new_fav;
        }
        self.sort_projects();

        let command = SyncCommand {
            r#type: "project_update".to_string(),
            temp_id: None,
            uuid: new_uuid(),
            args: serde_json::json!({ "id": pid, "is_favorite": new_fav }),
        };
        let op = OptimisticOp::ProjectUpdated {
            project_id: pid,
            before: Box::new(before),
        };
        self.send_command(command, Some(op), effects);
    }

    fn toggle_folder_collapse(&mut self) {
        let fid = if let Some(fi) = self.folder_cursor {
            self.folders.get(fi).map(|f| f.id.clone())
        } else {
            self.projects
                .get(self.selected_project)
                .and_then(|p| p.folder_id.clone())
        };
        let Some(fid) = fid else {
            return;
        };
        if self.collapsed_folders.contains(&fid) {
            self.collapsed_folders.remove(&fid);
        } else {
            self.collapsed_folders.insert(fid.clone());
        }
        if let Some(fi) = self.folders.iter().position(|f| f.id == fid) {
            self.folder_cursor = Some(fi);
        }
    }

    fn apply_priority(&mut self, new_priority: u8, effects: &mut Vec<Effect>) {
        let (task_id, before) = {
            let Some(task) = self.selected_task() else {
                return;
            };
            if task.priority == new_priority {
                return;
            }
            (task.id.clone(), task.clone())
        };

        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.priority = new_priority;
        }

        let command = SyncCommand {
            r#type: "item_update".to_string(),
            temp_id: None,
            uuid: new_uuid(),
            args: serde_json::json!({ "id": task_id, "priority": new_priority }),
        };
        let op = OptimisticOp::TaskUpdated {
            task_id,
            before: Box::new(before),
        };
        self.send_command(command, Some(op), effects);
    }

    fn start_field_edit(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };

        if self.detail_field == 1 {
            let selection = task.priority;
            self.modals.push(Modal::PriorityPicker { selection });
            return;
        }

        let prefill = match self.detail_field {
            0 => task.content.clone(),
            2 => task
                .due
                .as_ref()
                .and_then(|d| d.string.clone())
                .unwrap_or_default(),
            3 => task.description.clone(),
            _ => return,
        };
        self.start_input_for(InputTarget::Field, prefill);
    }

    fn move_detail_field(&mut self, delta: i32) {
        const FIELDS: usize = 4;
        self.detail_field = if delta < 0 {
            (self.detail_field + FIELDS - 1) % FIELDS
        } else {
            (self.detail_field + 1) % FIELDS
        };
    }

    fn toggle_collapse(&mut self) {
        let visible = self.visible_tasks();
        let Some(task) = visible.get(self.selected_task) else {
            return;
        };
        let task_id = task.id.clone();
        let parent_id = task.parent_id.clone();

        if self.has_children(&task_id) {
            if self.collapsed.contains(&task_id) {
                self.collapsed.remove(&task_id);
            } else {
                self.collapsed.insert(task_id);
            }
            return;
        }

        if let Some(pid) = parent_id {
            self.collapsed.insert(pid.clone());
            if let Some(pos) = self.visible_tasks().iter().position(|t| t.id == pid) {
                self.selected_task = pos;
            }
        }
    }

    fn close_all_folds(&mut self) {
        let parent_ids: HashSet<String> = self
            .tasks
            .iter()
            .filter_map(|t| t.parent_id.clone())
            .collect();
        for task in &self.tasks {
            if parent_ids.contains(&task.id) {
                self.collapsed.insert(task.id.clone());
            }
        }
    }

    fn toggle_input_mode(&mut self) {
        self.input_mode = match self.input_mode {
            InputMode::Vim(_) => InputMode::Standard,
            InputMode::Standard => InputMode::Vim(VimState::Normal),
        };
        info!(mode = self.input_mode.label(), "input mode toggled");
    }
}

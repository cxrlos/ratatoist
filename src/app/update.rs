use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Size;
use tracing::info;

use super::failure::{ErrorKind, Failure};
use super::modal::{InputKind, Modal, TaskForm};
use super::picker::{self, Choice, PickerKind, open_picker};
use super::state::{ListFilter, Pane, ProjectNavItem, Screen};
use super::sync::{OptimisticOp, full_sync_request, incremental_sync_request, new_uuid};
use super::text_field::TextField;
use super::{Action, Editable, Effect, Event, State};
use crate::keys::{self, Key, Resolution};
use crate::todoist::model::{Comment, Task};
use crate::todoist::sync::SyncCommand;
use crate::todoist::{CompletedQuery, LiveMessage};
use crate::ui;
use crate::ui::views::detail::{self, DetailField};

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
        Event::Start if state.load_error.is_some() => Update::default(),
        Event::Start => {
            info!("full sync starting");
            Update {
                redraw: false,
                effects: vec![Effect::Sync(full_sync_request())],
            }
        }
        Event::Key(key) => on_key(state, key),
        Event::Paste(text) => paste(state, &text),
        Event::Shutdown => Update::quit(),
        Event::Resize { width, height } => {
            state.viewport = Some(Size::new(width, height));
            state.detail_scroll = state.detail_scroll.min(state.max_detail_scroll());
            Update::redraw(effects)
        }
        Event::Tick { now } => {
            let before = (state.today, state.indicator());
            state.now = now;
            state.today = now.date_naive();
            Update {
                redraw: before != (state.today, state.indicator()),
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
            let was_dropped = state.live_dropped;
            match message {
                LiveMessage::Connected => state.live_dropped = false,
                LiveMessage::Changed => {
                    state.live_dropped = false;
                    if state.is_idle() {
                        state.pending_ws_sync = true;
                    } else {
                        effects.push(Effect::Sync(incremental_sync_request(&state.sync_token)));
                    }
                }
                LiveMessage::Disconnected => {
                    if !was_dropped {
                        state.log_failure(Failure::new(
                            ErrorKind::Offline,
                            "keep the live connection open",
                            "the live-updates connection dropped",
                            state.now,
                        ));
                    }
                    state.live_dropped = true;
                }
            }
            Update {
                redraw: was_dropped != state.live_dropped,
                effects,
            }
        }
    }
}

fn on_key(state: &mut State, key: KeyEvent) -> Update {
    if key.kind == KeyEventKind::Release {
        return Update::default();
    }
    let key = Key::from(key);
    let mut effects = Vec::new();
    let was_idle = state.is_idle();
    state.last_activity = state.now;
    if was_idle && state.pending_ws_sync {
        state.pending_ws_sync = false;
        effects.push(Effect::Sync(incremental_sync_request(&state.sync_token)));
    }

    let cleared_notice = state.notice.take().is_some() | std::mem::take(&mut state.show_failure);
    let mut update = type_text(state, key).unwrap_or_else(|| press(state, key));
    // Leaving idle changes the sync indicator.
    update.redraw |= was_idle || cleared_notice;
    effects.append(&mut update.effects);
    update.effects = effects;
    update
}

/// A field edit a key makes, before the keymap sees it.
enum TextEdit {
    Insert(char),
    Backspace,
    Delete,
    DeleteWord,
    Left,
    Right,
    Home,
    End,
}

impl TextEdit {
    fn of(key: Key) -> Option<Self> {
        if let Some(c) = key.typed() {
            return Some(TextEdit::Insert(c));
        }
        [
            (Key::named(KeyCode::Backspace), TextEdit::Backspace),
            (Key::named(KeyCode::Delete), TextEdit::Delete),
            (Key::ctrl('w'), TextEdit::DeleteWord),
            (Key::named(KeyCode::Left), TextEdit::Left),
            (Key::named(KeyCode::Right), TextEdit::Right),
            (Key::named(KeyCode::Home), TextEdit::Home),
            (Key::named(KeyCode::End), TextEdit::End),
        ]
        .into_iter()
        .find_map(|(bound, edit)| (bound == key).then_some(edit))
    }

    /// Applies the edit; whether the field changed.
    fn apply(self, field: &mut TextField) -> bool {
        match self {
            TextEdit::Insert(c) => {
                field.insert(c);
                true
            }
            TextEdit::Backspace => field.backspace(),
            TextEdit::Delete => field.delete(),
            TextEdit::DeleteWord => field.delete_word(),
            TextEdit::Left => field.left(),
            TextEdit::Right => field.right(),
            TextEdit::Home => field.home(),
            TextEdit::End => field.end(),
        }
    }
}

/// The text field that takes typed keys now: the open prompt's, or the
/// add-task form's active text field.
fn focused_field(state: &mut State) -> Option<&mut TextField> {
    match state.top_modal() {
        Some(Modal::Input(_)) => Some(&mut state.input),
        Some(Modal::Form(form)) if form.editing() => Some(&mut state.input),
        _ => None,
    }
}

/// Text entry: when the top overlay has a text field, a printable key
/// without Ctrl or Alt types into it, and Backspace, Delete, Ctrl-w and the
/// cursor keys edit it, before the keymap, so `j`, `k` and `q` type rather
/// than move or quit. The one exception is a key bound to a number pick
/// that can run now, so 1 to 9 pick in a picker; a digit with no entry to
/// pick types into its filter.
fn type_text(state: &mut State, key: Key) -> Option<Update> {
    let edit = TextEdit::of(key)?;
    if key.typed().is_some() && picks_by_number(state, key) {
        return None;
    }
    if let Some(Modal::Picker(picker)) = state.modals.last_mut() {
        return Some(Update {
            redraw: edit_query(picker.query_mut(), edit),
            effects: Vec::new(),
        });
    }
    let field = focused_field(state)?;
    Some(Update {
        redraw: edit.apply(field),
        effects: Vec::new(),
    })
}

/// Applies `edit` to a picker's filter, which has no cursor: typing adds at
/// the end and Backspace and Ctrl-w take from it. Whether it changed.
fn edit_query(query: &mut String, edit: TextEdit) -> bool {
    match edit {
        TextEdit::Insert(c) => {
            query.push(c);
            true
        }
        TextEdit::Backspace => query.pop().is_some(),
        TextEdit::DeleteWord => {
            let kept = query.trim_end().rfind(' ').map_or(0, |space| space + 1);
            let changed = kept < query.len();
            query.truncate(kept);
            changed
        }
        TextEdit::Delete | TextEdit::Left | TextEdit::Right | TextEdit::Home | TextEdit::End => {
            false
        }
    }
}

/// Pasted text goes into the focused text field, and nowhere else.
fn paste(state: &mut State, text: &str) -> Update {
    match focused_field(state) {
        Some(field) if !text.is_empty() => {
            field.paste(text);
            Update::redraw(Vec::new())
        }
        _ => Update::default(),
    }
}

/// Whether `key` runs a number pick that can run now.
fn picks_by_number(state: &State, key: Key) -> bool {
    let contexts = keys::active_contexts(state);
    match state.keymap.resolve(&contexts, &[key]) {
        Resolution::Action(action @ Action::PickNumber(_)) => {
            keys::spec(action).is_some_and(|spec| spec.availability(state).can_run())
        }
        _ => false,
    }
}

/// Runs the binding for `key` in the active contexts, or for the chord it
/// completes. A key that continues no chord just closes the which-key popup.
fn press(state: &mut State, key: Key) -> Update {
    let mut keys = Vec::new();
    let mut chord_ended = false;
    if let Some(Modal::WhichKey { prefix }) = state.top_modal() {
        keys.clone_from(prefix);
        state.modals.pop();
        chord_ended = true;
    }
    keys.push(key);

    let contexts = keys::active_contexts(state);
    let mut update = match state.keymap.resolve(&contexts, &keys) {
        Resolution::Action(action) => perform(state, action),
        Resolution::Prefix => {
            state.modals.push(Modal::WhichKey { prefix: keys });
            Update::redraw(Vec::new())
        }
        Resolution::Unbound => Update::default(),
    };
    update.redraw |= chord_ended;
    update
}

/// Carries out `action` against `state`. Actions whose feature does not
/// exist yet change nothing and ask for no redraw.
pub fn perform(state: &mut State, action: Action) -> Update {
    let mut effects = Vec::new();
    let redraw = state.perform(action, &mut effects);
    Update { redraw, effects }
}

impl State {
    /// Runs `action`; whether the screen changed.
    fn perform(&mut self, action: Action, effects: &mut Vec<Effect>) -> bool {
        match action {
            Action::Quit => {
                info!("quit requested");
                effects.push(Effect::Quit);
                false
            }
            Action::ShowScreen(screen) => self.show_screen(screen),
            // Logged out or with a rejected token there is nothing to sync,
            // and a sync answered by the stand-in client would save its token.
            Action::Resync if self.load_error.is_some() => false,
            Action::Resync => {
                self.force_full_resync(effects);
                true
            }
            Action::ShowCheatsheet => {
                self.modals.push(Modal::Cheatsheet { scroll: 0 });
                true
            }
            Action::ChooseTheme => {
                open_picker(self, PickerKind::Theme);
                true
            }
            Action::ShowMessages => {
                self.modals.push(Modal::Messages { scroll: 0 });
                true
            }
            Action::ShowPalette
            | Action::SkipRelease
            | Action::ShowReleaseNotes
            | Action::Confirm
            | Action::Decline => false,
            Action::Close => self.close(),
            Action::ScrollDown => self.scroll_overlay(1),
            Action::ScrollUp => self.scroll_overlay(-1),
            Action::Accept => self.accept(effects),
            Action::PickNumber(n) => self.pick_number(n, effects),
            Action::NextField => self.move_form_field(1),
            Action::PreviousField => self.move_form_field(-1),
            Action::MoveDown
            | Action::MoveUp
            | Action::MoveToTop
            | Action::MoveToBottom
            | Action::HalfPageDown
            | Action::HalfPageUp => self.move_cursor(action),
            Action::LineDown | Action::LineUp => {
                self.active_pane == Pane::Detail && self.scroll_detail(action)
            }
            Action::FocusLeft => self.focus_left(),
            Action::FocusRight => self.focus_right(),
            Action::Open => {
                if self.active_pane != Pane::Tasks || self.selected_task().is_none() {
                    return false;
                }
                self.open_detail(effects);
                true
            }
            Action::OpenList => self.open_list(),
            Action::ToggleComplete => {
                self.complete_selected_task(effects);
                true
            }
            Action::AddTask => {
                self.start_input();
                true
            }
            Action::Edit(field) => self.edit(field),
            Action::Comment => {
                let Some(task) = self.selected_task() else {
                    return false;
                };
                self.edit_task = Some(task.id.clone());
                self.start_input_for(InputKind::Comment, String::new());
                true
            }
            Action::ToggleFold => self.toggle_fold(),
            Action::OpenAllFolds => {
                self.collapsed.clear();
                true
            }
            Action::CloseAllFolds => {
                self.close_all_folds();
                true
            }
            Action::FilterList => {
                open_picker(self, PickerKind::Filter);
                true
            }
            Action::SortList => {
                open_picker(self, PickerKind::Sort);
                true
            }
            Action::ToggleFavourite => {
                self.star_selected_project(effects);
                true
            }
        }
    }

    /// Switches to `screen`, on its list, or on the tree for Projects.
    fn show_screen(&mut self, screen: Screen) -> bool {
        if self.screen == screen {
            return false;
        }
        self.screen = screen;
        self.active_pane = match screen {
            Screen::Projects | Screen::Labels => Pane::Projects,
            Screen::Inbox | Screen::Today | Screen::Upcoming => Pane::Tasks,
        };
        self.list_filter = ListFilter::default();
        self.selected_task = 0;
        self.detail_scroll = 0;
        true
    }

    /// Closes the top overlay, or steps back: from the detail to its list,
    /// from a list filter other than open tasks to open tasks, from a
    /// project's list to the Projects tree.
    fn close(&mut self) -> bool {
        match self.top_modal() {
            Some(Modal::Input(_) | Modal::Form(_)) => {
                self.cancel_input();
                return true;
            }
            Some(_) => {
                self.modals.pop();
                return true;
            }
            None => {}
        }
        match self.active_pane {
            Pane::Detail => {
                self.active_pane = Pane::Tasks;
                self.detail_scroll = 0;
                true
            }
            Pane::Tasks | Pane::Projects if self.list_filter != ListFilter::default() => {
                self.keep_focus(|state| state.list_filter = ListFilter::default());
                true
            }
            Pane::Tasks if self.screen == Screen::Projects => {
                self.active_pane = Pane::Projects;
                true
            }
            Pane::Tasks | Pane::Projects => false,
        }
    }

    /// Moves the highlight of the overlay on top by `step`.
    fn scroll_overlay(&mut self, step: isize) -> bool {
        if let Some(&Modal::Messages { scroll: before }) = self.top_modal() {
            let max = ui::messages_max_scroll(self, self.screen_size());
            let after = if step > 0 {
                before.saturating_add(1).min(max)
            } else {
                before.saturating_sub(1)
            };
            if let Some(Modal::Messages { scroll }) = self.modals.last_mut() {
                *scroll = after;
            }
            return after != before;
        }
        match self.modals.last_mut() {
            Some(Modal::Picker(picker)) => picker.select_by(step),
            Some(Modal::Form(_)) => self.move_form_field(if step > 0 { 1 } else { -1 }),
            _ => false,
        }
    }

    /// Chooses what the overlay on top has highlighted, or submits it.
    fn accept(&mut self, effects: &mut Vec<Effect>) -> bool {
        match self.top_modal() {
            Some(Modal::Picker(picker)) => match picker.selected().cloned() {
                Some(choice) => self.choose(choice, effects),
                None => false,
            },
            Some(Modal::Input(_)) => {
                self.submit_input(effects);
                true
            }
            Some(Modal::Form(_)) => self.accept_form(effects),
            _ => false,
        }
    }

    /// Chooses entry `n` of the picker on top, counting from 1 among the
    /// entries its filter leaves.
    fn pick_number(&mut self, n: u8, effects: &mut Vec<Effect>) -> bool {
        match self.top_modal() {
            Some(Modal::Picker(picker)) => match picker.nth(usize::from(n)).cloned() {
                Some(choice) => self.choose(choice, effects),
                None => false,
            },
            _ => false,
        }
    }

    /// Closes the picker on top and applies `choice`.
    fn choose(&mut self, choice: Choice, effects: &mut Vec<Effect>) -> bool {
        self.modals.pop();
        match choice {
            Choice::Priority(priority) => {
                if let Some(Modal::Form(form)) = self.modals.last_mut() {
                    form.priority = priority.api();
                } else {
                    self.apply_priority(priority.api(), effects);
                }
            }
            Choice::Sort(sort) => {
                info!(sort = sort.label(), "sort changed");
                self.keep_focus(|state| state.sort = sort);
            }
            Choice::Filter(filter) => {
                info!(filter = filter.label(), "list filter changed");
                self.keep_focus(|state| state.list_filter = filter);
                self.fetch_completed_if_shown(effects);
            }
            Choice::Project(project_id) => {
                if let Some(Modal::Form(form)) = self.modals.last_mut() {
                    form.project_id = project_id;
                }
            }
            Choice::Theme(index) => {
                if let Some(theme) = self.themes.get(index) {
                    self.notice = Some(picker::theme_notice(&theme.name));
                    self.theme_idx = index;
                }
            }
        }
        true
    }

    /// Runs `change` on the task list, then focuses the task focused before
    /// when it is still visible, or the nearest row when it is not.
    fn keep_focus(&mut self, change: impl FnOnce(&mut Self)) {
        let focused = self.selected_task().map(|task| task.id.clone());
        change(self);
        let position =
            focused.and_then(|id| self.visible_tasks().iter().position(|task| task.id == id));
        match position {
            Some(position) => self.selected_task = position,
            None => self.clamp_selected_task(),
        }
    }

    /// Moves the cursor of the focused pane, or the detail's field cursor
    /// and scroll.
    fn move_cursor(&mut self, action: Action) -> bool {
        match (self.active_pane, action) {
            (Pane::Detail, Action::MoveDown) => self.move_detail_field(1),
            (Pane::Detail, Action::MoveUp) => self.move_detail_field(-1),
            (Pane::Detail, Action::MoveToTop) => self.detail_field = DetailField::Content,
            (Pane::Detail, Action::MoveToBottom) => self.detail_field = DetailField::Description,
            (Pane::Detail, _) => return self.scroll_detail(action),
            (Pane::Tasks, _) => self.move_in_tasks(action),
            (Pane::Projects, _) if self.screen == Screen::Projects => {
                self.move_in_projects(action);
            }
            (Pane::Projects, _) => return false,
        }
        true
    }

    /// Half the rows of the list on screen, the step of a half-page move.
    fn half_page(&self) -> usize {
        (crate::ui::list_rows(self, self.screen_size()) / 2).max(1)
    }

    /// One row down or up wraps around the task list; the other moves stop
    /// at its ends. Context rows (the dimmed open parents of completed
    /// subtasks) are skipped.
    fn move_in_tasks(&mut self, action: Action) {
        let visible = self.visible_tasks();
        let len = visible.len();
        if len == 0 {
            return;
        }
        let last = len - 1;
        let current = self.selected_task.min(last);
        let (mut next, delta) = match action {
            Action::MoveDown => ((current + 1) % len, 1),
            Action::MoveUp => (current.checked_sub(1).unwrap_or(last), -1),
            Action::MoveToTop => (0, 1),
            Action::MoveToBottom => (last, -1),
            Action::HalfPageDown => (current.saturating_add(self.half_page()).min(last), 1),
            _ => (current.saturating_sub(self.half_page()), -1),
        };
        let wraps = matches!(action, Action::MoveDown | Action::MoveUp);
        for _ in 0..len {
            if !self.is_context_task(visible[next]) {
                break;
            }
            next = if wraps {
                (next as isize + delta).rem_euclid(len as isize) as usize
            } else {
                next.saturating_add_signed(delta).min(last)
            };
        }
        self.selected_task = next;
    }

    /// Moves through the Projects tree: folders and projects.
    fn move_in_projects(&mut self, action: Action) {
        let nav = self.visible_nav_items();
        if nav.is_empty() {
            return;
        }
        let last = nav.len() - 1;
        let current = nav
            .iter()
            .position(|item| match item {
                ProjectNavItem::Project(i) => {
                    self.folder_cursor.is_none() && *i == self.selected_project
                }
                ProjectNavItem::Folder(fi) => self.folder_cursor == Some(*fi),
            })
            .unwrap_or(0);
        let next = match action {
            Action::MoveDown => (current + 1).min(last),
            Action::MoveUp => current.saturating_sub(1),
            Action::MoveToTop => 0,
            Action::MoveToBottom => last,
            Action::HalfPageDown => current.saturating_add(self.half_page()).min(last),
            _ => current.saturating_sub(self.half_page()),
        };
        match nav[next] {
            ProjectNavItem::Project(i) => {
                let changed = self.folder_cursor.is_some() || self.selected_project != i;
                self.folder_cursor = None;
                self.selected_project = i;
                if changed {
                    self.switch_to_project_tasks();
                }
            }
            ProjectNavItem::Folder(fi) => self.folder_cursor = Some(fi),
        }
    }

    /// From the detail back to its list. The list and the detail show one
    /// at a time, so there is no pane to its left otherwise.
    fn focus_left(&mut self) -> bool {
        if self.active_pane != Pane::Detail {
            return false;
        }
        self.active_pane = Pane::Tasks;
        self.detail_scroll = 0;
        true
    }

    /// The list and the detail show one at a time, so there is no pane to
    /// the right of either.
    fn focus_right(&mut self) -> bool {
        false
    }

    /// Opens the focused project's tasks from the Projects tree.
    fn open_list(&mut self) -> bool {
        if self.screen != Screen::Projects
            || self.active_pane != Pane::Projects
            || self.folder_cursor.is_some()
            || self.projects.get(self.selected_project).is_none()
        {
            return false;
        }
        self.active_pane = Pane::Tasks;
        self.selected_task = 0;
        true
    }

    /// Opens the edit of `field` of the focused task: the priority picker,
    /// or the prompt for the matching detail field.
    fn edit(&mut self, field: Editable) -> bool {
        let Some(task) = self.selected_task() else {
            return false;
        };
        self.edit_task = Some(task.id.clone());
        self.detail_field = match field {
            Editable::Priority => {
                open_picker(self, PickerKind::Priority);
                return true;
            }
            Editable::Content => DetailField::Content,
            Editable::Description => DetailField::Description,
            Editable::DueDate => DetailField::Due,
        };
        self.start_field_edit();
        true
    }

    /// Folds the focused row: the Overdue group in the Today view, a task's
    /// subtasks, or the focused folder.
    fn toggle_fold(&mut self) -> bool {
        match self.active_pane {
            Pane::Tasks if self.screen == Screen::Today => self.toggle_overdue_section(),
            Pane::Tasks => self.toggle_collapse(),
            Pane::Projects if self.screen == Screen::Projects => self.toggle_folder_collapse(),
            _ => return false,
        }
        true
    }

    /// Asks for the listed project's completed tasks when the list filter
    /// shows them and they are not loaded yet.
    fn fetch_completed_if_shown(&mut self, effects: &mut Vec<Effect>) {
        if matches!(self.list_filter, ListFilter::Completed | ListFilter::All)
            && let Some(pid) = self.list_project().map(|p| p.id.clone())
            && !self.completed_cache.contains_key(&pid)
        {
            effects.push(Effect::FetchCompletedTasks(CompletedQuery {
                project_id: Some(pid),
                since: None,
            }));
        }
    }

    fn open_detail(&mut self, effects: &mut Vec<Effect>) {
        let visible = self.visible_tasks();
        let Some(task) = visible.get(self.selected_task) else {
            return;
        };
        let task_id = task.id.clone();

        self.active_pane = Pane::Detail;
        self.detail_scroll = 0;
        self.detail_field = DetailField::Content;

        // Serve cached comments immediately, refresh in background.
        if let Some(cached) = self.comments_by_task.get(&task_id) {
            self.comments = cached.clone();
        } else {
            self.comments.clear();
        }
        self.fetch_comments(task_id, effects);
    }

    fn switch_to_project_tasks(&mut self) {
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

    fn start_input(&mut self) {
        let project_id = self
            .list_project()
            .or_else(|| self.projects.iter().find(|p| p.is_inbox()))
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.modals.push(Modal::Form(TaskForm::new(project_id)));
        self.input = TextField::default();
    }

    fn start_input_for(&mut self, target: InputKind, prefill: String) {
        self.modals.push(Modal::Input(target));
        self.input = TextField::new(prefill);
    }

    fn submit_input(&mut self, effects: &mut Vec<Effect>) {
        let content = self.input.text().trim().to_string();

        if let Some(Modal::Input(target)) = self.top_modal() {
            if !content.is_empty() {
                match target {
                    InputKind::Comment => self.submit_comment(content, effects),
                    InputKind::Field => self.submit_field_edit(content, effects),
                    // The edit keys open the Field prompt; nothing opens these.
                    InputKind::Content | InputKind::Description | InputKind::DueDate => {}
                }
            }
            self.cancel_input();
        }
    }

    /// Enter in the add-task form: on a text field it adds the task; on the
    /// priority or the project it opens that picker.
    fn accept_form(&mut self, effects: &mut Vec<Effect>) -> bool {
        let Some(Modal::Form(form)) = self.top_modal() else {
            return false;
        };
        match form.active_field {
            1 => open_picker(self, PickerKind::Priority),
            3 => open_picker(self, PickerKind::Project),
            _ => {
                self.commit_form_field();
                self.submit_task_form(effects);
            }
        }
        true
    }

    /// Keeps the typed text in the form's active text field.
    fn commit_form_field(&mut self) {
        let text = self.input.text().to_owned();
        if let Some(Modal::Form(form)) = self.modals.last_mut() {
            match form.active_field {
                0 => form.content = text,
                2 => form.due_string = text,
                _ => {}
            }
        }
    }

    /// Moves the add-task form to the field `step` away, wrapping, keeping
    /// what was typed and loading the next text field's text.
    fn move_form_field(&mut self, step: i32) -> bool {
        self.commit_form_field();
        let Some(Modal::Form(form)) = self.modals.last_mut() else {
            return false;
        };
        let count = TaskForm::field_count();
        form.active_field = if step > 0 {
            (form.active_field + 1) % count
        } else {
            form.active_field.checked_sub(1).unwrap_or(count - 1)
        };
        self.input = match form.active_field {
            0 => TextField::new(form.content.clone()),
            2 => TextField::new(form.due_string.clone()),
            _ => TextField::default(),
        };
        true
    }

    fn submit_task_form(&mut self, effects: &mut Vec<Effect>) {
        let Some(Modal::Form(form)) = self.modals.last().cloned() else {
            return;
        };
        let content = form.content.trim().to_owned();
        let due_string = form.due_string.trim().to_owned();

        if content.is_empty() {
            self.cancel_input();
            return;
        }

        let project_id = form.project_id.clone();
        let temp_id = self.new_temp_id();

        let optimistic = Task {
            id: temp_id.clone(),
            content: content.clone(),
            project_id: project_id.clone(),
            priority: form.priority,
            ..Task::default()
        };
        self.tasks.push(optimistic);

        // Content goes verbatim; the API parses any inline natural-language
        // dates or priorities.
        let mut args = serde_json::json!({
            "content": content,
            "project_id": project_id,
        });
        if !due_string.is_empty() {
            args["due_string"] = serde_json::Value::String(due_string);
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
        let Some(task_id) = self.edit_task.take() else {
            return;
        };
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
        // The comment list is the open detail's; a comment from the list
        // shows there only when that detail is on the same task.
        if self.active_pane == Pane::Detail
            && self.selected_task().map(|t| t.id.as_str()) == Some(task_id.as_str())
        {
            self.comments.push(optimistic);
            self.comments_fetch_seq += 1;
        }

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
        let Some(before) = self.edit_target() else {
            return;
        };
        let task_id = before.id.clone();

        let args = match self.detail_field {
            DetailField::Content => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.content = value.clone();
                }
                serde_json::json!({ "id": task_id, "content": value })
            }
            DetailField::Due => {
                // Due string: the server parses it and returns the Due object,
                // so there is nothing to show optimistically.
                serde_json::json!({ "id": task_id, "due_string": value })
            }
            DetailField::Description => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.description = value.clone();
                }
                serde_json::json!({ "id": task_id, "description": value })
            }
            DetailField::Priority => return,
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

    fn cancel_input(&mut self) {
        self.modals
            .retain(|m| !matches!(m, Modal::Input(_) | Modal::Form(_)));
        self.input = TextField::default();
        self.edit_task = None;
    }

    /// The task the closing prompt or priority picker opened on, as it is
    /// now; `None` once it is gone.
    fn edit_target(&mut self) -> Option<Task> {
        let id = self.edit_task.take()?;
        self.tasks.iter().find(|t| t.id == id).cloned()
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
        let Some(before) = self.edit_target() else {
            return;
        };
        if before.priority == new_priority {
            return;
        }
        let task_id = before.id.clone();

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
        let id = task.id.clone();

        let prefill = match self.detail_field {
            DetailField::Content => task.content.clone(),
            DetailField::Priority => {
                self.edit_task = Some(id);
                open_picker(self, PickerKind::Priority);
                return;
            }
            DetailField::Due => task
                .due
                .as_ref()
                .and_then(|d| d.string.clone())
                .unwrap_or_default(),
            DetailField::Description => task.description.clone(),
        };
        self.edit_task = Some(id);
        self.start_input_for(InputKind::Field, prefill);
    }

    fn move_detail_field(&mut self, delta: i32) {
        self.detail_field = if delta < 0 {
            self.detail_field.prev()
        } else {
            self.detail_field.next()
        };
    }

    fn max_detail_scroll(&self) -> u16 {
        detail::max_scroll(self, self.screen_size())
    }

    /// Scrolls the task detail, never past its last row; whether it moved.
    /// Starting from the clamped offset makes the first key up move at once
    /// even when the content shrank since the last scroll.
    fn scroll_detail(&mut self, by: Action) -> bool {
        let max = self.max_detail_scroll();
        let half_page = (detail::page_height(self, self.screen_size()) / 2).max(1);
        let current = self.detail_scroll.min(max);
        let before = self.detail_scroll;
        self.detail_scroll = match by {
            Action::HalfPageDown => current.saturating_add(half_page),
            Action::HalfPageUp => current.saturating_sub(half_page),
            Action::LineDown => current.saturating_add(1),
            Action::LineUp => current.saturating_sub(1),
            _ => current,
        }
        .min(max);
        self.detail_scroll != before
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
}

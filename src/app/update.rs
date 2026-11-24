use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Size;
use tracing::info;

use super::confirm;
use super::failure::{ErrorKind, Failure};
use super::modal::Modal;
use super::palette::{self, Palette};
use super::picker::{self, Choice, PickerKind, open_picker};
use super::prompt::{PromptField, open_prompt};
use super::release;
use super::state::{ListFilter, Pane, Screen};
use super::sync::{OptimisticOp, incremental_sync_request, new_uuid, startup_sync_request};
use super::task_form::{FormField, TODAY, TaskForm};
use super::text_field::TextField;
use super::{Action, Editable, Effect, Event, State};
use crate::keys::{self, Key, Resolution};
use crate::todoist::model::{Comment, Due, Task};
use crate::todoist::sync::SyncCommand;
use crate::todoist::{CompletedQuery, CompletedWindow, LiveMessage};
use crate::ui;
use crate::ui::detail;

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
            info!(
                from_cache = state.cache_user_id.is_some(),
                "first sync starting"
            );
            Update {
                redraw: false,
                effects: vec![Effect::Sync(startup_sync_request(&state.sync_token))],
            }
        }
        Event::Key(key) => on_key(state, key),
        Event::Paste(text) => paste(state, &text),
        Event::Shutdown => Update::quit(),
        Event::ReleaseChecked(latest) => {
            let redraw = release::checked(state, latest, &mut effects);
            Update { redraw, effects }
        }
        Event::Resize { width, height } => {
            state.viewport = Some(Size::new(width, height));
            state.detail_scroll = state.detail_scroll.min(state.max_detail_scroll());
            Update::redraw(effects)
        }
        Event::Tick { now } => {
            effects.extend(release::tick(state, now.timestamp()));
            effects.extend(state.cache_save_due(now.timestamp()));
            let before = (state.today, state.indicator());
            state.today = now.date();
            state.now = now;
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
        Event::CompletedTasks { query, result } => {
            state.completed_loaded(query, result);
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
                            &state.now,
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
    state.last_activity = state.now.clone();
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
    Clear,
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
            (Key::ctrl('u'), TextEdit::Clear),
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
            TextEdit::Clear => {
                let changed = !field.is_empty();
                *field = TextField::default();
                changed
            }
            TextEdit::Left => field.left(),
            TextEdit::Right => field.right(),
            TextEdit::Home => field.home(),
            TextEdit::End => field.end(),
        }
    }

    /// Applies the edit to the palette's query, which is typed at its end
    /// and has no cursor to move; whether the query changed. A changed
    /// query highlights the best match again.
    fn apply_to_query(self, palette: &mut Palette) -> bool {
        let query = &mut palette.query;
        let before = query.len();
        match self {
            TextEdit::Insert(c) => query.push(c),
            TextEdit::Backspace => {
                query.pop();
            }
            TextEdit::Clear => query.clear(),
            TextEdit::DeleteWord => {
                let kept = query.trim_end().rfind(' ').map_or(0, |space| space + 1);
                query.truncate(kept);
            }
            TextEdit::Delete
            | TextEdit::Left
            | TextEdit::Right
            | TextEdit::Home
            | TextEdit::End => {}
        }
        let changed = query.len() != before;
        if changed {
            palette.selected = 0;
        }
        changed
    }
}

/// The text field that takes typed keys now: the open prompt's, or the
/// add-task form's active text field.
fn focused_field(state: &mut State) -> Option<&mut TextField> {
    match state.top_modal() {
        Some(Modal::Prompt(_)) => Some(&mut state.input),
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
    if let Some(Modal::Palette(palette)) = state.top_modal_mut() {
        return Some(Update {
            redraw: edit.apply_to_query(palette),
            effects: Vec::new(),
        });
    }
    let field = focused_field(state)?;
    let redraw = edit.apply(field);
    if redraw {
        forget_refusal(state);
    }
    Some(Update {
        redraw,
        effects: Vec::new(),
    })
}

/// Drops the reason the prompt or the add-task form last refused Enter,
/// once the user edits or moves on, so it never shows under text it no
/// longer applies to.
fn forget_refusal(state: &mut State) {
    match state.modals.last_mut() {
        Some(Modal::Form(form)) => form.error = None,
        Some(Modal::Prompt(prompt)) => prompt.error = None,
        _ => {}
    }
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
        TextEdit::Clear => {
            let changed = !query.is_empty();
            query.clear();
            changed
        }
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
    if let Some(Modal::Palette(palette)) = state.top_modal_mut() {
        let line = text.replace(['\n', '\r'], " ");
        if line.is_empty() {
            return Update::default();
        }
        palette.query.push_str(&line);
        palette.selected = 0;
        return Update::redraw(Vec::new());
    }
    match focused_field(state) {
        Some(field) if !text.is_empty() => {
            field.paste(text);
            forget_refusal(state);
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
                confirm::quit(self, effects);
                matches!(self.top_modal(), Some(Modal::Confirm(_)))
            }
            Action::ShowScreen(screen) => self.show_screen(screen),
            // Logged out or with a rejected token there is nothing to sync,
            // and a sync answered by the stand-in client would save its token.
            Action::Resync if self.load_error.is_some() => false,
            Action::Resync => {
                confirm::resync(self, effects);
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
            Action::ShowPalette => {
                self.modals.push(Modal::Palette(Palette::default()));
                true
            }
            Action::SkipRelease => release::skip(self, effects),
            Action::ShowReleaseNotes => release::show_notes(self, effects),
            Action::Confirm => confirm::confirm(self, effects),
            Action::Decline => confirm::decline(self),
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
            | Action::HalfPageUp => self.move_cursor(action, effects),
            Action::LineDown | Action::LineUp => {
                self.active_pane == Pane::Detail && self.scroll_detail(action)
            }
            Action::FocusLeft => self.focus_left(),
            Action::FocusRight => self.focus_right(effects),
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
                self.open_form();
                true
            }
            Action::Edit(field) => self.edit(field),
            Action::Comment => open_prompt(self, PromptField::Comment),
            Action::ToggleFold => self.toggle_fold(),
            Action::OpenAllFolds => {
                self.keep_focus(|state| {
                    state.collapsed.clear();
                    state.overdue_section_collapsed = false;
                });
                true
            }
            Action::CloseAllFolds => {
                self.keep_focus(Self::close_all_folds);
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
            Action::ToggleFavourite => self.toggle_favourite(effects),
            Action::LoadOlderCompleted => self.load_older_completed(effects),
        }
    }

    /// Switches to `screen`, on its list, or on the tree for Projects.
    fn show_screen(&mut self, screen: Screen) -> bool {
        if self.screen == screen {
            return false;
        }
        self.screen = screen;
        self.active_pane = match screen {
            Screen::Projects | Screen::Labels => Pane::Browse,
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
            Some(Modal::Prompt(_) | Modal::Form(_)) => {
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
                if !self.is_wide() {
                    self.detail_scroll = 0;
                }
                true
            }
            Pane::Tasks if self.list_filter != ListFilter::default() => {
                self.keep_focus(|state| state.list_filter = ListFilter::default());
                true
            }
            Pane::Tasks if matches!(self.screen, Screen::Projects | Screen::Labels) => {
                self.back_to_browse();
                true
            }
            Pane::Tasks | Pane::Browse => false,
        }
    }

    /// Moves the highlight of the overlay on top by `step`.
    fn scroll_overlay(&mut self, step: isize) -> bool {
        match self.top_modal() {
            Some(Modal::Cheatsheet { .. }) => return self.scroll_cheatsheet(step),
            Some(Modal::Palette(_)) => {
                let action = if step > 0 {
                    Action::ScrollDown
                } else {
                    Action::ScrollUp
                };
                return palette::step(self, action);
            }
            _ => {}
        }
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
            Some(Modal::Form(_)) => self.move_form_field(step.signum()),
            _ => false,
        }
    }

    /// Scrolls the cheatsheet on top one row, never past its last row;
    /// whether it moved. Starting from the clamped offset makes the first
    /// key up move at once after the terminal grew.
    fn scroll_cheatsheet(&mut self, step: isize) -> bool {
        let max = ui::cheatsheet_max_scroll(self, self.screen_size());
        let Some(Modal::Cheatsheet { scroll }) = self.modals.last_mut() else {
            return false;
        };
        let before = *scroll;
        let current = before.min(max);
        *scroll = if step > 0 {
            current.saturating_add(1).min(max)
        } else {
            current.saturating_sub(1)
        };
        *scroll != before
    }

    /// Chooses what the overlay on top has highlighted, or submits it.
    fn accept(&mut self, effects: &mut Vec<Effect>) -> bool {
        match self.top_modal() {
            Some(Modal::Palette(_)) => {
                let mut update = palette::accept(self);
                effects.append(&mut update.effects);
                update.redraw
            }
            Some(Modal::Picker(picker)) => match picker.selected().cloned() {
                Some(choice) => self.choose(choice, effects),
                None => false,
            },
            Some(Modal::Prompt(_)) => {
                self.submit_prompt(effects);
                true
            }
            Some(Modal::Form(_)) => self.accept_form(effects),
            Some(Modal::Confirm(_)) => confirm::confirm(self, effects),
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

    /// Moves the focus of the list, or scrolls the detail by rows. Beside
    /// the list, the detail follows the focused task.
    fn move_cursor(&mut self, action: Action, effects: &mut Vec<Effect>) -> bool {
        match self.active_pane {
            Pane::Detail => self.scroll_detail(action),
            Pane::Tasks => {
                let before = self.selected_task().map(|task| task.id.clone());
                self.move_in_tasks(action);
                let after = self.selected_task().map(|task| task.id.clone());
                if before != after {
                    self.follow_focus(effects);
                }
                true
            }
            Pane::Browse => self.move_in_browse(action),
        }
    }

    /// Whether the list and the detail show side by side now: on a wide
    /// terminal, on a task list or its detail.
    pub(crate) fn shows_panes(&self) -> bool {
        self.is_wide() && self.active_pane != Pane::Browse && !self.loading
    }

    /// Beside the list, shows the detail of the task the list focuses now:
    /// from its top, with the comments loaded so far, asking for them when
    /// none are.
    fn follow_focus(&mut self, effects: &mut Vec<Effect>) {
        if !self.shows_panes() {
            return;
        }
        self.detail_scroll = 0;
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id.clone();
        let wants_comments = task.note_count.unwrap_or(0) > 0;
        match self.store.comments_by_task.get(&task_id) {
            Some(cached) => self.comments = cached.clone(),
            None => {
                self.comments.clear();
                if wants_comments {
                    self.fetch_comments(task_id, effects);
                }
            }
        }
    }

    /// Half the rows of the list on screen, the step of a half-page move.
    pub(crate) fn half_page(&self) -> usize {
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

    /// From the detail to the list beside it. Below the wide layout only
    /// one of them shows, so neither has a pane beside it.
    fn focus_left(&mut self) -> bool {
        if !self.shows_panes() || self.active_pane != Pane::Detail {
            return false;
        }
        self.active_pane = Pane::Tasks;
        true
    }

    /// From the list to the detail beside it, when a task is focused.
    fn focus_right(&mut self, effects: &mut Vec<Effect>) -> bool {
        if !self.shows_panes() || self.active_pane != Pane::Tasks || self.selected_task().is_none()
        {
            return false;
        }
        self.open_detail(effects);
        true
    }

    /// Opens the edit of `field` of the focused task: the priority picker,
    /// or the prompt for the matching detail field.
    fn edit(&mut self, field: Editable) -> bool {
        let field = match field {
            Editable::Priority => {
                let Some(task) = self.selected_task() else {
                    return false;
                };
                self.edit_task = Some(task.id.clone());
                open_picker(self, PickerKind::Priority);
                return true;
            }
            Editable::Content => PromptField::Content,
            Editable::Description => PromptField::Description,
            Editable::DueDate => PromptField::DueDate,
        };
        open_prompt(self, field)
    }

    /// Folds the focused row: the Overdue group on Today, which lists no
    /// subtasks, a task's subtasks in a project, or the focused folder.
    fn toggle_fold(&mut self) -> bool {
        match (self.active_pane, self.screen) {
            (Pane::Tasks, Screen::Today) => self.keep_focus(|state| {
                state.overdue_section_collapsed = !state.overdue_section_collapsed;
            }),
            (Pane::Tasks, Screen::Inbox | Screen::Projects) => self.toggle_collapse(),
            (Pane::Browse, Screen::Projects) => return self.toggle_folder(),
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
                window: CompletedWindow::latest(self.today),
            }));
        }
    }

    /// The listed project and the oldest window of its completed tasks
    /// loaded, while the list filter shows them.
    pub(crate) fn listed_oldest_completed(&self) -> Option<(String, CompletedWindow)> {
        if !matches!(self.list_filter, ListFilter::Completed | ListFilter::All) {
            return None;
        }
        let pid = &self.list_project()?.id;
        let oldest = self.oldest_completed.get(pid)?;
        Some((pid.clone(), *oldest))
    }

    /// Asks for the listed project's completed tasks in the window before
    /// the oldest one loaded.
    fn load_older_completed(&mut self, effects: &mut Vec<Effect>) -> bool {
        let Some((pid, oldest)) = self.listed_oldest_completed() else {
            return false;
        };
        effects.push(Effect::FetchCompletedTasks(CompletedQuery {
            project_id: Some(pid),
            window: oldest.before(),
        }));
        false
    }

    fn open_detail(&mut self, effects: &mut Vec<Effect>) {
        let visible = self.visible_tasks();
        let Some(task) = visible.get(self.selected_task) else {
            return;
        };
        let task_id = task.id.clone();

        self.active_pane = Pane::Detail;
        self.detail_scroll = 0;

        // Serve cached comments immediately, refresh in background.
        if let Some(cached) = self.store.comments_by_task.get(&task_id) {
            self.comments = cached.clone();
        } else {
            self.comments.clear();
        }
        self.fetch_comments(task_id, effects);
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

        let before = self.store.tasks.by_id(&task_id).cloned();
        if let Some(t) = self.store.tasks.by_id_mut(&task_id) {
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

    /// Opens the add-task form on the project in view, else the Inbox, due
    /// today on Today.
    fn open_form(&mut self) {
        let project_id = self
            .list_project()
            .or_else(|| self.store.projects.iter().find(|p| p.is_inbox()))
            .map(|p| p.id.clone())
            .unwrap_or_default();
        let due_string = match self.screen {
            Screen::Today => TODAY.to_owned(),
            _ => String::new(),
        };
        self.modals
            .push(Modal::Form(TaskForm::new(project_id, due_string)));
        self.input = TextField::default();
    }

    /// Enter in the prompt: sends what it writes, or keeps it open with the
    /// reason when the text cannot be sent.
    fn submit_prompt(&mut self, effects: &mut Vec<Effect>) {
        let text = self.input.text().trim().to_owned();
        let Some(Modal::Prompt(prompt)) = self.modals.last_mut() else {
            return;
        };
        if let Some(reason) = prompt.field.refusal(&text) {
            prompt.error = Some(reason);
            return;
        }
        let field = prompt.field;
        match field {
            PromptField::Comment => self.submit_comment(text, effects),
            _ => self.submit_field_edit(field, text, effects),
        }
        self.cancel_input();
    }

    /// Enter in the add-task form: on a text row it adds the task; on the
    /// priority or the project it opens that row's picker.
    fn accept_form(&mut self, effects: &mut Vec<Effect>) -> bool {
        let Some(Modal::Form(form)) = self.top_modal() else {
            return false;
        };
        match form.field {
            FormField::Priority => open_picker(self, PickerKind::Priority),
            FormField::Project => open_picker(self, PickerKind::Project),
            FormField::Content | FormField::Due => {
                self.commit_form_field();
                self.submit_task_form(effects);
            }
        }
        true
    }

    /// Keeps the typed text in the form's focused text row.
    fn commit_form_field(&mut self) {
        let text = self.input.text().to_owned();
        if let Some(Modal::Form(form)) = self.modals.last_mut() {
            match form.field {
                FormField::Content => form.content = text,
                FormField::Due => form.due_string = text,
                FormField::Priority | FormField::Project => {}
            }
        }
    }

    /// Focuses `field` of the add-task form, loading its text into the
    /// input when it takes text.
    fn focus_form_field(&mut self, field: FormField) {
        let Some(Modal::Form(form)) = self.modals.last_mut() else {
            return;
        };
        form.field = field;
        self.input = match field {
            FormField::Content => TextField::new(form.content.clone()),
            FormField::Due => TextField::new(form.due_string.clone()),
            FormField::Priority | FormField::Project => TextField::default(),
        };
    }

    /// Moves the add-task form to the row `step` away, wrapping, keeping
    /// what was typed.
    fn move_form_field(&mut self, step: isize) -> bool {
        self.commit_form_field();
        forget_refusal(self);
        let Some(Modal::Form(form)) = self.top_modal() else {
            return false;
        };
        let field = form.field.step(step);
        self.focus_form_field(field);
        true
    }

    /// Adds the form's task: at once as a row with a temp id, which the
    /// answer to `item_add` replaces with the real one. Empty content keeps
    /// the form open on the content row with the reason.
    fn submit_task_form(&mut self, effects: &mut Vec<Effect>) {
        let Some(Modal::Form(form)) = self.modals.last_mut() else {
            return;
        };
        let content = form.content.trim().to_owned();
        if let Some(reason) = PromptField::Content.refusal(&content) {
            form.error = Some(reason);
            self.focus_form_field(FormField::Content);
            return;
        }
        let form = form.clone();
        let due_string = form.due_string.trim().to_owned();
        let temp_id = self.new_temp_id();

        // The one due string known without Todoist: Today's default, so the
        // row shows on Today at once.
        let due = due_string.eq_ignore_ascii_case(TODAY).then(|| Due {
            date: self.today.to_string(),
            string: Some(due_string.clone()),
            ..Due::default()
        });
        self.store.tasks.upsert(Task {
            id: temp_id.clone(),
            content: content.clone(),
            project_id: form.project_id.clone(),
            priority: form.priority,
            due,
            ..Task::default()
        });

        let mut args = serde_json::json!({
            "content": content,
            "project_id": form.project_id,
        });
        if !due_string.is_empty() {
            args["due_string"] = serde_json::Value::String(due_string);
        }
        if form.priority > 1 {
            args["priority"] = serde_json::Value::from(form.priority);
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
            posted_at: Some(self.now.timestamp().to_string()),
            posted_by_uid: self.store.current_user_id.clone(),
            task_id: Some(task_id.clone()),
            item_id: Some(task_id.clone()),
            ..Comment::default()
        };
        // The comment list is the shown detail's; a comment shows there
        // when it is on the focused task, whose detail that is.
        if self.selected_task().map(|t| t.id.as_str()) == Some(task_id.as_str()) {
            let other_task = |c: &Comment| {
                c.item_id.as_deref().or(c.task_id.as_deref()) != Some(task_id.as_str())
            };
            if self.comments.iter().any(other_task) {
                self.comments = self
                    .store
                    .comments_by_task
                    .get(&task_id)
                    .cloned()
                    .unwrap_or_default();
            }
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

    fn submit_field_edit(&mut self, field: PromptField, value: String, effects: &mut Vec<Effect>) {
        let Some(before) = self.edit_target() else {
            return;
        };
        let task_id = before.id.clone();

        let args = match field {
            PromptField::Content => {
                if let Some(t) = self.store.tasks.by_id_mut(&task_id) {
                    t.content = value.clone();
                }
                serde_json::json!({ "id": task_id, "content": value })
            }
            PromptField::DueDate => {
                // Todoist parses the string and sends the Due object back with
                // a later sync; until then the task shows what was typed.
                self.pending_due.insert(task_id.clone(), value.clone());
                serde_json::json!({ "id": task_id, "due_string": value })
            }
            PromptField::Description => {
                if let Some(t) = self.store.tasks.by_id_mut(&task_id) {
                    t.description = value.clone();
                }
                serde_json::json!({ "id": task_id, "description": value })
            }
            PromptField::Comment => return,
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
            .retain(|m| !matches!(m, Modal::Prompt(_) | Modal::Form(_)));
        self.input = TextField::default();
        self.edit_task = None;
    }

    /// The task the closing prompt or priority picker opened on, as it is
    /// now; `None` once it is gone.
    fn edit_target(&mut self) -> Option<Task> {
        let id = self.edit_task.take()?;
        self.store.tasks.by_id(&id).cloned()
    }

    fn apply_priority(&mut self, new_priority: u8, effects: &mut Vec<Effect>) {
        let Some(before) = self.edit_target() else {
            return;
        };
        if before.priority == new_priority {
            return;
        }
        let task_id = before.id.clone();

        if let Some(t) = self.store.tasks.by_id_mut(&task_id) {
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
            Action::LineDown | Action::MoveDown => current.saturating_add(1),
            Action::LineUp | Action::MoveUp => current.saturating_sub(1),
            Action::MoveToTop => 0,
            Action::MoveToBottom => max,
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
            .store
            .tasks
            .iter()
            .filter_map(|t| t.parent_id.clone())
            .collect();
        for task in &self.store.tasks {
            if parent_ids.contains(&task.id) {
                self.collapsed.insert(task.id.clone());
            }
        }
    }
}

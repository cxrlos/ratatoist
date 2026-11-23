//! Key dispatch for the Vim (Normal, Insert) and Standard modes. A key moves
//! the cursor or opens an overlay directly; anything with more to it comes
//! back as a [`KeyAction`] for [`crate::app::update`] to carry out.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{DOCK_ITEMS, InputMode, Modal, Pane, ProjectNavItem, State, VimState};

pub enum KeyAction {
    Quit,
    ProjectChanged,
    OpenDetail,
    CloseDetail,
    ToggleSettings,
    ToggleHelp,
    ToggleMode,
    ToggleCollapse,
    ToggleFolderCollapse,
    OpenAllFolds,
    CloseAllFolds,
    CompleteTask,
    SelectPriority,
    StarProject,
    CycleFilter,
    CycleSort,
    ForceResync,
    StartInput,
    StartCommentInput,
    StartFieldEdit,
    SubmitInput,
    SubmitForm,
    FormFieldUp,
    FormFieldDown,
    FormEditField,
    FormEscNormal,
    CancelInput,
    DetailFieldUp,
    DetailFieldDown,
    OpenThemePicker,
    SelectTheme,
    CloseThemePicker,
    TodayViewSelected,
    ToggleOverdueSection,
    CycleIdleTimeout,
    Consumed,
    None,
}

/// Keys while the first sync runs: only quitting works.
pub fn handle_key_while_loading(key: KeyEvent) -> KeyAction {
    let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
    if ctrl_c || key.code == KeyCode::Char('q') {
        KeyAction::Quit
    } else {
        KeyAction::None
    }
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> KeyAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return KeyAction::Quit;
    }

    match state.top_modal() {
        Some(Modal::Help) => {
            return match key.code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => KeyAction::ToggleHelp,
                _ => KeyAction::Consumed,
            };
        }
        Some(Modal::PriorityPicker { .. }) => return handle_priority_picker(state, key),
        Some(Modal::TaskForm(form)) if form.editing => return handle_input(state, key),
        Some(Modal::TaskForm(_)) => return handle_form_nav(state, key),
        Some(Modal::Input(_)) => return handle_input(state, key),
        Some(Modal::ThemePicker { .. }) => return handle_theme_picker(state, key),
        None => {}
    }

    if matches!(state.active_pane, Pane::Settings) {
        return handle_settings(state, key);
    }

    if matches!(state.active_pane, Pane::Detail) {
        return handle_detail(state, key);
    }

    if state.dock_focus.is_some() {
        return handle_dock_nav(state, key);
    }

    match state.input_mode {
        InputMode::Vim(vim) => handle_vim(state, key, vim),
        InputMode::Standard => handle_standard(state, key),
    }
}

fn handle_dock_nav(state: &mut State, key: KeyEvent) -> KeyAction {
    let focus = state.dock_focus.unwrap_or(0);

    match key.code {
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
            if focus + 1 >= DOCK_ITEMS.len() {
                state.dock_focus = None;
                state.active_pane = Pane::Projects;
            } else {
                state.dock_focus = Some(focus + 1);
            }
            KeyAction::Consumed
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::BackTab => {
            if focus == 0 {
                state.dock_focus = None;
                state.active_pane = Pane::Tasks;
            } else {
                state.dock_focus = Some(focus - 1);
            }
            KeyAction::Consumed
        }
        KeyCode::Char('j') | KeyCode::Down => {
            state.dock_focus = Some((focus + 1) % DOCK_ITEMS.len());
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.dock_focus = Some(if focus == 0 {
                DOCK_ITEMS.len() - 1
            } else {
                focus - 1
            });
            KeyAction::Consumed
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let item = DOCK_ITEMS[focus];
            state.dock_filter = if state.dock_filter == Some(item) {
                None
            } else {
                Some(item)
            };
            state.dock_focus = None;
            state.active_pane = Pane::Tasks;
            let visible_len = state.visible_tasks().len();
            state.selected_task = state.selected_task.min(visible_len.saturating_sub(1));
            KeyAction::Consumed
        }
        KeyCode::Esc => {
            state.dock_focus = None;
            state.dock_filter = None;
            state.active_pane = Pane::Projects;
            let visible_len = state.visible_tasks().len();
            state.selected_task = state.selected_task.min(visible_len.saturating_sub(1));
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

fn handle_input(state: &mut State, key: KeyEvent) -> KeyAction {
    let form_field = match state.top_modal() {
        Some(Modal::TaskForm(form)) => Some(form.active_field),
        _ => None,
    };

    match key.code {
        KeyCode::Esc => {
            if let Some(field) = form_field {
                if field == 0 {
                    KeyAction::CancelInput
                } else {
                    KeyAction::FormEscNormal
                }
            } else if matches!(state.input_mode, InputMode::Standard) {
                KeyAction::CancelInput
            } else {
                KeyAction::SubmitInput
            }
        }
        KeyCode::Enter => KeyAction::SubmitInput,
        KeyCode::Backspace => {
            state.input_buffer.pop();
            KeyAction::Consumed
        }
        KeyCode::Char(c) => {
            state.input_buffer.push(c);
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

fn handle_form_nav(state: &mut State, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Char('q') => KeyAction::CancelInput,
        KeyCode::Esc => {
            if let Some(Modal::TaskForm(form)) = state.modals.last_mut() {
                if form.active_field == 0 {
                    return KeyAction::CancelInput;
                }
                form.active_field = 0;
                form.editing = true;
                state.input_buffer = form.content.clone();
                if let InputMode::Vim(_) = state.input_mode {
                    state.input_mode = InputMode::Vim(VimState::Insert);
                }
            }
            KeyAction::Consumed
        }
        KeyCode::Char('j') | KeyCode::Down => KeyAction::FormFieldDown,
        KeyCode::Char('k') | KeyCode::Up => KeyAction::FormFieldUp,
        KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char(' ') => KeyAction::FormEditField,
        KeyCode::Tab => KeyAction::SubmitForm,
        _ => KeyAction::Consumed,
    }
}

fn handle_theme_picker(state: &mut State, key: KeyEvent) -> KeyAction {
    let count = state.themes.len();
    let Some(Modal::ThemePicker { selection }) = state.modals.last_mut() else {
        return KeyAction::Consumed;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => KeyAction::CloseThemePicker,
        KeyCode::Char('j') | KeyCode::Down => {
            *selection = (*selection + 1) % count.max(1);
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if count == 0 {
                return KeyAction::Consumed;
            }
            *selection = selection.checked_sub(1).unwrap_or(count - 1);
            KeyAction::Consumed
        }
        KeyCode::Enter | KeyCode::Char(' ') => KeyAction::SelectTheme,
        _ => KeyAction::Consumed,
    }
}

fn handle_priority_picker(state: &mut State, key: KeyEvent) -> KeyAction {
    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
        state.modals.pop();
        return KeyAction::Consumed;
    }
    let Some(Modal::PriorityPicker { selection }) = state.modals.last_mut() else {
        return KeyAction::Consumed;
    };
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            *selection = match *selection {
                4 => 3,
                3 => 2,
                2 => 1,
                _ => 4,
            };
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            *selection = match *selection {
                1 => 2,
                2 => 3,
                3 => 4,
                _ => 1,
            };
            KeyAction::Consumed
        }
        KeyCode::Char('1') => {
            *selection = 4;
            KeyAction::SelectPriority
        }
        KeyCode::Char('2') => {
            *selection = 3;
            KeyAction::SelectPriority
        }
        KeyCode::Char('3') => {
            *selection = 2;
            KeyAction::SelectPriority
        }
        KeyCode::Char('4') => {
            *selection = 1;
            KeyAction::SelectPriority
        }
        KeyCode::Enter | KeyCode::Char(' ') => KeyAction::SelectPriority,
        _ => KeyAction::Consumed,
    }
}

fn handle_detail(_state: &mut State, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left | KeyCode::BackTab => {
            KeyAction::CloseDetail
        }
        KeyCode::Char('q') => KeyAction::Quit,
        KeyCode::Char('?') => KeyAction::ToggleHelp,
        KeyCode::Char('x') => KeyAction::CompleteTask,
        KeyCode::Char('c') => KeyAction::StartCommentInput,
        KeyCode::Char('i') | KeyCode::Enter => KeyAction::StartFieldEdit,
        KeyCode::Char('j') | KeyCode::Down => KeyAction::DetailFieldDown,
        KeyCode::Char('k') | KeyCode::Up => KeyAction::DetailFieldUp,
        _ => KeyAction::None,
    }
}

fn handle_settings(state: &mut State, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => KeyAction::ToggleSettings,

        KeyCode::Char('j') | KeyCode::Down => {
            state.settings_selection = (state.settings_selection + 1) % settings_item_count();
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if state.settings_selection == 0 {
                state.settings_selection = settings_item_count() - 1;
            } else {
                state.settings_selection -= 1;
            }
            KeyAction::Consumed
        }

        KeyCode::Enter | KeyCode::Char(' ') => {
            match state.settings_selection {
                0 => return KeyAction::ToggleMode,
                1 => return KeyAction::OpenThemePicker,
                2 => return KeyAction::CycleIdleTimeout,
                _ => {}
            }
            KeyAction::Consumed
        }

        _ => KeyAction::None,
    }
}

fn settings_item_count() -> usize {
    3
}

fn handle_vim(state: &mut State, key: KeyEvent, vim: VimState) -> KeyAction {
    match vim {
        VimState::Normal => handle_vim_normal(state, key),
        VimState::Insert => handle_vim_insert(state, key),
    }
}

fn handle_vim_normal(state: &mut State, key: KeyEvent) -> KeyAction {
    if std::mem::take(&mut state.pending_z) {
        return match key.code {
            KeyCode::Char('a') if matches!(state.active_pane, Pane::Tasks) => {
                KeyAction::ToggleCollapse
            }
            KeyCode::Char('R') => KeyAction::OpenAllFolds,
            KeyCode::Char('M') => KeyAction::CloseAllFolds,
            _ => KeyAction::Consumed,
        };
    }

    match key.code {
        KeyCode::Char('q') => KeyAction::Quit,
        KeyCode::Char('?') => KeyAction::ToggleHelp,
        KeyCode::Char(',') => KeyAction::ToggleSettings,
        KeyCode::Char('R') => KeyAction::ForceResync,

        KeyCode::Char('z') => {
            state.pending_z = true;
            KeyAction::Consumed
        }

        KeyCode::Char('x') if matches!(state.active_pane, Pane::Tasks) => KeyAction::CompleteTask,
        KeyCode::Char('a') if matches!(state.active_pane, Pane::Tasks) => KeyAction::StartInput,
        KeyCode::Char('f') if matches!(state.active_pane, Pane::Tasks) => KeyAction::CycleFilter,
        KeyCode::Char('o') if matches!(state.active_pane, Pane::Tasks) => KeyAction::CycleSort,
        KeyCode::Char('s') if matches!(state.active_pane, Pane::Projects) => KeyAction::StarProject,

        KeyCode::Char('j') | KeyCode::Down => move_in_pane(state, 1),
        KeyCode::Char('k') | KeyCode::Up => move_in_pane(state, -1),

        KeyCode::Char('g') => jump_to_edge(state, true),
        KeyCode::Char('G') => jump_to_edge(state, false),

        KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
            match state.active_pane {
                Pane::Projects => state.active_pane = Pane::Tasks,
                Pane::Tasks => {
                    state.dock_focus = Some(0);
                    state.active_pane = Pane::StatsDock;
                }
                _ => {}
            }
            KeyAction::Consumed
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::BackTab => {
            match state.active_pane {
                Pane::Tasks => state.active_pane = Pane::Projects,
                Pane::Projects => {
                    state.dock_focus = Some(DOCK_ITEMS.len() - 1);
                    state.active_pane = Pane::StatsDock;
                }
                _ => {}
            }
            KeyAction::Consumed
        }

        KeyCode::Enter => match state.active_pane {
            Pane::Projects => {
                state.active_pane = Pane::Tasks;
                KeyAction::Consumed
            }
            Pane::Tasks => KeyAction::OpenDetail,
            _ => KeyAction::Consumed,
        },

        KeyCode::Char(' ')
            if matches!(state.active_pane, Pane::Tasks) && state.today_view_active =>
        {
            KeyAction::ToggleOverdueSection
        }
        KeyCode::Char(' ') if matches!(state.active_pane, Pane::Tasks) => KeyAction::ToggleCollapse,
        KeyCode::Char(' ') if matches!(state.active_pane, Pane::Projects) => {
            KeyAction::ToggleFolderCollapse
        }

        KeyCode::Esc => {
            if matches!(state.active_pane, Pane::Tasks) {
                if state.dock_filter.is_some() {
                    state.dock_filter = None;
                    let visible_len = state.visible_tasks().len();
                    state.selected_task = state.selected_task.min(visible_len.saturating_sub(1));
                } else {
                    state.active_pane = Pane::Projects;
                }
                KeyAction::Consumed
            } else {
                KeyAction::None
            }
        }

        _ => KeyAction::None,
    }
}

fn handle_vim_insert(_state: &mut State, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc => KeyAction::CancelInput,
        KeyCode::Enter => KeyAction::SubmitInput,
        _ => KeyAction::Consumed,
    }
}

fn handle_standard(state: &mut State, key: KeyEvent) -> KeyAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('a') if matches!(state.active_pane, Pane::Tasks) => KeyAction::StartInput,
            KeyCode::Char('x') if matches!(state.active_pane, Pane::Tasks) => {
                KeyAction::CompleteTask
            }
            _ => KeyAction::None,
        };
    }

    match key.code {
        KeyCode::Char('q') => KeyAction::Quit,
        KeyCode::Char('?') => KeyAction::ToggleHelp,
        KeyCode::Char(',') => KeyAction::ToggleSettings,
        KeyCode::Char('R') => KeyAction::ForceResync,
        KeyCode::Char('f') if matches!(state.active_pane, Pane::Tasks) => KeyAction::CycleFilter,

        KeyCode::Down => move_in_pane(state, 1),
        KeyCode::Up => move_in_pane(state, -1),

        KeyCode::Home => jump_to_edge(state, true),
        KeyCode::End => jump_to_edge(state, false),

        KeyCode::Right | KeyCode::Tab => {
            match state.active_pane {
                Pane::Projects => state.active_pane = Pane::Tasks,
                Pane::Tasks => {
                    state.dock_focus = Some(0);
                    state.active_pane = Pane::StatsDock;
                }
                _ => {}
            }
            KeyAction::Consumed
        }
        KeyCode::Left | KeyCode::BackTab => {
            match state.active_pane {
                Pane::Tasks => state.active_pane = Pane::Projects,
                Pane::Projects => {
                    state.dock_focus = Some(DOCK_ITEMS.len() - 1);
                    state.active_pane = Pane::StatsDock;
                }
                _ => {}
            }
            KeyAction::Consumed
        }

        KeyCode::Enter => match state.active_pane {
            Pane::Projects => {
                state.active_pane = Pane::Tasks;
                KeyAction::Consumed
            }
            Pane::Tasks => KeyAction::OpenDetail,
            _ => KeyAction::Consumed,
        },

        KeyCode::Esc => {
            if matches!(state.active_pane, Pane::Tasks) {
                if state.dock_filter.is_some() {
                    state.dock_filter = None;
                    let visible_len = state.visible_tasks().len();
                    state.selected_task = state.selected_task.min(visible_len.saturating_sub(1));
                } else {
                    state.active_pane = Pane::Projects;
                }
                KeyAction::Consumed
            } else {
                KeyAction::None
            }
        }

        _ => KeyAction::None,
    }
}

fn move_in_pane(state: &mut State, delta: i32) -> KeyAction {
    match state.active_pane {
        Pane::Projects => {
            let nav = state.visible_nav_items();
            if nav.is_empty() {
                return KeyAction::Consumed;
            }
            let pos = nav
                .iter()
                .position(|item| match item {
                    ProjectNavItem::Project(i) => {
                        !state.today_view_active
                            && state.folder_cursor.is_none()
                            && *i == state.selected_project
                    }
                    ProjectNavItem::Folder(fi) => state.folder_cursor == Some(*fi),
                    ProjectNavItem::TodayView => {
                        state.today_view_active && state.folder_cursor.is_none()
                    }
                })
                .unwrap_or(0) as i32;
            let next_pos = pos + delta;
            if next_pos >= nav.len() as i32 {
                state.dock_focus = Some(0);
                state.active_pane = Pane::StatsDock;
                return KeyAction::Consumed;
            }
            if next_pos < 0 {
                return KeyAction::Consumed;
            }
            match nav[next_pos as usize] {
                ProjectNavItem::Project(i) => {
                    state.folder_cursor = None;
                    state.selected_project = i;
                    KeyAction::ProjectChanged
                }
                ProjectNavItem::Folder(fi) => {
                    state.folder_cursor = Some(fi);
                    KeyAction::Consumed
                }
                ProjectNavItem::TodayView => {
                    state.folder_cursor = None;
                    KeyAction::TodayViewSelected
                }
            }
        }
        Pane::Tasks => {
            let visible = state.visible_tasks();
            let visible_len = visible.len();
            if visible_len == 0 {
                return KeyAction::Consumed;
            }
            let current = state.selected_task as i32;
            let mut next = (current + delta).rem_euclid(visible_len as i32) as usize;
            // Skip context rows (dimmed active parents shown in Done filter).
            for _ in 0..visible_len {
                if !state.is_context_task(visible[next]) {
                    break;
                }
                next = ((next as i32) + delta).rem_euclid(visible_len as i32) as usize;
            }
            state.selected_task = next;
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

fn jump_to_edge(state: &mut State, top: bool) -> KeyAction {
    match state.active_pane {
        Pane::Projects => {
            let nav = state.visible_nav_items();
            let item = if top { nav.first() } else { nav.last() };
            match item {
                Some(ProjectNavItem::Project(i)) => {
                    let i = *i;
                    state.folder_cursor = None;
                    if state.selected_project != i {
                        state.selected_project = i;
                        return KeyAction::ProjectChanged;
                    }
                }
                Some(ProjectNavItem::Folder(fi)) => {
                    state.folder_cursor = Some(*fi);
                }
                Some(ProjectNavItem::TodayView) => {
                    state.folder_cursor = None;
                    return KeyAction::TodayViewSelected;
                }
                None => {}
            }
            KeyAction::Consumed
        }
        Pane::Tasks => {
            let visible_len = state.visible_tasks().len();
            state.selected_task = if top {
                0
            } else {
                visible_len.saturating_sub(1)
            };
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

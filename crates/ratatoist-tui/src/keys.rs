use std::sync::Mutex;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, InputMode, Pane, VimState};

pub enum KeyAction {
    Quit,
    ProjectChanged,
    OpenDetail,
    CloseDetail,
    ToggleSettings,
    ToggleHelp,
    ToggleMode,
    ToggleCollapse,
    OpenAllFolds,
    CloseAllFolds,
    CompleteTask,
    #[allow(dead_code)]
    OpenPriorityPicker,
    SelectPriority,
    StarProject,
    CycleSort,
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
    Consumed,
    None,
}

static PENDING_Z: Mutex<bool> = Mutex::new(false);

fn take_pending_z() -> bool {
    let mut pending = PENDING_Z.lock().unwrap();
    let was = *pending;
    *pending = false;
    was
}

fn set_pending_z() {
    *PENDING_Z.lock().unwrap() = true;
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> KeyAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return KeyAction::Quit;
    }

    if app.show_help {
        return match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => KeyAction::ToggleHelp,
            _ => KeyAction::Consumed,
        };
    }

    if let Some(form) = &app.task_form {
        if form.editing {
            return handle_input(app, key);
        }
        return handle_form_nav(app, key);
    }

    if app.show_input {
        return handle_input(app, key);
    }

    if app.show_theme_picker {
        return handle_theme_picker(app, key);
    }

    if matches!(app.active_pane, Pane::Settings) {
        return handle_settings(app, key);
    }

    if app.show_priority_picker {
        return handle_priority_picker(app, key);
    }

    if matches!(app.active_pane, Pane::Detail) {
        return handle_detail(app, key);
    }

    match app.input_mode {
        InputMode::Vim(state) => handle_vim(app, key, state),
        InputMode::Standard => handle_standard(app, key),
    }
}

fn handle_input(app: &mut App, key: KeyEvent) -> KeyAction {
    let in_form = app.task_form.is_some();

    match key.code {
        KeyCode::Esc => {
            if in_form {
                let on_content = app
                    .task_form
                    .as_ref()
                    .map(|f| f.active_field == 0)
                    .unwrap_or(false);
                if on_content {
                    KeyAction::CancelInput
                } else {
                    KeyAction::FormEscNormal
                }
            } else if !app.input_mode.is_vim() {
                KeyAction::CancelInput
            } else {
                KeyAction::SubmitInput
            }
        }
        KeyCode::Enter => KeyAction::SubmitInput,
        KeyCode::Backspace => {
            app.input_buffer.pop();
            KeyAction::Consumed
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

fn handle_form_nav(app: &mut App, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Char('q') => KeyAction::CancelInput,
        KeyCode::Esc => {
            if let Some(form) = &mut app.task_form {
                if form.active_field == 0 {
                    return KeyAction::CancelInput;
                }
                form.active_field = 0;
                form.editing = true;
                app.input_buffer = form.content.clone();
                app.show_input = true;
                if let InputMode::Vim(_) = app.input_mode {
                    app.input_mode = InputMode::Vim(VimState::Insert);
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

fn handle_theme_picker(app: &mut App, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => KeyAction::CloseThemePicker,
        KeyCode::Char('j') | KeyCode::Down => {
            app.theme_selection = (app.theme_selection + 1) % app.themes.len().max(1);
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.themes.is_empty() {
                return KeyAction::Consumed;
            }
            app.theme_selection = app.theme_selection
                .checked_sub(1)
                .unwrap_or(app.themes.len() - 1);
            KeyAction::Consumed
        }
        KeyCode::Enter | KeyCode::Char(' ') => KeyAction::SelectTheme,
        _ => KeyAction::Consumed,
    }
}

fn handle_priority_picker(app: &mut App, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.show_priority_picker = false;
            KeyAction::Consumed
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.priority_selection = match app.priority_selection {
                4 => 3,
                3 => 2,
                2 => 1,
                _ => 4,
            };
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.priority_selection = match app.priority_selection {
                1 => 2,
                2 => 3,
                3 => 4,
                _ => 1,
            };
            KeyAction::Consumed
        }
        KeyCode::Char('1') => {
            app.priority_selection = 4;
            KeyAction::SelectPriority
        }
        KeyCode::Char('2') => {
            app.priority_selection = 3;
            KeyAction::SelectPriority
        }
        KeyCode::Char('3') => {
            app.priority_selection = 2;
            KeyAction::SelectPriority
        }
        KeyCode::Char('4') => {
            app.priority_selection = 1;
            KeyAction::SelectPriority
        }
        KeyCode::Enter | KeyCode::Char(' ') => KeyAction::SelectPriority,
        _ => KeyAction::Consumed,
    }
}

fn handle_detail(_app: &mut App, key: KeyEvent) -> KeyAction {
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

fn handle_settings(app: &mut App, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => KeyAction::ToggleSettings,

        KeyCode::Char('j') | KeyCode::Down => {
            app.settings_selection = (app.settings_selection + 1) % settings_item_count();
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.settings_selection == 0 {
                app.settings_selection = settings_item_count() - 1;
            } else {
                app.settings_selection -= 1;
            }
            KeyAction::Consumed
        }

        KeyCode::Enter | KeyCode::Char(' ') => {
            match app.settings_selection {
                0 => return KeyAction::ToggleMode,
                1 => return KeyAction::OpenThemePicker,
                _ => {}
            }
            KeyAction::Consumed
        }

        _ => KeyAction::None,
    }
}

fn settings_item_count() -> usize {
    2
}

fn handle_vim(app: &mut App, key: KeyEvent, state: VimState) -> KeyAction {
    match state {
        VimState::Normal => handle_vim_normal(app, key),
        VimState::Visual => handle_vim_visual(app, key),
        VimState::Insert => handle_vim_insert(app, key),
    }
}

fn handle_vim_normal(app: &mut App, key: KeyEvent) -> KeyAction {
    if take_pending_z() {
        return match key.code {
            KeyCode::Char('a') if matches!(app.active_pane, Pane::Tasks) => {
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

        KeyCode::Char('z') => {
            set_pending_z();
            KeyAction::Consumed
        }

        KeyCode::Char('x') if matches!(app.active_pane, Pane::Tasks) => KeyAction::CompleteTask,
        KeyCode::Char('a') if matches!(app.active_pane, Pane::Tasks) => KeyAction::StartInput,
        KeyCode::Char('o') if matches!(app.active_pane, Pane::Tasks) => KeyAction::CycleSort,
        KeyCode::Char('s') if matches!(app.active_pane, Pane::Projects) => KeyAction::StarProject,

        KeyCode::Char('j') | KeyCode::Down => move_in_pane(app, 1),
        KeyCode::Char('k') | KeyCode::Up => move_in_pane(app, -1),

        KeyCode::Char('g') => jump_to_edge(app, true),
        KeyCode::Char('G') => jump_to_edge(app, false),

        KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
            if matches!(app.active_pane, Pane::Projects) {
                app.active_pane = Pane::Tasks;
            }
            KeyAction::Consumed
        }
        KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
            if matches!(app.active_pane, Pane::Tasks) {
                app.active_pane = Pane::Projects;
            }
            KeyAction::Consumed
        }

        KeyCode::Enter => match app.active_pane {
            Pane::Projects => {
                app.active_pane = Pane::Tasks;
                KeyAction::Consumed
            }
            Pane::Tasks => KeyAction::OpenDetail,
            _ => KeyAction::Consumed,
        },

        KeyCode::Char(' ') if matches!(app.active_pane, Pane::Tasks) => KeyAction::ToggleCollapse,

        KeyCode::Esc => {
            if matches!(app.active_pane, Pane::Tasks) {
                app.active_pane = Pane::Projects;
                KeyAction::Consumed
            } else {
                KeyAction::None
            }
        }

        _ => KeyAction::None,
    }
}

fn handle_vim_visual(_app: &mut App, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc => KeyAction::Consumed,
        _ => KeyAction::None,
    }
}

fn handle_vim_insert(_app: &mut App, key: KeyEvent) -> KeyAction {
    match key.code {
        KeyCode::Esc => KeyAction::CancelInput,
        KeyCode::Enter => KeyAction::SubmitInput,
        _ => KeyAction::Consumed,
    }
}

fn handle_standard(app: &mut App, key: KeyEvent) -> KeyAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('a') if matches!(app.active_pane, Pane::Tasks) => KeyAction::StartInput,
            KeyCode::Char('x') if matches!(app.active_pane, Pane::Tasks) => KeyAction::CompleteTask,
            _ => KeyAction::None,
        };
    }

    match key.code {
        KeyCode::Char('q') => KeyAction::Quit,
        KeyCode::Char('?') => KeyAction::ToggleHelp,
        KeyCode::Char(',') => KeyAction::ToggleSettings,

        KeyCode::Down => move_in_pane(app, 1),
        KeyCode::Up => move_in_pane(app, -1),

        KeyCode::Home => jump_to_edge(app, true),
        KeyCode::End => jump_to_edge(app, false),

        KeyCode::Tab | KeyCode::Right => {
            if matches!(app.active_pane, Pane::Projects) {
                app.active_pane = Pane::Tasks;
            }
            KeyAction::Consumed
        }
        KeyCode::BackTab | KeyCode::Left => {
            if matches!(app.active_pane, Pane::Tasks) {
                app.active_pane = Pane::Projects;
            }
            KeyAction::Consumed
        }

        KeyCode::Enter => match app.active_pane {
            Pane::Projects => {
                app.active_pane = Pane::Tasks;
                KeyAction::Consumed
            }
            Pane::Tasks => KeyAction::OpenDetail,
            _ => KeyAction::Consumed,
        },

        KeyCode::Esc => {
            if matches!(app.active_pane, Pane::Tasks) {
                app.active_pane = Pane::Projects;
                KeyAction::Consumed
            } else {
                KeyAction::None
            }
        }

        _ => KeyAction::None,
    }
}

fn move_in_pane(app: &mut App, delta: i32) -> KeyAction {
    match app.active_pane {
        Pane::Projects => {
            let len = app.projects.len();
            if len == 0 {
                return KeyAction::Consumed;
            }
            let current = app.selected_project as i32;
            let next = (current + delta).rem_euclid(len as i32) as usize;
            app.selected_project = next;
            KeyAction::ProjectChanged
        }
        Pane::Tasks => {
            let visible_len = app.visible_tasks().len();
            if visible_len == 0 {
                return KeyAction::Consumed;
            }
            let current = app.selected_task as i32;
            let next = (current + delta).rem_euclid(visible_len as i32) as usize;
            app.selected_task = next;
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

fn jump_to_edge(app: &mut App, top: bool) -> KeyAction {
    match app.active_pane {
        Pane::Projects => {
            let new = if top {
                0
            } else {
                app.projects.len().saturating_sub(1)
            };
            if app.selected_project != new {
                app.selected_project = new;
                return KeyAction::ProjectChanged;
            }
            KeyAction::Consumed
        }
        Pane::Tasks => {
            let visible_len = app.visible_tasks().len();
            app.selected_task = if top {
                0
            } else {
                visible_len.saturating_sub(1)
            };
            KeyAction::Consumed
        }
        _ => KeyAction::Consumed,
    }
}

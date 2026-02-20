use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Pane};

pub fn handle_key(app: &mut App, key: KeyEvent) -> KeyAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return KeyAction::Quit;
    }

    match key.code {
        KeyCode::Char('q') => KeyAction::Quit,

        KeyCode::Char('j') | KeyCode::Down => {
            move_selection(app, 1);
            KeyAction::Consumed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            move_selection(app, -1);
            KeyAction::Consumed
        }

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

        KeyCode::Enter => {
            if matches!(app.active_pane, Pane::Projects) {
                return KeyAction::LoadTasks;
            }
            KeyAction::Consumed
        }

        _ => KeyAction::None,
    }
}

fn move_selection(app: &mut App, delta: i32) {
    match app.active_pane {
        Pane::Projects => {
            let len = app.projects.len();
            if len == 0 {
                return;
            }
            let current = app.selected_project as i32;
            let next = (current + delta).rem_euclid(len as i32) as usize;
            app.selected_project = next;
        }
        Pane::Tasks => {
            let len = app.tasks.len();
            if len == 0 {
                return;
            }
            let current = app.selected_task as i32;
            let next = (current + delta).rem_euclid(len as i32) as usize;
            app.selected_task = next;
        }
    }
}

pub enum KeyAction {
    Quit,
    LoadTasks,
    Consumed,
    None,
}

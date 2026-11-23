pub mod components;
pub mod dates;
pub mod keyhints;
pub mod layout;
pub mod splash;
pub mod statusbar;
pub mod theme;
pub mod views;

pub const LOGO: &str = r#"
░█▀▄░█▀█░▀█▀░█▀█░▀█▀░█▀█░▀█▀░█▀▀░▀█▀
░█▀▄░█▀█░░█░░█▀█░░█░░█░█░░█░░▀▀█░░█░
░▀░▀░▀░▀░░▀░░▀░▀░░▀░░▀▀▀░▀▀▀░▀▀▀░░▀░
"#;

use ratatui::Frame;

use crate::app::{Modal, State};

/// Draws the whole screen from `state`: the splash until the first sync
/// answers, then the panes, the top overlay and any error.
pub fn draw(frame: &mut Frame, state: &State) {
    if state.loading {
        splash::render(frame, 0.3, "syncing data...", state.theme());
        return;
    }

    layout::render(frame, state);

    match state.top_modal() {
        Some(Modal::ThemePicker { selection }) => {
            components::theme_picker::render(frame, state, *selection);
        }
        Some(Modal::PriorityPicker { selection }) => {
            components::priority_picker::render(frame, *selection, state.theme());
        }
        Some(Modal::TaskForm(form)) => components::task_form::render(frame, state, form),
        Some(Modal::Input(target)) => components::input_popup::render(frame, state, *target),
        Some(Modal::Help) => {
            components::cheatsheet::render(frame, &state.input_mode, state.theme());
        }
        None => {}
    }

    if let Some(error) = &state.error {
        components::error_popup::render(frame, error, state.theme());
    }
}

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
use ratatui::widgets::Paragraph;

use crate::app::State;
use crate::app::legacy::App;

pub fn draw(frame: &mut Frame, app: &App) {
    layout::render(frame, app);

    if app.show_theme_picker {
        components::theme_picker::render(frame, app);
    } else if app.show_priority_picker {
        components::priority_picker::render(frame, app.priority_selection, app.theme());
    } else if let Some(form) = &app.task_form {
        components::task_form::render(frame, app, form);
    } else if app.show_input {
        components::input_popup::render(frame, app);
    }

    if app.show_help {
        components::cheatsheet::render(frame, &app.input_mode, app.theme());
    }

    if let Some(error) = &app.error {
        components::error_popup::render(frame, error, app.theme());
    }
}

/// Draws the event-driven core's screen.
pub fn draw_state(frame: &mut Frame, state: &State) {
    let _ = state;
    frame.render_widget(Paragraph::new(LOGO), frame.area());
}

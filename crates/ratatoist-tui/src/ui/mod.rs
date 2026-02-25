pub mod components;
pub mod dates;
pub mod keyhints;
pub mod layout;
pub mod setup;
pub mod splash;
pub mod statusbar;
pub mod theme;
pub mod views;

pub const LOGO: &str = r#"
                     $$\                $$\               $$\             $$\
                     $$ |               $$ |              \__|            $$ |
  $$$$$$\  $$$$$$\ $$$$$$\    $$$$$$\ $$$$$$\    $$$$$$\  $$\  $$$$$$$\ $$$$$$\
 $$  __$$\ \____$$\\_$$  _|   \____$$\\_$$  _|  $$  __$$\ $$ |$$  _____|\\_$$  _|
 $$ |  \__|$$$$$$$ | $$ |     $$$$$$$ | $$ |    $$ /  $$ |$$ |\$$$$$$\    $$ |
 $$ |     $$  __$$ | $$ |$$\ $$  __$$ | $$ |$$\ $$ |  $$ |$$ | \____$$\   $$ |$$\
 $$ |     \$$$$$$$ | \$$$$  |\$$$$$$$ | \$$$$  |\$$$$$$  |$$ |$$$$$$$  |  \$$$$  |
 \__|      \_______| \____/  \_______| \____/  \______/ \__|\_______/    \____/
"#;

use ratatui::Frame;

use crate::app::App;

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

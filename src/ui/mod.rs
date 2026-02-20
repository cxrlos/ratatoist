pub mod components;
pub mod keyhints;
pub mod layout;
pub mod theme;
pub mod views;

use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    layout::render(frame, app);
}

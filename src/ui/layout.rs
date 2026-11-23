//! The panes of the screens: the Projects tree and a task list.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::State;

use super::text;
use super::views;

/// The Projects tree, alone in `area`.
pub fn render_projects_block(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();
    let active = true;

    let block = Block::default()
        .title(" Projects ")
        .title_style(if active {
            theme.active_title()
        } else {
            theme.title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if active {
            theme.active_border()
        } else {
            theme.inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    views::projects::render(frame, state, inner, active);
}

/// A task list titled `title`, alone in `area`.
pub fn render_tasks_block(frame: &mut Frame, state: &State, area: Rect, title: &str) {
    let theme = state.theme();
    let active = true;

    let block = Block::default()
        .title(format!(
            " {} ",
            text::fit(title, usize::from(area.width.saturating_sub(4)))
        ))
        .title_style(if active {
            theme.active_title()
        } else {
            theme.title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if active {
            theme.active_border()
        } else {
            theme.inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);
    views::tasks::render(frame, state, inner, active);
}

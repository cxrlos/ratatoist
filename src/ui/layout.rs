use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::{App, Pane};

use super::keyhints;
use super::theme::Theme;
use super::views;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let [main_area, hints_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let [projects_area, tasks_area] =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
            .areas(main_area);

    let projects_active = matches!(app.active_pane, Pane::Projects);
    let tasks_active = matches!(app.active_pane, Pane::Tasks);

    let projects_block = Block::default()
        .title(" Projects ")
        .title_style(if projects_active {
            Theme::active_title()
        } else {
            Theme::title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if projects_active {
            Theme::active_border()
        } else {
            Theme::inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(Theme::base_bg());

    let tasks_title = format!(" {} ", app.selected_project_name());
    let tasks_block = Block::default()
        .title(tasks_title)
        .title_style(if tasks_active {
            Theme::active_title()
        } else {
            Theme::title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if tasks_active {
            Theme::active_border()
        } else {
            Theme::inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(Theme::base_bg());

    let projects_inner = projects_block.inner(projects_area);
    let tasks_inner = tasks_block.inner(tasks_area);

    frame.render_widget(projects_block, projects_area);
    frame.render_widget(tasks_block, tasks_area);

    views::projects::render(frame, app, projects_inner, projects_active);
    views::tasks::render(frame, app, tasks_inner, tasks_active);
    keyhints::render(frame, app, hints_area);
}

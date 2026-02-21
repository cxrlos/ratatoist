use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::{App, Pane};

use super::keyhints;
use super::statusbar;
use super::theme::Theme;
use super::views;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let [main_area, status_area, hints_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
            .areas(main_area);

    let projects_active = matches!(app.active_pane, Pane::Projects);
    let settings_active = matches!(app.active_pane, Pane::Settings);

    if app.show_settings {
        let [projects_area, settings_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(5)]).areas(left_area);

        render_projects_block(frame, app, projects_area, projects_active);
        views::settings::render(frame, app, settings_area, settings_active);
    } else {
        render_projects_block(frame, app, left_area, projects_active);
    }

    if matches!(app.active_pane, Pane::Detail) {
        if let Some(task) = app.selected_task() {
            let task = task.clone();
            let comments = app.comments.clone();
            views::detail::render(
                frame,
                &task,
                &comments,
                &app.user_names,
                app.current_user_id.as_deref(),
                right_area,
                app.detail_scroll,
                app.detail_field,
            );
        }
    } else {
        let [overview_area, task_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(right_area);

        views::overview::render(frame, app, overview_area);

        let tasks_active = matches!(app.active_pane, Pane::Tasks);
        render_tasks_block(frame, app, task_area, tasks_active);
    }

    statusbar::render(frame, app, status_area);
    keyhints::render(frame, app, hints_area);
}

fn render_projects_block(frame: &mut Frame, app: &App, area: Rect, active: bool) {
    let block = Block::default()
        .title(" Projects ")
        .title_style(if active {
            Theme::active_title()
        } else {
            Theme::title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if active {
            Theme::active_border()
        } else {
            Theme::inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(Theme::base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);
    views::projects::render(frame, app, inner, active);
}

fn render_tasks_block(frame: &mut Frame, app: &App, area: Rect, active: bool) {
    let tasks_title = format!(" {} ", app.selected_project_name());
    let block = Block::default()
        .title(tasks_title)
        .title_style(if active {
            Theme::active_title()
        } else {
            Theme::title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if active {
            Theme::active_border()
        } else {
            Theme::inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(Theme::base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);
    views::tasks::render(frame, app, inner, active);
}

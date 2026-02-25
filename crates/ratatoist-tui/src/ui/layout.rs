use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::{App, DOCK_ITEMS, DockItem, Pane, SortMode, TaskFilter};

const STATS_HEIGHT: u16 = 4;
use crate::ui::theme::Theme;

use super::keyhints;
use super::statusbar;
use super::views;

pub fn render(frame: &mut Frame, app: &App) {
    let theme = app.theme();
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
    let stats_active = matches!(app.active_pane, Pane::StatsDock);
    let settings_active = matches!(app.active_pane, Pane::Settings);

    if app.show_settings {
        let [projects_area, stats_area, settings_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(STATS_HEIGHT),
            Constraint::Length(5),
        ])
        .areas(left_area);

        render_projects_block(frame, app, projects_area, projects_active);
        render_stats_block(frame, app, stats_area, stats_active);
        views::settings::render(frame, app, settings_area, settings_active);
    } else {
        let [projects_area, stats_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(STATS_HEIGHT),
        ])
        .areas(left_area);

        render_projects_block(frame, app, projects_area, projects_active);
        render_stats_block(frame, app, stats_area, stats_active);
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
                theme,
            );
        }
    } else {
        let tasks_active = matches!(app.active_pane, Pane::Tasks);
        render_tasks_block(frame, app, right_area, tasks_active);
    }

    statusbar::render(frame, app, status_area);
    keyhints::render(frame, app, hints_area);
}

fn render_projects_block(frame: &mut Frame, app: &App, area: Rect, active: bool) {
    let theme = app.theme();

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

    views::projects::render(frame, app, inner, active);
}

fn dock_filter_color(filter: DockItem, theme: &Theme) -> Color {
    match filter {
        DockItem::DueOverdue => theme.red,
        DockItem::DueToday => theme.yellow,
        DockItem::DueWeek => theme.cyan,
        DockItem::Priority(4) => theme.red,
        DockItem::Priority(3) => theme.yellow,
        DockItem::Priority(2) => theme.maroon,
        DockItem::Priority(_) => theme.subtle,
    }
}

fn render_tasks_block(frame: &mut Frame, app: &App, area: Rect, active: bool) {
    let theme = app.theme();

    let (title, title_style, border_style) = if let Some(filter) = app.dock_filter {
        let color = dock_filter_color(filter, theme);
        let s = Style::default().fg(color);
        (format!(" ◈ {} ", filter.hint()), s, s)
    } else {
        (
            format!(" {} ", app.selected_project_name()),
            if active {
                theme.active_title()
            } else {
                theme.title()
            },
            if active {
                theme.active_border()
            } else {
                theme.inactive_border()
            },
        )
    };

    let block = Block::default()
        .title(title)
        .title_style(title_style)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style)
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.dock_filter.is_some() {
        let [filter_area, banner_area, tasks_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);
        render_filter_row(frame, app, filter_area);
        render_filter_banner(frame, app, banner_area);
        views::tasks::render(frame, app, tasks_area, active);
    } else {
        let [filter_area, tasks_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        render_filter_row(frame, app, filter_area);
        views::tasks::render(frame, app, tasks_area, active);
    }
}

fn render_filter_banner(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let Some(filter) = app.dock_filter else {
        return;
    };
    let color = dock_filter_color(filter, theme);
    let banner = Style::default()
        .fg(theme.base)
        .bg(color)
        .add_modifier(Modifier::BOLD);
    let hint = Style::default().fg(color).bg(theme.surface);
    let line = Line::from(vec![
        Span::styled(format!(" ◈ {}  ", filter.hint()), banner),
        Span::styled("Esc: clear", hint),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme.surface)),
        area,
    );
}

fn render_filter_row(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();

    let style_for = |f: TaskFilter| {
        if app.task_filter == f {
            theme.active_title()
        } else {
            theme.muted_text()
        }
    };

    let mut spans = vec![
        Span::styled("Active", style_for(TaskFilter::Active)),
        Span::styled("  ", theme.muted_text()),
        Span::styled("Done", style_for(TaskFilter::Done)),
        Span::styled("  ", theme.muted_text()),
        Span::styled("Both", style_for(TaskFilter::Both)),
    ];

    if app.sort_mode != SortMode::Default {
        spans.push(Span::styled(
            format!("   ⟳ {}", app.sort_mode.label()),
            theme.due_upcoming(),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_stats_block(frame: &mut Frame, app: &App, area: Rect, active: bool) {
    let theme = app.theme();
    let stats = app.overview_stats();

    let title = if let Some(idx) = app.dock_focus {
        let hint = DOCK_ITEMS[idx].hint();
        format!(" Stats → {hint} ")
    } else {
        " Stats ".to_string()
    };

    let block = Block::default()
        .title(title)
        .title_style(if active {
            theme.active_title()
        } else {
            theme.muted_text()
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

    let [due_area, prio_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

    let dock_style = |item: DockItem, idx: usize, base: ratatui::style::Style| {
        if app.dock_focus == Some(idx) {
            theme.dock_focused_item()
        } else if app.dock_filter == Some(item) {
            theme.active_title()
        } else {
            base
        }
    };

    let overdue_base = if stats.overdue > 0 {
        theme.due_overdue()
    } else {
        theme.muted_text()
    };

    let due_line = Line::from(vec![
        Span::styled("Due  ", theme.muted_text()),
        Span::styled(
            format!("▲ {}  ", stats.overdue),
            dock_style(DockItem::DueOverdue, 0, overdue_base),
        ),
        Span::styled(
            format!("◆ {}  ", stats.due_today),
            dock_style(DockItem::DueToday, 1, theme.due_today()),
        ),
        Span::styled(
            format!("◇ {}", stats.due_week),
            dock_style(DockItem::DueWeek, 2, theme.due_upcoming()),
        ),
    ]);

    let p = &stats.by_priority;
    let prio_line = Line::from(vec![
        Span::styled("P    ", theme.muted_text()),
        Span::styled(
            format!("● {}  ", p[4]),
            dock_style(DockItem::Priority(4), 3, theme.priority_style(4)),
        ),
        Span::styled(
            format!("● {}  ", p[3]),
            dock_style(DockItem::Priority(3), 4, theme.priority_style(3)),
        ),
        Span::styled(
            format!("● {}  ", p[2]),
            dock_style(DockItem::Priority(2), 5, theme.priority_style(2)),
        ),
        Span::styled(
            format!("─ {}", p[1]),
            dock_style(DockItem::Priority(1), 6, theme.muted_text()),
        ),
    ]);

    frame.render_widget(Paragraph::new(due_line), due_area);
    frame.render_widget(Paragraph::new(prio_line), prio_area);
}

//! The panes of the screens: the Projects tree, and a task list with its
//! filter row.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::{DockItem, SortMode, State, TaskFilter};
use crate::todoist::model::Priority;
use crate::ui::theme::Theme;

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

fn dock_filter_color(filter: DockItem, theme: &Theme) -> Color {
    match filter {
        DockItem::DueOverdue => theme.due.overdue,
        DockItem::DueToday => theme.due.today,
        DockItem::DueWeek => theme.due.this_week,
        DockItem::Priority(priority) => theme.priority(Priority::from_api(priority)),
    }
}

/// A task list titled `title`, alone in `area`.
pub fn render_tasks_block(frame: &mut Frame, state: &State, area: Rect, title: &str) {
    let theme = state.theme();
    let active = true;

    let (title, title_style, border_style) = if let Some(filter) = state.dock_filter {
        let color = dock_filter_color(filter, theme);
        let s = Style::default().fg(color);
        (format!(" ◈ {} ", filter.hint()), s, s)
    } else {
        (
            format!(
                " {} ",
                text::fit(title, usize::from(area.width.saturating_sub(4)))
            ),
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

    if state.dock_filter.is_some() {
        let [filter_area, banner_area, tasks_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);
        render_filter_row(frame, state, filter_area);
        render_filter_banner(frame, state, banner_area);
        views::tasks::render(frame, state, tasks_area, active);
    } else {
        let [filter_area, tasks_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        render_filter_row(frame, state, filter_area);
        views::tasks::render(frame, state, tasks_area, active);
    }
}

fn render_filter_banner(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();
    let Some(filter) = state.dock_filter else {
        return;
    };
    let color = dock_filter_color(filter, theme);
    let banner = theme.selection.fg(color).add_modifier(Modifier::BOLD);
    let hint = theme.base_bg().fg(color);
    let line = Line::from(vec![
        Span::styled(format!(" ◈ {}  ", filter.hint()), banner),
        Span::styled("Esc: clear", hint),
    ]);
    frame.render_widget(
        Paragraph::new(text::fit_line(line, usize::from(area.width))).style(theme.surface_bg()),
        area,
    );
}

fn render_filter_row(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();

    let style_for = |f: TaskFilter| {
        if state.task_filter == f {
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

    if state.sort_mode != SortMode::Default {
        spans.push(Span::styled(
            format!("   ⟳ {}", state.sort_mode.label()),
            theme.due_upcoming(),
        ));
    }

    text::draw(frame, Line::from(spans), area);
}

use ratatoist_core::api::models::Task;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::{App, InputMode};
use crate::ui::dates;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let theme = app.theme();
    let visible = app.visible_tasks();

    if visible.is_empty() && app.dock_filter.is_none() {
        let hint = match app.input_mode {
            InputMode::Vim(_) => "press a to add a task",
            InputMode::Standard => "press Ctrl-a to add a task",
        };
        let lines = vec![
            ListItem::new(Line::default()),
            ListItem::new(Line::from(Span::styled(
                "No tasks in this project",
                theme.muted_text(),
            ))),
            ListItem::new(Line::from(vec![Span::styled(hint, theme.muted_text())])),
        ];
        frame.render_widget(List::new(lines), area);
        return;
    }

    if visible.is_empty() {
        let lines = vec![
            ListItem::new(Line::default()),
            ListItem::new(Line::from(Span::styled(
                "No matching tasks",
                theme.muted_text(),
            ))),
        ];
        frame.render_widget(List::new(lines), area);
        return;
    }

    let cross_project = app.dock_filter.is_some();

    let mut items: Vec<ListItem> = Vec::new();
    let mut visual_selected: Option<usize> = None;
    let mut current_project_id: Option<String> = None;
    let mut last_section_id: Option<String> = None;

    for (task_idx, task) in visible.iter().enumerate() {
        if cross_project && current_project_id.as_deref() != Some(&task.project_id) {
            let project_name = app
                .projects
                .iter()
                .find(|p| p.id == task.project_id)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" ── {project_name} ──"),
                theme.muted_text().add_modifier(Modifier::BOLD),
            ))));
            current_project_id = Some(task.project_id.clone());
            last_section_id = None;
        }

        if !cross_project && task.parent_id.is_none() && task.section_id != last_section_id {
            last_section_id = task.section_id.clone();
            if let Some(sid) = &task.section_id {
                let name = app
                    .sections
                    .iter()
                    .find(|s| &s.id == sid)
                    .map(|s| s.name.as_str())
                    .unwrap_or("Section");
                if !items.is_empty() {
                    items.push(ListItem::new(Line::default()));
                }
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {name}"),
                    theme.muted_text().add_modifier(Modifier::BOLD),
                ))));
            }
        }

        if task_idx == app.selected_task {
            visual_selected = Some(items.len());
        }
        items.push(build_task_item(task, app, theme, cross_project));
    }

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);
    let mut state = ListState::default().with_selected(visual_selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn build_task_item<'a>(
    task: &'a Task,
    app: &App,
    theme: &Theme,
    show_project: bool,
) -> ListItem<'a> {
    let mut spans = Vec::new();
    let depth = if show_project {
        0
    } else {
        app.task_depth(task)
    };
    let has_children = app.has_children(&task.id);
    let collapsed = app.is_collapsed(&task.id);

    if depth > 0 {
        spans.push(Span::styled("  ".repeat(depth), theme.muted_text()));
    }

    let tree_icon = if has_children {
        if collapsed { "▸ " } else { "▾ " }
    } else {
        match depth {
            0 => "○ ",
            1 => "◦ ",
            _ => "· ",
        }
    };
    spans.push(Span::styled(tree_icon, theme.muted_text()));

    if app.is_context_task(task) {
        spans.push(Span::styled(&task.content, theme.muted_text()));
        return ListItem::new(Line::from(spans));
    }

    if task.checked {
        spans.push(Span::styled("✓ ", theme.success()));
        spans.push(Span::styled(
            &task.content,
            theme.muted_text().add_modifier(Modifier::CROSSED_OUT),
        ));
    } else {
        spans.push(Span::styled(
            Theme::priority_dot(task.priority),
            theme.priority_style(task.priority),
        ));
        spans.push(Span::styled(&task.content, theme.normal_text()));
    }

    if !task.labels.is_empty() && !task.checked {
        for label_name in &task.labels {
            let color = app
                .labels
                .iter()
                .find(|l| &l.name == label_name)
                .map(|l| theme.color_for(&l.color))
                .unwrap_or(theme.purple);
            spans.push(Span::styled(
                format!("  {label_name}"),
                Style::default().fg(color),
            ));
        }
    }

    if let Some(count) = task.note_count
        && count > 0
        && !task.checked
    {
        spans.push(Span::styled(format!("  [{count}]"), theme.muted_text()));
    }

    if task.due.as_ref().is_some_and(|d| d.is_recurring) && !task.checked {
        spans.push(Span::styled("  ↻", theme.muted_text()));
    }

    if let Some(due) = &task.due
        && !task.checked
    {
        let formatted = dates::format_due(due, theme);
        spans.push(Span::styled(
            format!("  {}", formatted.text),
            formatted.style,
        ));
    }

    ListItem::new(Line::from(spans))
}

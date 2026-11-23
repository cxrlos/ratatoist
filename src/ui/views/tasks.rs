use crate::todoist::model::{Priority, Task};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding};

use crate::app::{ListFilter, Screen, State};
use crate::ui::dates::{self, DueBucket};
use crate::ui::icons::Icons;
use crate::ui::text;
use crate::ui::theme::Theme;

/// The task list in its bordered block titled `title`, filling `area`.
pub fn render_block(frame: &mut Frame, state: &State, area: Rect, title: &str) {
    let theme = state.theme();
    let block = Block::default()
        .title(format!(
            " {} ",
            text::fit(title, usize::from(area.width.saturating_sub(4)))
        ))
        .title_style(theme.active_title())
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render(frame, state, inner, true);
}

pub fn render(frame: &mut Frame, state: &State, area: Rect, is_active: bool) {
    render_with_icons(frame, state, area, is_active, state.icons());
}

/// The task list drawn with `icons` in place of the icon set `state` picks,
/// so glyph candidates can be compared in a real row.
pub fn render_with_icons(
    frame: &mut Frame,
    state: &State,
    area: Rect,
    is_active: bool,
    icons: &Icons,
) {
    let theme = state.theme();
    let visible = state.visible_tasks();
    let room = usize::from(area.width);
    let row = |line: Line<'static>| ListItem::new(text::fit_line(line, room));
    let today = state.screen == Screen::Today;

    if today && visible.is_empty() {
        let lines = vec![
            ListItem::new(Line::default()),
            row(Line::from(Span::styled(
                "All caught up for today",
                theme.muted_text(),
            ))),
        ];
        frame.render_widget(List::new(lines), area);
        return;
    }

    if visible.is_empty() && !today && state.list_filter == ListFilter::default() {
        let lines = vec![
            ListItem::new(Line::default()),
            row(Line::from(Span::styled(
                "No tasks in this project",
                theme.muted_text(),
            ))),
        ];
        frame.render_widget(List::new(lines), area);
        return;
    }

    if visible.is_empty() {
        let lines = vec![
            ListItem::new(Line::default()),
            row(Line::from(Span::styled(
                "No matching tasks",
                theme.muted_text(),
            ))),
        ];
        frame.render_widget(List::new(lines), area);
        return;
    }

    let cross_project = today;

    let mut items: Vec<ListItem> = Vec::new();
    let mut visual_selected: Option<usize> = None;
    let mut current_project_id: Option<String> = None;
    let mut last_section_id: Option<String> = None;

    let stats = if today {
        Some(state.overview_stats())
    } else {
        None
    };
    let mut overdue_header_shown = false;

    for (task_idx, task) in visible.iter().enumerate() {
        if today && !overdue_header_shown && state.due_bucket(task) == DueBucket::Overdue {
            let overdue_count = stats.as_ref().map(|s| s.overdue).unwrap_or(0);
            let fold = icons.fold(!state.overdue_section_collapsed);
            items.push(row(Line::from(vec![Span::styled(
                format!(" {fold} {} Overdue  ({overdue_count})", icons.overdue),
                theme.due_overdue().add_modifier(Modifier::BOLD),
            )])));
            overdue_header_shown = true;
        }

        if cross_project && current_project_id.as_deref() != Some(&task.project_id) {
            current_project_id = Some(task.project_id.clone());
        }

        if !cross_project && task.parent_id.is_none() && task.section_id != last_section_id {
            last_section_id = task.section_id.clone();
            if let Some(sid) = &task.section_id {
                let name = state
                    .sections
                    .iter()
                    .find(|s| &s.id == sid)
                    .map(|s| s.name.as_str())
                    .unwrap_or("Section");
                if !items.is_empty() {
                    items.push(ListItem::new(Line::default()));
                }
                items.push(row(Line::from(Span::styled(
                    format!("  {name}"),
                    theme.muted_text().add_modifier(Modifier::BOLD),
                ))));
            }
        }

        if task_idx == state.selected_task {
            visual_selected = Some(items.len());
        }
        let line = task_line(task, state, theme, icons, cross_project);
        items.push(ListItem::new(text::fit_line(line, room)));
    }

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.unfocused_selection()
    };

    let list = List::new(items).highlight_style(highlight_style);
    let mut list_state = ListState::default().with_selected(visual_selected);
    frame.render_stateful_widget(list, area, &mut list_state);
}

/// One task's row; `show_project` flattens it, as in lists across
/// projects.
pub(crate) fn task_line<'a>(
    task: &'a Task,
    state: &State,
    theme: &Theme,
    icons: &Icons,
    show_project: bool,
) -> Line<'a> {
    let mut spans = Vec::new();
    let depth = if show_project {
        0
    } else {
        state.task_depth(task)
    };
    let has_children = state.has_children(&task.id);
    let collapsed = state.is_collapsed(&task.id);

    if depth > 0 {
        spans.push(Span::styled("  ".repeat(depth), theme.muted_text()));
    }

    if state.rollback_reason(&task.id).is_some() {
        spans.push(Span::styled(
            format!("{} ", icons.failed),
            crate::ui::style::error(theme),
        ));
    } else {
        let lead = if has_children && !task.checked {
            icons.fold(!collapsed)
        } else {
            icons.check(task.checked)
        };
        spans.push(Span::styled(format!("{lead} "), theme.muted_text()));
    }

    if state.is_context_task(task) {
        spans.push(Span::styled(&task.content, theme.muted_text()));
        return Line::from(spans);
    }

    let priority = format!("{} ", icons.priority(Priority::from_api(task.priority)));
    if task.checked {
        spans.push(Span::styled(priority, theme.muted_text()));
        spans.push(Span::styled(
            &task.content,
            theme.muted_text().add_modifier(Modifier::CROSSED_OUT),
        ));
    } else {
        spans.push(Span::styled(priority, theme.priority_style(task.priority)));
        spans.push(Span::styled(&task.content, theme.normal_text()));
    }

    if !task.labels.is_empty() && !task.checked {
        for label_name in &task.labels {
            spans.push(Span::styled(format!("  {label_name}"), theme.label_tag()));
        }
    }

    if let Some(count) = task.note_count
        && count > 0
        && !task.checked
    {
        spans.push(Span::styled(
            format!("  {}{count}", icons.comments),
            theme.muted_text(),
        ));
    }

    if task.due.as_ref().is_some_and(|d| d.is_recurring) && !task.checked {
        spans.push(Span::styled(
            format!("  {}", icons.recurring),
            theme.muted_text(),
        ));
    }

    if let Some(due) = &task.due
        && !task.checked
    {
        let formatted = dates::format_due(due, state.today, theme);
        spans.push(Span::styled(
            format!("  {}", formatted.text),
            formatted.style,
        ));
    }

    Line::from(spans)
}

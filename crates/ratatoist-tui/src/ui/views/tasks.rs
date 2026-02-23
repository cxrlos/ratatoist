use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::{App, InputMode};
use crate::ui::dates;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let theme = app.theme();

    if app.tasks.is_empty() {
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

    let visible = app.visible_tasks();

    let items: Vec<ListItem> = visible
        .iter()
        .map(|task| {
            let mut spans = Vec::new();
            let depth = app.task_depth(task);
            let has_children = app.has_children(&task.id);
            let collapsed = app.is_collapsed(&task.id);

            if depth > 0 {
                spans.push(Span::styled("  ".repeat(depth), theme.muted_text()));
            }

            let tree_icon = if has_children {
                if collapsed { "▸ " } else { "▾ " }
            } else {
                match depth {
                    0 => "◇ ",
                    1 => "◦ ",
                    _ => "· ",
                }
            };
            spans.push(Span::styled(tree_icon, theme.muted_text()));

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
                let label_str = format!("  {}", task.labels.join(" "));
                spans.push(Span::styled(label_str, theme.label_tag()));
            }

            if let Some(count) = task.note_count
                && count > 0
                && !task.checked
            {
                spans.push(Span::styled(format!("  [{count}]"), theme.muted_text()));
            }

            if let Some(due) = &task.due
                && !task.checked
            {
                let formatted = dates::format_due(&due.date, theme);
                spans.push(Span::styled(
                    format!("  {}", formatted.text),
                    formatted.style,
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default().with_selected(Some(app.selected_task));
    frame.render_stateful_widget(list, area, &mut state);
}

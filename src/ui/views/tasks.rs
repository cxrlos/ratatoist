use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::App;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.tasks.is_empty() {
        let empty = List::new(vec![ListItem::new(Line::from(Span::styled(
            "No tasks",
            Theme::muted_text(),
        )))]);
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let mut spans = Vec::new();

            spans.push(Span::styled(
                Theme::priority_dot(task.priority),
                Theme::priority_style(task.priority),
            ));

            spans.push(Span::styled(&task.content, Theme::normal_text()));

            if let Some(due) = &task.due {
                spans.push(Span::styled(format!("  {}", due.date), Theme::muted_text()));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let highlight_style = if is_active {
        Theme::selected_item()
    } else {
        Theme::subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default().with_selected(Some(app.selected_task));
    frame.render_stateful_widget(list, area, &mut state);
}

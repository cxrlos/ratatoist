use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let theme = app.theme();

    if app.projects.is_empty() {
        let empty = List::new(vec![ListItem::new(Line::from(Span::styled(
            "No projects found",
            theme.muted_text(),
        )))]);
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|project| {
            let mut spans = Vec::new();

            if project.is_inbox() {
                spans.push(Span::styled(" ", theme.inbox_icon()));
            } else if project.is_favorite {
                spans.push(Span::styled("★ ", theme.favorite_icon()));
            } else {
                spans.push(Span::raw("  "));
            }

            spans.push(Span::styled(&project.name, theme.normal_text()));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default().with_selected(Some(app.selected_project));
    frame.render_stateful_widget(list, area, &mut state);
}

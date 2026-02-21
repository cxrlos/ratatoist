use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::App;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.projects.is_empty() {
        let empty = List::new(vec![ListItem::new(Line::from(Span::styled(
            "No projects found",
            Theme::muted_text(),
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
                spans.push(Span::styled(" ", Theme::inbox_icon()));
            } else if project.is_favorite {
                spans.push(Span::styled("★ ", Theme::favorite_icon()));
            } else {
                spans.push(Span::raw("  "));
            }

            spans.push(Span::styled(&project.name, Theme::normal_text()));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let highlight_style = if is_active {
        Theme::selected_item()
    } else {
        Theme::subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default().with_selected(Some(app.selected_project));
    frame.render_stateful_widget(list, area, &mut state);
}

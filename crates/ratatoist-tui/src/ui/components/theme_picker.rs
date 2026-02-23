use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding};

use crate::app::App;

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, app: &App) {
    let theme = app.theme();
    render_dim_overlay(frame, theme);

    let area = frame.area();
    let popup = centered_rect(45, 70, area);

    let block = Block::default()
        .title(" Theme ")
        .title_style(theme.active_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(1, 1, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let items: Vec<ListItem> = app
        .themes
        .iter()
        .map(|t| {
            let swatches: Vec<Span> = t
                .user_colors()
                .iter()
                .map(|&c| Span::styled("● ", Style::default().fg(c)))
                .collect();

            let mut spans = vec![Span::styled(
                format!("{:<20}", t.name),
                theme.normal_text().fg(theme.text),
            )];
            spans.extend(swatches);

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).highlight_style(theme.selected_item());
    let mut state = ListState::default().with_selected(Some(app.theme_selection));
    frame.render_stateful_widget(list, inner, &mut state);
}

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::App;

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, app: &App) {
    let theme = app.theme();
    render_dim_overlay(frame, theme);

    let area = frame.area();
    let popup_area = centered_rect(50, 20, area);

    let title = if app.comment_input {
        " Add Comment "
    } else if app.editing_field {
        match app.detail_field {
            0 => " Edit Content ",
            1 => " Edit Due Date ",
            2 => " Edit Description ",
            _ => " Edit ",
        }
    } else {
        " Add Task "
    };

    let block = Block::default()
        .title(title)
        .title_style(theme.mode_insert())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.due_upcoming())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut lines = Vec::new();

    if app.input_buffer.is_empty() {
        let placeholder = if app.editing_field && app.detail_field == 2 {
            "e.g. tomorrow, next monday, 2026-03-15, 28/02/2026..."
        } else if app.comment_input {
            "write a comment..."
        } else {
            "type task content (p1, @label, #project, due date parsed automatically)..."
        };
        lines.push(Line::from(Span::styled(
            placeholder,
            theme.muted_text().add_modifier(Modifier::ITALIC),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled(&app.input_buffer, theme.normal_text()),
            Span::styled("▎", theme.due_upcoming()),
        ]));
    }

    lines.push(Line::default());
    lines.push(
        Line::from(vec![
            Span::styled("Enter", theme.key_hint()),
            Span::styled(" submit  ", theme.muted_text()),
            Span::styled("Esc", theme.key_hint()),
            Span::styled(" cancel", theme.muted_text()),
        ])
        .alignment(Alignment::Center),
    );

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

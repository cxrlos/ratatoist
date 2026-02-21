use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::AppError;
use crate::ui::theme::Theme;

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, error: &AppError) {
    render_dim_overlay(frame);

    let area = frame.area();
    let popup_area = centered_rect(55, 35, area);

    let title = format!(" {} ", error.title);

    let block = Block::default()
        .title(title)
        .title_style(Theme::error_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Theme::error_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(Theme::base_bg());

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        &error.message,
        Theme::normal_text(),
    )));

    if let Some(suggestion) = &error.suggestion {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("Hint: ", Theme::due_upcoming().add_modifier(Modifier::BOLD)),
            Span::styled(suggestion, Theme::due_upcoming()),
        ]));
    }

    lines.push(Line::default());

    let dismiss = if error.recoverable {
        "press any key to dismiss"
    } else {
        "press any key to exit"
    };
    lines.push(Line::from(Span::styled(dismiss, Theme::muted_text())).alignment(Alignment::Center));

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

    frame.render_widget(paragraph, popup_area);
}

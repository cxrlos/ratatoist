use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::AppError;
use crate::ui::text;
use crate::ui::theme::Theme;

use super::popup::centered_rect;

pub fn render(frame: &mut Frame, error: &AppError, theme: &Theme) {
    let area = frame.area();
    let popup_area = centered_rect(55, 35, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);

    let room = usize::from(popup_area.width.saturating_sub(4));
    let title = format!(" {} ", text::fit(&error.title, room));

    let block = Block::default()
        .title(title)
        .title_style(theme.error_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.error_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        &error.message,
        theme.normal_text(),
    )));

    if let Some(suggestion) = &error.suggestion {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("Hint: ", theme.due_upcoming().add_modifier(Modifier::BOLD)),
            Span::styled(suggestion, theme.due_upcoming()),
        ]));
    }

    lines.push(Line::default());

    let dismiss = if error.recoverable {
        "press any key to dismiss"
    } else {
        "press any key to exit"
    };
    lines.push(Line::from(Span::styled(dismiss, theme.muted_text())).alignment(Alignment::Center));

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

    frame.render_widget(paragraph, popup_area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::render;
    use crate::app::AppError;
    use crate::ui::theme::Theme;

    #[test]
    fn a_title_wider_than_the_popup_ends_in_an_ellipsis() {
        let error = AppError {
            title: "週末の買い物リストを同期できませんでした".into(),
            message: "message".into(),
            suggestion: None,
            recoverable: true,
        };
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal
            .draw(|frame| render(frame, &error, &Theme::built_ins()[0]))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let top = (0..12)
            .map(|y| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .find(|row| row.contains('╭'))
            .unwrap();
        assert!(top.contains('…'));
    }
}

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::ui::theme::Theme;

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, selected: u8, theme: &Theme) {
    render_dim_overlay(frame, theme);

    let area = frame.area();
    let popup = centered_rect(30, 20, area);

    let block = Block::default()
        .title(" Priority ")
        .title_style(theme.active_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let priorities = [
        (4, "1  Urgent"),
        (3, "2  High"),
        (2, "3  Medium"),
        (1, "4  Normal"),
    ];

    let mut lines = Vec::new();
    for (value, label) in priorities {
        let is_selected = value == selected;
        let marker = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            theme.selected_item()
        } else {
            theme.priority_style(value)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, theme.key_hint()),
            Span::styled(format!("● {label}"), style),
        ]));
    }

    lines.push(Line::default());
    lines.push(
        Line::from(Span::styled(
            "Enter select  Esc cancel",
            theme.muted_text(),
        ))
        .alignment(Alignment::Center),
    );

    frame.render_widget(Paragraph::new(lines), inner);
}

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::{InputKind, State};
use crate::ui::views::detail::DetailField;

use super::popup::centered_rect;

pub fn render(frame: &mut Frame, state: &State, target: InputKind) {
    let theme = state.theme();

    let area = frame.area();
    let popup_area = centered_rect(50, 20, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);

    let title = if target == InputKind::Comment {
        " Add Comment "
    } else if target == InputKind::Field {
        match state.detail_field {
            DetailField::Content => " Edit Content ",
            DetailField::Priority => " Edit Priority ",
            DetailField::Due => " Edit Due Date ",
            DetailField::Description => " Edit Description ",
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

    if state.input.is_empty() {
        let placeholder = if target == InputKind::Field && state.detail_field == DetailField::Due {
            "e.g. tomorrow, next monday, 2026-03-15, 28/02/2026..."
        } else if target == InputKind::Comment {
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
            Span::styled(state.input.before_cursor(), theme.normal_text()),
            Span::styled("▎", theme.due_upcoming()),
            Span::styled(state.input.after_cursor(), theme.normal_text()),
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

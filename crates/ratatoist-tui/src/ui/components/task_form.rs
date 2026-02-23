use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::{App, TaskForm};

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, app: &App, form: &TaskForm) {
    let theme = app.theme();
    render_dim_overlay(frame, theme);

    let area = frame.area();
    let popup = centered_rect(55, 45, area);

    let block = Block::default()
        .title(" New Task ")
        .title_style(theme.mode_insert())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.due_upcoming())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = Vec::new();

    let fields = [
        (
            "Content",
            if form.content.is_empty() {
                "(empty)".to_string()
            } else {
                form.content.clone()
            },
        ),
        ("Priority", format_priority(form.priority)),
        (
            "Due date",
            if form.due_string.is_empty() {
                "none".to_string()
            } else {
                form.due_string.clone()
            },
        ),
        (
            "Project",
            app.projects
                .get(form.project_idx)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Inbox".to_string()),
        ),
    ];

    for (idx, (label, value)) in fields.iter().enumerate() {
        let active = idx == form.active_field;
        let label_style = if active {
            theme.active_title()
        } else {
            theme.muted_text()
        };
        let value_style = if active && !form.editing {
            theme.normal_text().add_modifier(Modifier::UNDERLINED)
        } else if idx == 1 {
            theme.priority_style(form.priority)
        } else {
            theme.normal_text()
        };

        let cursor = if active && !form.editing { " <" } else { "" };

        lines.push(Line::from(vec![
            Span::styled(format!("{label:<10}"), label_style),
            Span::styled(value, value_style),
            Span::styled(cursor, theme.key_hint()),
        ]));
    }

    if form.editing {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(&app.input_buffer, theme.normal_text()),
            Span::styled("_", theme.due_upcoming()),
        ]));
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Parses: p1-p4, today, tomorrow, next monday",
        theme.muted_text().add_modifier(Modifier::ITALIC),
    )));
    lines.push(Line::from(Span::styled(
        "Dates: YYYY-MM-DD, DD/MM/YYYY, DD-MM-YYYY",
        theme.muted_text().add_modifier(Modifier::ITALIC),
    )));
    lines.push(Line::default());

    let submit_hint = if form.editing {
        "Enter save field  Esc back to form"
    } else {
        "j/k navigate  Enter/i edit  Tab submit  Esc cancel"
    };
    lines.push(
        Line::from(Span::styled(submit_hint, theme.muted_text())).alignment(Alignment::Center),
    );

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn format_priority(p: u8) -> String {
    match p {
        4 => "P1 urgent".to_string(),
        3 => "P2 high".to_string(),
        2 => "P3 medium".to_string(),
        _ => "P4 normal".to_string(),
    }
}

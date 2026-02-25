use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};

use crate::ui::theme::Theme;

pub fn render(
    frame: &mut Frame,
    input: &str,
    error: Option<&str>,
    validating: bool,
    theme: &Theme,
) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme.base_bg()), area);

    let logo_lines: Vec<&str> = super::LOGO
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let logo_height = logo_lines.len() as u16;

    let [_, logo_area, _, form_area, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(logo_height),
        Constraint::Length(2),
        Constraint::Length(10),
        Constraint::Min(1),
    ])
    .areas(area);

    render_logo(frame, &logo_lines, theme, logo_area);

    let form_width = 64u16.min(area.width.saturating_sub(4));
    let h_pad = area.width.saturating_sub(form_width) / 2;

    let [_, center_area, _] = Layout::horizontal([
        Constraint::Length(h_pad),
        Constraint::Length(form_width),
        Constraint::Min(0),
    ])
    .areas(form_area);

    render_token_form(frame, input, error, validating, theme, center_area);
}

pub fn render_alias(
    frame: &mut Frame,
    selected_idx: usize,
    custom_input: &str,
    is_typing: bool,
    rc_path: &str,
    status: Option<&str>,
    theme: &Theme,
) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme.base_bg()), area);

    let logo_lines: Vec<&str> = super::LOGO
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let logo_height = logo_lines.len() as u16;

    let [_, logo_area, _, form_area, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(logo_height),
        Constraint::Length(2),
        Constraint::Length(12),
        Constraint::Min(1),
    ])
    .areas(area);

    render_logo(frame, &logo_lines, theme, logo_area);

    let form_width = 64u16.min(area.width.saturating_sub(4));
    let h_pad = area.width.saturating_sub(form_width) / 2;

    let [_, center_area, _] = Layout::horizontal([
        Constraint::Length(h_pad),
        Constraint::Length(form_width),
        Constraint::Min(0),
    ])
    .areas(form_area);

    render_alias_form(
        frame,
        selected_idx,
        custom_input,
        is_typing,
        rc_path,
        status,
        theme,
        center_area,
    );
}

fn render_logo(frame: &mut Frame, logo_lines: &[&str], theme: &Theme, area: Rect) {
    let max_width = logo_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);

    let logo_text: Vec<Line> = logo_lines
        .iter()
        .map(|l| {
            let padded = format!("{:width$}", l, width = max_width);
            Line::from(Span::styled(padded, theme.subtle_text()))
        })
        .collect();

    frame.render_widget(Paragraph::new(logo_text).alignment(Alignment::Center), area);
}

fn render_token_form(
    frame: &mut Frame,
    input: &str,
    error: Option<&str>,
    validating: bool,
    theme: &Theme,
    area: Rect,
) {
    let block = Block::default()
        .title(" --new-user session ")
        .title_style(theme.active_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [label_area, input_area, hint_area, _, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Todoist API token",
            theme.muted_text().add_modifier(Modifier::BOLD),
        ))),
        label_area,
    );

    let input_line = if input.is_empty() {
        Line::from(Span::styled(
            "paste token here…",
            theme.muted_text().add_modifier(Modifier::ITALIC),
        ))
    } else {
        Line::from(vec![
            Span::styled(input, theme.normal_text()),
            Span::styled("▎", theme.active_border()),
        ])
    };
    frame.render_widget(Paragraph::new(input_line), input_area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", theme.key_hint()),
            Span::styled("  confirm   ", theme.muted_text()),
            Span::styled("Esc", theme.key_hint()),
            Span::styled("  quit   ", theme.muted_text()),
            Span::styled("todoist.com/app/settings/integrations", theme.muted_text()),
        ])),
        hint_area,
    );

    let status_line = if validating {
        Line::from(Span::styled("validating…", theme.muted_text()))
    } else if let Some(msg) = error {
        Line::from(Span::styled(msg, theme.due_overdue()))
    } else {
        Line::default()
    };
    frame.render_widget(Paragraph::new(status_line), status_area);
}

#[allow(clippy::too_many_arguments)]
fn render_alias_form(
    frame: &mut Frame,
    selected_idx: usize,
    custom_input: &str,
    is_typing: bool,
    rc_path: &str,
    status: Option<&str>,
    theme: &Theme,
    area: Rect,
) {
    let block = Block::default()
        .title(" shell alias ")
        .title_style(theme.active_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [opt0, opt1, opt2, _, hint_area, _, rc_area, _, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let options: [(&str, &str); 3] = [
        ("rat", "alias rat='ratatoist'"),
        ("custom", "type your own"),
        ("none", "skip"),
    ];

    for (i, (label, desc)) in options.iter().enumerate() {
        let area = [opt0, opt1, opt2][i];
        let is_sel = i == selected_idx;

        let cursor = if is_sel { "▶ " } else { "  " };
        let label_style = if is_sel {
            theme.active_title()
        } else {
            theme.muted_text()
        };

        let right_part: Line = if is_sel && i == 1 && is_typing {
            Line::from(vec![
                Span::styled(cursor, theme.active_border()),
                Span::styled(*label, label_style),
                Span::styled("  ", theme.muted_text()),
                Span::styled(custom_input, theme.normal_text()),
                Span::styled("▎", theme.active_border()),
            ])
        } else {
            Line::from(vec![
                Span::styled(cursor, theme.active_border()),
                Span::styled(*label, label_style),
                Span::styled(format!("  {desc}"), theme.muted_text()),
            ])
        };
        frame.render_widget(Paragraph::new(right_part), area);
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", theme.key_hint()),
            Span::styled("  choose   ", theme.muted_text()),
            Span::styled("Enter", theme.key_hint()),
            Span::styled("  confirm   ", theme.muted_text()),
            Span::styled("Esc", theme.key_hint()),
            Span::styled("  skip", theme.muted_text()),
        ])),
        hint_area,
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("→  {rc_path}"),
            theme.muted_text(),
        ))),
        rc_area,
    );

    let status_line = match status {
        Some(msg) if msg.starts_with("added") => Line::from(Span::styled(msg, theme.success())),
        Some(msg) => Line::from(Span::styled(msg, theme.due_overdue())),
        None => Line::default(),
    };
    frame.render_widget(Paragraph::new(status_line), status_area);
}

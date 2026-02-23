use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use std::collections::HashMap;

use ratatoist_core::api::models::{Comment, Task};

use crate::app::UserRecord;

use crate::ui::dates;
use crate::ui::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    task: &Task,
    comments: &[Comment],
    user_names: &HashMap<String, UserRecord>,
    current_user_id: Option<&str>,
    area: Rect,
    scroll: u16,
    selected_field: usize,
    theme: &Theme,
) {
    let block = Block::default()
        .title(" Task Detail ")
        .title_style(theme.active_title())
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    let content_style = if selected_field == 0 {
        theme.active_title().add_modifier(Modifier::UNDERLINED)
    } else {
        theme.active_title()
    };
    lines.push(Line::from(vec![
        Span::styled(&task.content, content_style),
        field_hint(selected_field == 0, theme),
    ]));
    lines.push(Line::default());

    let priority_label = match task.priority {
        4 => "Priority 1 (urgent)",
        3 => "Priority 2 (high)",
        2 => "Priority 3 (medium)",
        _ => "Priority 4 (normal)",
    };
    let priority_active = selected_field == 1;
    lines.push(Line::from(vec![
        Span::styled(
            "Priority  ",
            if priority_active {
                theme.active_title()
            } else {
                theme.muted_text()
            },
        ),
        Span::styled(
            format!("● {priority_label}"),
            theme.priority_style(task.priority),
        ),
        field_hint(priority_active, theme),
    ]));

    let due_style = if selected_field == 2 {
        theme.due_upcoming().add_modifier(Modifier::UNDERLINED)
    } else {
        theme.muted_text()
    };
    if let Some(due) = &task.due {
        let formatted = dates::format_due(&due.date, theme);
        let due_display = format!("{}  ({})", formatted.text, due.date);
        lines.push(Line::from(vec![
            Span::styled("Due       ", due_style),
            Span::styled(due_display, formatted.style),
            field_hint(selected_field == 2, theme),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Due       ", due_style),
            Span::styled("not set", theme.muted_text()),
            field_hint(selected_field == 2, theme),
        ]));
    }

    if task.checked {
        lines.push(Line::from(vec![
            Span::styled("Status    ", theme.muted_text()),
            Span::styled("✓ completed", theme.success()),
        ]));
    }

    if !task.labels.is_empty() {
        let labels = task.labels.join("  ");
        lines.push(Line::from(vec![
            Span::styled("Labels    ", theme.muted_text()),
            Span::styled(labels, theme.label_tag()),
        ]));
    }

    let desc_style = if selected_field == 3 {
        theme.normal_text().add_modifier(Modifier::UNDERLINED)
    } else {
        theme.normal_text()
    };
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Description", theme.subtle_text()),
        field_hint(selected_field == 3, theme),
    ]));
    if task.description.is_empty() {
        lines.push(Line::from(Span::styled("(empty)", theme.muted_text())));
    } else {
        for desc_line in task.description.lines() {
            lines.push(Line::from(Span::styled(desc_line.to_string(), desc_style)));
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "─── Comments ───",
        theme.subtle_text(),
    )));
    lines.push(Line::default());

    if comments.is_empty() {
        lines.push(Line::from(Span::styled(
            "no comments yet",
            theme.muted_text(),
        )));
    } else {
        let user_colors = theme.user_colors();
        let mut seen_users: Vec<String> = Vec::new();
        let mut prev_user: Option<String> = None;

        for comment in comments {
            let user_id = comment
                .posted_by_uid
                .as_deref()
                .unwrap_or("you")
                .to_string();

            if !seen_users.contains(&user_id) {
                seen_users.push(user_id.clone());
            }
            let color_idx =
                seen_users.iter().position(|u| u == &user_id).unwrap_or(0) % user_colors.len();
            let user_color = user_colors[color_idx];

            let same_user = prev_user.as_deref() == Some(&user_id);
            let timestamp = comment
                .posted_at
                .as_deref()
                .map(format_comment_time)
                .unwrap_or_default();

            if !same_user {
                if prev_user.is_some() {
                    lines.push(Line::default());
                }
                let is_me = current_user_id == Some(user_id.as_str());
                let resolved = user_names
                    .get(&user_id)
                    .map(|r| r.display.as_str())
                    .unwrap_or_else(|| &user_id[..user_id.len().min(8)]);
                let display_name = if is_me {
                    format!("{resolved} (you)")
                } else {
                    resolved.to_string()
                };
                let name_style = if is_me {
                    Style::default()
                        .fg(user_color)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC)
                } else {
                    Style::default().fg(user_color).add_modifier(Modifier::BOLD)
                };
                lines.push(Line::from(Span::styled(display_name, name_style)));
            }

            let has_attachment = comment.attachment.is_some();

            if !comment.content.is_empty() {
                for content_line in comment.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(user_color)),
                        Span::styled(content_line.to_string(), theme.normal_text()),
                    ]));
                }
            }

            if let Some(attachment) = &comment.attachment {
                let file_name = attachment.get("file_name").and_then(|v| v.as_str());
                let file_type = attachment.get("file_type").and_then(|v| v.as_str());
                let file_url = attachment.get("file_url").and_then(|v| v.as_str());
                let resource_type = attachment
                    .get("resource_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("file");

                let display = if let Some(name) = file_name {
                    let hint = file_type.map(|t| format!(" ({t})")).unwrap_or_default();
                    format!("[+] {name}{hint}")
                } else if let Some(url) = file_url {
                    let short = if url.len() > 50 { &url[..50] } else { url };
                    format!("[+] {short}")
                } else {
                    format!("[+] {resource_type} attachment")
                };

                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(user_color)),
                    Span::styled(
                        display,
                        theme.due_upcoming().add_modifier(Modifier::UNDERLINED),
                    ),
                ]));
            }

            if comment.content.is_empty() && !has_attachment {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(user_color)),
                    Span::styled("(empty)", theme.muted_text()),
                ]));
            }

            if let Some(last_line) = lines.last_mut() {
                last_line.spans.push(Span::styled(
                    format!("  {timestamp}"),
                    theme.muted_text().add_modifier(Modifier::ITALIC),
                ));
            }

            prev_user = Some(user_id);
        }
        lines.push(Line::default());
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("i", theme.key_hint()),
        Span::styled(" edit  ", theme.muted_text()),
        Span::styled("c", theme.key_hint()),
        Span::styled(" comment  ", theme.muted_text()),
        Span::styled("x", theme.key_hint()),
        Span::styled(" complete  ", theme.muted_text()),
        Span::styled("Esc", theme.key_hint()),
        Span::styled(" back", theme.muted_text()),
    ]));

    let paragraph = Paragraph::new(lines)
        .scroll((scroll, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);
}

fn field_hint(active: bool, theme: &Theme) -> Span<'static> {
    if active {
        Span::styled("  ◂", theme.key_hint())
    } else {
        Span::raw("")
    }
}

fn format_comment_time(timestamp: &str) -> String {
    if timestamp.len() < 16 {
        return timestamp.to_string();
    }
    let date_part = &timestamp[..10];
    let time_part = &timestamp[11..16];
    format!("{date_part} {time_part}")
}

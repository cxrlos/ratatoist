use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect, Size};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap};

use crate::todoist::model::{Comment, Priority, Task};

use crate::app::{State, UserRecord};
use crate::ui::icons::Icons;
use crate::ui::theme::Theme;
use crate::ui::{dates, text};

/// The task fields the detail's cursor moves over, top to bottom. The
/// cursor, the edit it starts and the edit popup's title all match on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailField {
    Content,
    Priority,
    Due,
    Description,
}

impl DetailField {
    /// The next field down, wrapping to the top.
    pub fn next(self) -> Self {
        match self {
            DetailField::Content => DetailField::Priority,
            DetailField::Priority => DetailField::Due,
            DetailField::Due => DetailField::Description,
            DetailField::Description => DetailField::Content,
        }
    }

    /// The next field up, wrapping to the bottom.
    pub fn prev(self) -> Self {
        match self {
            DetailField::Content => DetailField::Description,
            DetailField::Priority => DetailField::Content,
            DetailField::Due => DetailField::Priority,
            DetailField::Description => DetailField::Due,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    task: &Task,
    comments: &[Comment],
    user_names: &HashMap<String, UserRecord>,
    current_user_id: Option<&str>,
    area: Rect,
    scroll: u16,
    selected_field: DetailField,
    today: NaiveDate,
    theme: &Theme,
    icons: &Icons,
) {
    let block = block(theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = lines(
        task,
        comments,
        user_names,
        current_user_id,
        selected_field,
        today,
        theme,
        icons,
    );
    let scroll = scroll.min(overflow(&lines, inner));
    frame.render_widget(paragraph(lines).scroll((scroll, 0)), inner);
}

/// How far the selected task's detail scrolls before its last row reaches
/// the bottom, on a terminal of `screen`.
pub fn max_scroll(state: &State, screen: Size) -> u16 {
    let Some(task) = state.selected_task() else {
        return 0;
    };
    let theme = state.theme();
    let lines = lines(
        task,
        &state.comments,
        &state.user_names,
        state.current_user_id.as_deref(),
        state.detail_field,
        state.today,
        theme,
        state.icons(),
    );
    overflow(&lines, text_area(screen, theme))
}

/// The rows of text the detail shows at once on a terminal of `screen`.
pub fn page_height(state: &State, screen: Size) -> u16 {
    text_area(screen, state.theme()).height
}

/// Where the detail's text goes: inside the detail block, which takes the
/// right of the main area as `ui::layout` splits it.
fn text_area(screen: Size, theme: &Theme) -> Rect {
    let [main, _status, _hints] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(Rect::from((Position::ORIGIN, screen)));
    let [_left, right] =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).areas(main);
    block(theme).inner(right)
}

fn block(theme: &Theme) -> Block<'static> {
    Block::default()
        .title(" Task Detail ")
        .title_style(theme.active_title())
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg())
}

fn paragraph(lines: Vec<Line<'_>>) -> Paragraph<'_> {
    Paragraph::new(lines).wrap(Wrap { trim: false })
}

/// The rows `lines` take past the bottom of `area` once wrapped.
fn overflow(lines: &[Line], area: Rect) -> u16 {
    rendered_rows(lines, area.width).saturating_sub(area.height)
}

/// The rows `lines` fill when wrapped to `width`, found by drawing them off
/// screen: ratatui's own count sits behind an unstable feature. The last
/// line is the key hint, never blank, so the last row drawn is the end.
fn rendered_rows(lines: &[Line], width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    // A word-wrapped row and the one after it hold more than `width` cells
    // between them, so a line `w` cells wide takes under `2 * (w / width + 1)`
    // rows.
    let bound: usize = lines
        .iter()
        .map(|line| 2 * (line.width() / usize::from(width) + 1))
        .sum();
    let height = u16::try_from(bound).unwrap_or(u16::MAX);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    paragraph(lines.to_vec()).render(area, &mut buffer);
    (0..height)
        .rev()
        .find(|&y| (0..width).any(|x| buffer[(x, y)].symbol() != " "))
        .map_or(0, |y| y + 1)
}

#[allow(clippy::too_many_arguments)]
fn lines<'a>(
    task: &'a Task,
    comments: &[Comment],
    user_names: &HashMap<String, UserRecord>,
    current_user_id: Option<&str>,
    selected_field: DetailField,
    today: NaiveDate,
    theme: &Theme,
    icons: &Icons,
) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> = Vec::new();

    let content_style = if selected_field == DetailField::Content {
        theme.active_title().add_modifier(Modifier::UNDERLINED)
    } else {
        theme.active_title()
    };
    lines.push(Line::from(vec![
        Span::styled(&task.content, content_style),
        field_hint(selected_field == DetailField::Content, theme),
    ]));
    lines.push(Line::default());

    let priority_label = match task.priority {
        4 => "Priority 1 (urgent)",
        3 => "Priority 2 (high)",
        2 => "Priority 3 (medium)",
        _ => "Priority 4 (normal)",
    };
    let priority_active = selected_field == DetailField::Priority;
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
            format!(
                "{} {priority_label}",
                icons.priority(Priority::from_api(task.priority))
            ),
            theme.priority_style(task.priority),
        ),
        field_hint(priority_active, theme),
    ]));

    let due_active = selected_field == DetailField::Due;
    let due_style = if due_active {
        theme.due_upcoming().add_modifier(Modifier::UNDERLINED)
    } else {
        theme.muted_text()
    };
    if let Some(due) = &task.due {
        let formatted = dates::format_due(due, today, theme);
        let recurring_marker = if due.is_recurring {
            format!(" {}", icons.recurring)
        } else {
            String::new()
        };
        let due_display = format!("{}{}  ({})", formatted.text, recurring_marker, due.date);
        lines.push(Line::from(vec![
            Span::styled("Due       ", due_style),
            Span::styled(due_display, formatted.style),
            field_hint(due_active, theme),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Due       ", due_style),
            Span::styled("not set", theme.muted_text()),
            field_hint(due_active, theme),
        ]));
    }

    if task.checked {
        lines.push(Line::from(vec![
            Span::styled("Status    ", theme.muted_text()),
            Span::styled(format!("{} completed", icons.completed), theme.success()),
        ]));
    }

    if !task.labels.is_empty() {
        let labels = task.labels.join("  ");
        lines.push(Line::from(vec![
            Span::styled("Labels    ", theme.muted_text()),
            Span::styled(labels, theme.label_tag()),
        ]));
    }

    let description_active = selected_field == DetailField::Description;
    let desc_style = if description_active {
        theme.normal_text().add_modifier(Modifier::UNDERLINED)
    } else {
        theme.normal_text()
    };
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Description", theme.subtle_text()),
        field_hint(description_active, theme),
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
        let mut prev_user: Option<String> = None;

        for comment in comments {
            let user_id = comment
                .posted_by_uid
                .as_deref()
                .unwrap_or("you")
                .to_string();

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
                    .map(|r| r.display.clone())
                    .unwrap_or_else(|| text::fit(&user_id, 9));
                let display_name = if is_me {
                    format!("{resolved} (you)")
                } else {
                    resolved
                };
                let name_style = if is_me {
                    theme.active_title().add_modifier(Modifier::ITALIC)
                } else {
                    theme.active_title()
                };
                lines.push(Line::from(Span::styled(display_name, name_style)));
            }

            let has_attachment = comment.attachment.is_some();

            if !comment.content.is_empty() {
                for content_line in comment.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("│ ", theme.muted_text()),
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
                    format!("[+] {}", text::fit(url, 51))
                } else {
                    format!("[+] {resource_type} attachment")
                };

                lines.push(Line::from(vec![
                    Span::styled("│ ", theme.muted_text()),
                    Span::styled(
                        display,
                        theme.due_upcoming().add_modifier(Modifier::UNDERLINED),
                    ),
                ]));
            }

            if comment.content.is_empty() && !has_attachment {
                lines.push(Line::from(vec![
                    Span::styled("│ ", theme.muted_text()),
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

    lines
}

fn field_hint(active: bool, theme: &Theme) -> Span<'static> {
    if active {
        Span::styled("  ◂", theme.key_hint())
    } else {
        Span::raw("")
    }
}

/// A comment's time as `YYYY-MM-DD HH:MM`, from RFC 3339 or from an ISO
/// time without an offset (`2024-01-15T14:30:00`); anything else shows as
/// sent, cut with `…` past 17 cells.
fn format_comment_time(timestamp: &str) -> String {
    const SHOWN: &str = "%Y-%m-%d %H:%M";
    if let Ok(time) = DateTime::parse_from_rfc3339(timestamp) {
        return time.format(SHOWN).to_string();
    }
    if let Ok(time) = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S%.f") {
        return time.format(SHOWN).to_string();
    }
    text::fit(timestamp, 17)
}

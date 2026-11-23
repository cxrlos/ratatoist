//! A task's detail: its content, the field block, the description, its
//! subtasks, the comments and where it lives. Built as lines already
//! wrapped to the pane, so scrolling counts real rows. A prompt editing the
//! task draws in place of the field it edits.

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::list::{self, Focus};
use super::{dates, prompt, style, text};
use crate::app::{Prompt, PromptField, State};
use crate::todoist::model::{Comment, Priority, Task};

/// The width of the muted field names, as `Priority  `.
const FIELD_WIDTH: usize = 10;

/// What comment bodies are indented by under their author.
const BODY_INDENT: &str = "  ";

type Pieces = Vec<(String, Style)>;

/// The focused task's detail in its pane, scrolled as far as the user
/// scrolled it, with `editing`'s input in place of the field it edits,
/// scrolled into view. An empty pane when no task is focused.
pub fn render(
    state: &State,
    frame: &mut Frame,
    area: Rect,
    focus: Focus,
    editing: Option<&Prompt>,
) {
    let theme = state.theme();
    let Some(task) = state.selected_task() else {
        frame.render_widget(list::pane_block("Detail", area, focus, theme), area);
        return;
    };
    let block = list::pane_block(breadcrumb(state, task), area, focus, theme);
    let inner = block.inner(area);
    let (lines, input) = build(state, task, inner.width, editing);
    let height = usize::from(inner.height);
    let max = lines.len().saturating_sub(height);
    let mut scroll = usize::from(state.detail_scroll).min(max);
    if let Some(row) = input {
        // The input line and the hint under it.
        let end = row + 2;
        if end > scroll + height {
            scroll = end.saturating_sub(height);
        }
        scroll = scroll.min(row);
    }
    let shown: Vec<Line> = lines.into_iter().skip(scroll).collect();
    frame.render_widget(Paragraph::new(shown).block(block), area);
}

/// How far the focused task's detail scrolls before its last row reaches
/// the bottom, on a terminal of `screen`; 0 when the detail does not show.
pub fn max_scroll(state: &State, screen: Size) -> u16 {
    let Some(area) = super::detail_rows(state, screen) else {
        return 0;
    };
    let Some(task) = state.selected_task() else {
        return 0;
    };
    let rows = build(state, task, area.width, None).0.len();
    u16::try_from(rows.saturating_sub(usize::from(area.height))).unwrap_or(u16::MAX)
}

/// The rows of text the detail shows at once on a terminal of `screen`.
pub fn page_height(state: &State, screen: Size) -> u16 {
    super::detail_rows(state, screen).map_or(0, |area| area.height)
}

/// The detail pane's title: the task's project, then its section, as
/// `Personal › Admin`.
fn breadcrumb(state: &State, task: &Task) -> String {
    let project = state
        .projects
        .iter()
        .find(|project| project.id == task.project_id)
        .map_or(task.project_id.as_str(), |project| project.name.as_str());
    match section_name(state, task) {
        Some(section) => format!("{project} › {section}"),
        None => project.to_owned(),
    }
}

fn section_name<'a>(state: &'a State, task: &'a Task) -> Option<&'a str> {
    let id = task.section_id.as_deref()?;
    Some(
        state
            .sections
            .iter()
            .find(|section| section.id == id)
            .map_or(id, |section| section.name.as_str()),
    )
}

/// Every line of `task`'s detail wrapped to `width` columns, with
/// `editing`'s input in place of the field it edits; also the line the
/// input is on.
fn build(
    state: &State,
    task: &Task,
    width: u16,
    editing: Option<&Prompt>,
) -> (Vec<Line<'static>>, Option<usize>) {
    let theme = state.theme();
    let icons = state.icons();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let muted = style::muted(theme);
    let room = usize::from(width);
    let edits = |field| editing.filter(|prompt| prompt.field == field);
    let mut input = None;
    let mut lines = Vec::new();

    match edits(PromptField::Content) {
        Some(prompt) => {
            input = Some(lines.len());
            lines.push(prompt::input_line(state, prompt, room));
            lines.push(prompt::hint_line(state, prompt, room));
        }
        None => lines.extend(wrap(&[(task.content.clone(), bold)], width, &[], &[])),
    }
    if let Some(failure) = state.rollback_reason(&task.id) {
        let reason = format!(
            "{} Could not {}: {}",
            icons.failed, failure.action, failure.message
        );
        lines.extend(wrap(&[(reason, style::error(theme))], width, &[], &[]));
    }

    lines.push(Line::default());
    let check = if task.checked {
        (format!("{} Completed", icons.completed), muted)
    } else {
        (format!("{} Open", icons.open), Style::new())
    };
    lines.extend(field(state, "State", vec![check], width));
    let priority = Priority::from_api(task.priority);
    lines.extend(field(
        state,
        "Priority",
        vec![
            (
                icons.priority(priority).to_owned(),
                Style::new().fg(theme.priority(priority)),
            ),
            (format!(" {}", priority.label()), Style::new()),
        ],
        width,
    ));
    match edits(PromptField::DueDate) {
        Some(prompt) => {
            input = Some(lines.len());
            let value_room = room.saturating_sub(FIELD_WIDTH);
            let mut line = vec![Span::styled(format!("{:FIELD_WIDTH$}", "Due"), muted)];
            line.extend(prompt::input_line(state, prompt, value_room).spans);
            lines.push(Line::from(line));
            let mut hint = vec![Span::raw(" ".repeat(FIELD_WIDTH))];
            hint.extend(prompt::hint_line(state, prompt, value_room).spans);
            lines.push(Line::from(hint));
        }
        None => lines.extend(field(state, "Due", due_pieces(state, task), width)),
    }
    let labels = if task.labels.is_empty() {
        vec![("None".to_owned(), muted)]
    } else {
        plain(task.labels.join(", "))
    };
    lines.extend(field(state, "Labels", labels, width));

    lines.push(Line::default());
    match edits(PromptField::Description) {
        Some(prompt) => {
            input = Some(lines.len());
            lines.push(prompt::input_line(state, prompt, room));
            lines.push(prompt::hint_line(state, prompt, room));
        }
        None if task.description.trim().is_empty() => {
            lines.push(Line::styled("No description.", muted));
        }
        None => {
            for paragraph in task.description.lines() {
                lines.extend(wrap(&plain(paragraph), width, &[], &[]));
            }
        }
    }

    lines.extend(subtask_lines(state, task, width));

    let comments = comments(state, task);
    let loading = comments.is_empty()
        && !state.comments_by_task.contains_key(&task.id)
        && task.note_count.unwrap_or(0) > 0;
    lines.push(Line::default());
    let heading = if loading {
        "Comments".to_owned()
    } else {
        format!("Comments ({})", comments.len())
    };
    lines.push(Line::styled(heading, bold));
    if loading {
        lines.push(Line::styled("Loading…", muted));
    } else if comments.is_empty() {
        lines.push(Line::styled("No comments.", muted));
    }
    for (index, comment) in comments.iter().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        lines.extend(comment_lines(state, comment, width));
    }
    if let Some(prompt) = edits(PromptField::Comment) {
        lines.push(Line::default());
        lines.push(Line::styled("New comment", bold));
        input = Some(lines.len());
        lines.push(prompt::input_line(state, prompt, room));
        lines.push(prompt::hint_line(state, prompt, room));
    }

    lines.push(Line::default());
    lines.push(Line::styled("Details", bold));
    lines.extend(details(state, task, width));
    (lines, input)
}

/// When `task` is due, as `yesterday (Sat Nov 21)`, styled by how near it
/// is; a due string Todoist has not parsed yet, muted and ending in `…`.
fn due_pieces(state: &State, task: &Task) -> Pieces {
    let muted = style::muted(state.theme());
    if let Some(pending) = state.pending_due(&task.id) {
        return vec![(format!("{pending}{}", text::ELLIPSIS), muted)];
    }
    let Some(due) = &task.due else {
        return vec![("None".to_owned(), muted)];
    };
    let formatted = dates::format_due(due, state.today, state.theme());
    let spelled_out = State::due_day(task)
        .map(|day| (day.format("%b %-d").to_string(), day))
        .filter(|(short, _)| !formatted.text.contains(short.as_str()));
    let mut pieces = vec![(formatted.text, formatted.style)];
    if let Some((_, day)) = spelled_out {
        pieces.push((format!(" ({})", day.format("%a %b %-d")), muted));
    }
    if due.is_recurring {
        pieces.push((format!(" {}", state.icons().recurring), muted));
    }
    pieces
}

/// `Subtasks  1/3 done` and a row for each subtask, when `task` has any.
fn subtask_lines(state: &State, task: &Task, width: u16) -> Vec<Line<'static>> {
    let mut children: Vec<&Task> = state
        .tasks
        .iter()
        .filter(|t| !t.is_deleted && t.parent_id.as_deref() == Some(task.id.as_str()))
        .collect();
    if children.is_empty() {
        return Vec::new();
    }
    children.sort_by_key(|t| t.child_order);
    let muted = style::muted(state.theme());
    let done = children.iter().filter(|t| t.checked).count();
    let mut lines = vec![Line::default()];
    lines.extend(field(
        state,
        "Subtasks",
        plain(format!("{done}/{} done", children.len())),
        width,
    ));
    let indent = [(" ".repeat(FIELD_WIDTH), Style::new())];
    for child in children {
        let content_style = if child.checked { muted } else { Style::new() };
        let pieces = vec![
            (format!("{} ", state.icons().check(child.checked)), muted),
            (child.content.clone(), content_style),
        ];
        lines.extend(wrap(&pieces, width, &indent, &indent));
    }
    lines
}

/// The comments on `task` loaded so far, oldest first: those the last
/// sync or fetch brought, then any just written that are not among them.
fn comments<'a>(state: &'a State, task: &Task) -> Vec<&'a Comment> {
    let mut shown: Vec<&Comment> = state
        .comments_by_task
        .get(&task.id)
        .map(|list| list.iter().filter(|c| !c.is_deleted).collect())
        .unwrap_or_default();
    for comment in &state.comments {
        let on_task =
            comment.item_id.as_deref().or(comment.task_id.as_deref()) == Some(task.id.as_str());
        if on_task && !comment.is_deleted && shown.iter().all(|c| c.id != comment.id) {
            shown.push(comment);
        }
    }
    shown
}

/// One comment: `Name · 2 days ago`, then its text indented under it.
fn comment_lines(state: &State, comment: &Comment, width: u16) -> Vec<Line<'static>> {
    let muted = style::muted(state.theme());
    let uid = comment.posted_by_uid.as_deref();
    let author = uid.and_then(|uid| state.user_names.get(uid)).map_or_else(
        || uid.unwrap_or("Unknown").to_owned(),
        |user| user.name.clone(),
    );
    let mut header = vec![(author, Style::new().add_modifier(Modifier::BOLD))];
    if uid.is_some() && uid == state.current_user_id.as_deref() {
        header.push((" (you)".to_owned(), muted));
    }
    if let Some(posted) = comment.posted_at.as_deref().and_then(parse_time) {
        header.push((format!(" · {}", ago(posted, state.now)), muted));
    }
    let mut lines = wrap(&header, width, &[], &[]);
    let indent = [(BODY_INDENT.to_owned(), Style::new())];
    for paragraph in comment.content.lines() {
        lines.extend(wrap(&plain(paragraph), width, &indent, &indent));
    }
    if let Some(attachment) = &comment.attachment {
        let name = ["file_name", "file_url"]
            .iter()
            .find_map(|key| attachment.get(key).and_then(|value| value.as_str()))
            .unwrap_or("attachment");
        lines.extend(wrap(
            &[(format!("[+] {name}"), muted)],
            width,
            &indent,
            &indent,
        ));
    }
    lines
}

/// Where `task` lives and when it was added.
fn details(state: &State, task: &Task, width: u16) -> Vec<Line<'static>> {
    let muted = style::muted(state.theme());
    let none = || vec![("None".to_owned(), muted)];
    let project = state
        .projects
        .iter()
        .find(|project| project.id == task.project_id)
        .map_or_else(|| plain(task.project_id.clone()), |p| plain(p.name.clone()));
    let section = section_name(state, task).map_or_else(none, plain);
    let parent = task.parent_id.as_deref().map_or_else(none, |id| {
        state
            .tasks
            .iter()
            .find(|t| t.id == id)
            .map_or_else(|| plain(id), |t| plain(t.content.clone()))
    });
    let created = task.added_at.as_deref().and_then(parse_time).map_or_else(
        || vec![("Unknown".to_owned(), muted)],
        |at| {
            plain(format!(
                "{} ({})",
                at.format("%Y-%m-%d %H:%M UTC"),
                ago(at, state.now)
            ))
        },
    );
    let mut lines = Vec::new();
    for (name, value) in [
        ("Project", project),
        ("Section", section),
        ("Parent", parent),
        ("Created", created),
    ] {
        lines.extend(field(state, name, value, width));
    }
    lines
}

fn plain(text: impl Into<String>) -> Pieces {
    vec![(text.into(), Style::new())]
}

/// `name` in the muted field column, then `value`, wrapped under itself.
fn field(state: &State, name: &str, value: Pieces, width: u16) -> Vec<Line<'static>> {
    wrap(
        &value,
        width,
        &[(format!("{name:FIELD_WIDTH$}"), style::muted(state.theme()))],
        &[(" ".repeat(FIELD_WIDTH), Style::new())],
    )
}

/// A time Todoist sent: RFC 3339, or an ISO time without an offset, read
/// as UTC.
fn parse_time(at: &str) -> Option<DateTime<Utc>> {
    if let Ok(time) = DateTime::parse_from_rfc3339(at) {
        return Some(time.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|time| time.and_utc())
}

/// How long before `now` `then` was, in the largest whole unit, as
/// `2 days ago`.
fn ago(then: DateTime<Utc>, now: DateTime<Local>) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let seconds = (now.with_timezone(&Utc) - then).num_seconds();
    let (count, unit) = match seconds {
        _ if seconds < MINUTE => return "just now".to_owned(),
        _ if seconds < HOUR => (seconds / MINUTE, "minute"),
        _ if seconds < DAY => (seconds / HOUR, "hour"),
        _ if seconds < 14 * DAY => (seconds / DAY, "day"),
        _ if seconds < 60 * DAY => (seconds / (7 * DAY), "week"),
        _ if seconds < 365 * DAY => (seconds / (30 * DAY), "month"),
        _ => (seconds / (365 * DAY), "year"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

/// `pieces` wrapped at spaces into lines of at most `width` cells, the
/// first after `first` and the rest after `rest`. A word wider than a whole
/// line is cut with `…`.
fn wrap(
    pieces: &[(String, Style)],
    width: u16,
    first: &[(String, Style)],
    rest: &[(String, Style)],
) -> Vec<Line<'static>> {
    let room = usize::from(width);
    let prefix = |prefix: &[(String, Style)]| -> (Vec<Span<'static>>, usize) {
        let spans = prefix
            .iter()
            .map(|(text, style)| Span::styled(text.clone(), *style))
            .collect();
        let used = prefix.iter().map(|(text, _)| text::width(text)).sum();
        (spans, used)
    };
    let mut lines = Vec::new();
    let (mut line, mut used) = prefix(first);
    let mut has_words = false;
    let mut wrapped = false;
    let mut spaces: Vec<Span<'static>> = Vec::new();
    let mut spaces_width = 0;
    for (piece, style) in pieces {
        for token in tokens(piece) {
            if token.starts_with(' ') {
                spaces_width += text::width(token);
                spaces.push(Span::styled(token.to_owned(), *style));
                continue;
            }
            let word_width = text::width(token);
            if has_words && used + spaces_width + word_width > room {
                lines.push(Line::from(std::mem::take(&mut line)));
                (line, used) = prefix(rest);
                wrapped = true;
                has_words = false;
            }
            if has_words || !wrapped {
                used += spaces_width;
                line.append(&mut spaces);
            }
            spaces.clear();
            spaces_width = 0;
            let mut word = token;
            loop {
                let left = room.saturating_sub(used);
                let (head, tail) = text::split(word, left);
                if head.is_empty() && !has_words {
                    line.push(Span::styled(text::fit(word, left), *style));
                    has_words = true;
                    break;
                }
                if !head.is_empty() {
                    used += text::width(head);
                    line.push(Span::styled(head.to_owned(), *style));
                    has_words = true;
                }
                if tail.is_empty() {
                    break;
                }
                lines.push(Line::from(std::mem::take(&mut line)));
                (line, used) = prefix(rest);
                wrapped = true;
                has_words = false;
                word = tail;
            }
        }
    }
    lines.push(Line::from(line));
    lines
        .into_iter()
        .map(|line| text::fit_line(line, room))
        .collect()
}

/// `text` split into runs of spaces and runs of anything else.
fn tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut in_space = None;
    for (index, c) in text.char_indices() {
        let space = c == ' ';
        if in_space.is_some_and(|was| was != space) {
            tokens.push(&text[start..index]);
            start = index;
        }
        in_space = Some(space);
    }
    if start < text.len() {
        tokens.push(&text[start..]);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn wrapping_breaks_at_spaces_under_the_rest_prefix() {
        let lines = wrap(
            &plain("one two three"),
            12,
            &[("Due  ".to_owned(), Style::new())],
            &[("     ".to_owned(), Style::new())],
        );

        assert_eq!(texts(&lines), ["Due  one two", "     three"]);
    }

    #[test]
    fn a_word_wider_than_the_line_breaks_across_lines() {
        let lines = wrap(&plain("abcdefgh"), 5, &[], &[]);

        assert_eq!(texts(&lines), ["abcde", "fgh"]);
    }

    #[test]
    fn a_word_wider_than_the_line_starts_on_a_line_of_its_own() {
        let lines = wrap(&plain("to abcdefgh"), 5, &[], &[]);

        assert_eq!(texts(&lines), ["to", "abcde", "fgh"]);
    }

    #[test]
    fn a_broken_word_continues_under_the_rest_prefix() {
        let lines = wrap(
            &plain("abcdefgh"),
            7,
            &[("Due  ".to_owned(), Style::new())],
            &[("     ".to_owned(), Style::new())],
        );

        assert_eq!(texts(&lines), ["Due  ab", "     cd", "     ef", "     gh"]);
    }

    #[test]
    fn wide_characters_without_spaces_break_by_their_cells() {
        let lines = wrap(&plain("これはとても長い"), 7, &[], &[]);

        assert_eq!(texts(&lines), ["これは", "とても", "長い"]);
    }

    #[test]
    fn a_column_narrower_than_a_wide_character_still_ends() {
        let lines = wrap(&plain("チーズ"), 1, &[], &[]);

        assert_eq!(texts(&lines), ["…"]);
    }

    #[test]
    fn leading_spaces_stay_on_the_first_line() {
        let lines = wrap(&plain("  - item"), 20, &[], &[]);

        assert_eq!(texts(&lines), ["  - item"]);
    }

    #[test]
    fn wide_characters_wrap_by_their_cells() {
        let lines = wrap(&plain("チーズ ケーキ"), 7, &[], &[]);

        assert_eq!(texts(&lines), ["チーズ", "ケーキ"]);
    }

    #[test]
    fn ago_names_the_largest_whole_unit() {
        let now = DateTime::parse_from_rfc3339("2026-11-23T09:00:00Z")
            .unwrap()
            .with_timezone(&Local);
        let then = |at: &str| parse_time(at).unwrap();

        assert_eq!(ago(then("2026-11-23T08:59:30Z"), now), "just now");
        assert_eq!(ago(then("2026-11-23T08:00:00Z"), now), "1 hour ago");
        assert_eq!(ago(then("2026-11-21T09:00:00Z"), now), "2 days ago");
        assert_eq!(ago(then("2026-11-03T09:00:00Z"), now), "2 weeks ago");
        assert_eq!(ago(then("2026-08-25T09:00:00Z"), now), "3 months ago");
    }
}

//! The prompt: one line of text for a comment or a field of a task. It
//! draws as a popup over the body; a view that shows the field itself can
//! draw it in place instead with [`input_line`] and [`hint_line`].

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph};

use super::{confirm, dates, style, text};
use crate::app::{Action, Prompt, PromptField, State};

/// The narrowest a prompt popup is drawn, so short titles still leave room
/// to type.
const MIN_WIDTH: usize = 44;

/// What marks the line being typed, as in the picker.
const MARK: &str = "> ";

/// The prompt's title, as `Due date of Renew passport photos`.
pub fn title(prompt: &Prompt) -> String {
    format!("{} {}", prompt.field.title(), prompt.subject)
}

/// Draws `prompt` as a popup centred in `area`, sized to what it shows:
/// the due date in force for a due date, the line being typed, and what
/// the field takes or why the last Enter was refused. The keys that send
/// and close it sit in the bottom border.
pub fn render(state: &State, prompt: &Prompt, frame: &mut Frame, area: Rect) {
    let area = area.inner(Margin::new(1, 1));
    let bottom = Line::from(
        [
            confirm::keys(state, &[Action::Accept], prompt.field.verb()),
            confirm::keys(state, &[Action::Close], "close"),
        ]
        .concat(),
    );
    let title = format!(" {} ", title(prompt));
    let now = now(state, prompt).map(|now| format!("Now  {now}"));
    let widest = [
        text::width(&title),
        text::width(prompt.field.hint()),
        prompt.error.map_or(0, text::width),
        now.as_deref().map_or(0, text::width),
        bottom.width(),
        MIN_WIDTH,
    ]
    .into_iter()
    .max()
    .unwrap_or(MIN_WIDTH);
    let width = u16::try_from(widest + 4)
        .unwrap_or(u16::MAX)
        .min(area.width);
    let room = usize::from(width.saturating_sub(4).max(1));
    let mut lines = Vec::new();
    if let Some(now) = &now {
        lines.push(Line::styled(
            text::fit(now, room),
            style::muted(state.theme()),
        ));
    }
    lines.push(input_line(state, prompt, room));
    lines.push(hint_line(state, prompt, room));
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let popup = super::centered(area, width, height);
    let edge = usize::from(popup.width.saturating_sub(2));
    let block = super::popup_block(state)
        .title(text::fit(&title, edge))
        .title_bottom(text::fit_line(bottom, edge))
        .padding(Padding::horizontal(1));
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

/// The line being typed in `room` cells: `> `, the text and the cursor as
/// a reversed cell. When the text is wider than the room it keeps the
/// cursor in view, cutting the start with `…`. An empty line shows the
/// field's placeholder, muted.
pub fn input_line(state: &State, prompt: &Prompt, room: usize) -> Line<'static> {
    typed_line(state, prompt.field.placeholder(), room)
}

/// [`input_line`] for any field: [`State::input`] with its cursor, or
/// `placeholder` when it is empty.
pub fn typed_line(state: &State, placeholder: &str, room: usize) -> Line<'static> {
    let theme = state.theme();
    let room = room.saturating_sub(text::width(MARK));
    let mark = Span::styled(MARK, style::muted(theme));
    let cursor_style = theme.selection;
    if state.input.is_empty() {
        let placeholder = text::fit(placeholder, room);
        let mut chars = placeholder.chars();
        let first = chars.next().map_or_else(|| " ".to_owned(), String::from);
        return Line::from(vec![
            mark,
            Span::styled(first, style::muted(theme).patch(cursor_style)),
            Span::styled(chars.collect::<String>(), style::muted(theme)),
        ]);
    }
    let before = state.input.before_cursor();
    let after = state.input.after_cursor();
    let mut rest = after.chars();
    let under = rest.next().map_or_else(|| " ".to_owned(), String::from);
    let rest: String = rest.collect();
    let under_width = text::width(&under).max(1);
    let (left, right) = if text::width(before) + under_width <= room {
        let right_room = room - text::width(before) - under_width;
        (before.to_owned(), text::fit(&rest, right_room))
    } else {
        (
            text::fit_start(before, room.saturating_sub(under_width)),
            String::new(),
        )
    };
    Line::from(vec![
        mark,
        Span::styled(left, style::base(theme)),
        Span::styled(under, cursor_style),
        Span::styled(right, style::base(theme)),
    ])
}

/// A field's line while it is not being typed in: `> ` and `text` cut to
/// `room` cells, or `placeholder`, muted, when it is empty.
pub fn kept_line(state: &State, text: &str, placeholder: &str, room: usize) -> Line<'static> {
    let theme = state.theme();
    let room = room.saturating_sub(text::width(MARK));
    let mark = Span::styled(MARK, style::muted(theme));
    if text.is_empty() {
        return Line::from(vec![
            mark,
            Span::styled(text::fit(placeholder, room), style::muted(theme)),
        ]);
    }
    Line::from(vec![
        mark,
        Span::styled(text::fit(text, room), style::base(theme)),
    ])
}

/// What the field takes, muted, or why the last Enter was refused, in the
/// error style, cut to `room` cells.
pub fn hint_line(state: &State, prompt: &Prompt, room: usize) -> Line<'static> {
    match prompt.error {
        Some(reason) => Line::styled(text::fit(reason, room), style::error(state.theme())),
        None => Line::styled(
            text::fit(prompt.field.hint(), room),
            style::muted(state.theme()),
        ),
    }
}

/// The due date in force for a due date prompt, as the list shows it, or
/// `no date`; `None` for the other fields.
fn now(state: &State, prompt: &Prompt) -> Option<String> {
    if prompt.field != PromptField::DueDate {
        return None;
    }
    let task = state.tasks.iter().find(|task| task.id == prompt.task_id)?;
    Some(task.due.as_ref().map_or_else(
        || "no date".to_owned(),
        |due| dates::format_due(due, state.today, state.theme()).text,
    ))
}

//! The add-task form: the prompt's look, one row per field, in a popup
//! over the body sized to what it shows.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph};

use super::{confirm, prompt, style, text};
use crate::app::{Action, FormField, PromptField, State, TaskForm};
use crate::todoist::model::Priority;

/// The narrowest the form is drawn, so short titles still leave room to
/// type.
const MIN_WIDTH: usize = 44;

/// How wide the label column is, its gap included.
const LABEL_WIDTH: usize = 10;

/// What the due row shows, muted, when it is empty and not focused.
const NO_DUE: &str = "no date";

/// Draws `form` centred in `area`: its rows, the due date's hint under the
/// due row while it has focus, and why the last Enter did not add under the
/// content row. The keys that move, add or choose, and close sit in the
/// bottom border.
pub fn render(state: &State, form: &TaskForm, frame: &mut Frame, area: Rect) {
    let area = area.inner(Margin::new(1, 1));
    let verb = if form.editing() { "add" } else { "choose" };
    let bottom = Line::from(
        [
            confirm::keys(state, &[Action::NextField, Action::PreviousField], "field"),
            confirm::keys(state, &[Action::Accept], verb),
            confirm::keys(state, &[Action::Close], "close"),
        ]
        .concat(),
    );
    let title = format!(" Add task to {} ", project_name(state, form));
    let widest = [
        text::width(&title),
        LABEL_WIDTH + text::width(PromptField::DueDate.hint()),
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
    let lines = rows(state, form, room);
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

/// The form's lines in `room` cells: each field's row, with the lines that
/// belong under it.
fn rows(state: &State, form: &TaskForm, room: usize) -> Vec<Line<'static>> {
    let theme = state.theme();
    let value_room = room.saturating_sub(LABEL_WIDTH);
    let mut lines = Vec::new();
    for field in FormField::ALL {
        let focused = form.field == field;
        let label_style = if focused {
            style::accent(theme)
        } else {
            style::muted(theme)
        };
        let label = text::fit(field.label(), LABEL_WIDTH - 1);
        let gap = " ".repeat(LABEL_WIDTH - text::width(&label));
        let label = Span::styled(label + &gap, label_style);
        let value = value_line(state, form, field, value_room);
        let mut spans = vec![label];
        spans.extend(value.spans);
        lines.push(Line::from(spans));

        let under = match field {
            FormField::Content => form.error.map(|reason| (reason, style::error(theme))),
            FormField::Due if focused => Some((PromptField::DueDate.hint(), style::muted(theme))),
            FormField::Due | FormField::Priority | FormField::Project => None,
        };
        if let Some((words, words_style)) = under {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(LABEL_WIDTH)),
                Span::styled(text::fit(words, value_room), words_style),
            ]));
        }
    }
    lines
}

/// What `field`'s row shows after its label, in `room` cells.
fn value_line(state: &State, form: &TaskForm, field: FormField, room: usize) -> Line<'static> {
    let theme = state.theme();
    let focused = form.field == field;
    match field {
        FormField::Content if focused => {
            prompt::typed_line(state, PromptField::Content.placeholder(), room)
        }
        FormField::Due if focused => {
            prompt::typed_line(state, PromptField::DueDate.placeholder(), room)
        }
        FormField::Content => prompt::kept_line(
            state,
            &form.content,
            PromptField::Content.placeholder(),
            room,
        ),
        FormField::Due => prompt::kept_line(state, &form.due_string, NO_DUE, room),
        FormField::Priority => {
            let priority = Priority::from_api(form.priority);
            let glyph_style = if priority == Priority::P4 {
                style::muted(theme)
            } else {
                theme.priority_style(form.priority)
            };
            text::fit_line(
                Line::from(vec![
                    Span::styled(
                        format!("{} ", state.icons().priority(priority)),
                        glyph_style,
                    ),
                    Span::styled(priority.label(), style::base(theme)),
                ]),
                room,
            )
        }
        FormField::Project => {
            let project = state
                .store
                .projects
                .iter()
                .find(|p| p.id == form.project_id);
            let glyph = match project {
                Some(project) if project.is_inbox() => {
                    Span::styled(format!("{} ", state.icons().inbox), theme.inbox_icon())
                }
                Some(project) => Span::styled(
                    format!("{} ", state.icons().project),
                    theme.project_dot(&project.color),
                ),
                None => Span::styled(format!("{} ", state.icons().project), style::muted(theme)),
            };
            text::fit_line(
                Line::from(vec![
                    glyph,
                    Span::styled(project_name(state, form), style::base(theme)),
                ]),
                room,
            )
        }
    }
}

/// The name of the project the form adds to; `Inbox` when it is not
/// loaded.
fn project_name(state: &State, form: &TaskForm) -> String {
    state
        .store
        .projects
        .iter()
        .find(|p| p.id == form.project_id)
        .map_or_else(|| "Inbox".to_owned(), |p| p.name.clone())
}

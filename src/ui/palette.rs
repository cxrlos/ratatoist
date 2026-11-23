//! The palette overlay: a query line, a rule, then the matching actions
//! with their keys, tasks with their project, and projects.

use super::text;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::style;
use crate::app::{self, Palette, PaletteEntry, State};
use crate::keys;
use crate::todoist::model::{Project, Task};

const MAX_WIDTH: u16 = 76;
const MAX_HEIGHT: u16 = 16;

pub fn render(state: &State, palette: &Palette, frame: &mut Frame, area: Rect) {
    let theme = state.theme();
    let list = finder(state, " Palette ", ":", &palette.query, frame, area);
    let entries = app::palette_matches(state, &palette.query);
    if entries.is_empty() {
        let none = Line::styled(" Nothing matches.", style::muted(theme));
        frame.render_widget(none, list);
        return;
    }
    let width = usize::from(list.width);
    let lines = entries.iter().map(|entry| match entry {
        PaletteEntry::Action(spec) => {
            let keys: Vec<String> = action_keys(state, spec)
                .iter()
                .map(ToString::to_string)
                .collect();
            spread(
                vec![Span::raw(spec.description_in(state))],
                vec![Span::styled(keys.join(", "), style::accent(theme))],
                width,
            )
        }
        PaletteEntry::Task(task) => task_line(state, task, width),
        PaletteEntry::Project(project) => project_line(state, project, width),
    });
    let selected = app::palette_highlight(palette.selected, entries.len());
    render_list(state, lines.collect(), selected, frame, list);
}

/// The keys that run `spec` on the screen under the overlay or, when none
/// do there, the keys it has where it applies.
fn action_keys(state: &State, spec: &keys::ActionSpec) -> Vec<keys::KeySequence> {
    let here = state
        .keymap
        .keys_for(spec.action, &keys::screen_contexts(state));
    if here.is_empty() {
        state.keymap.keys_for(spec.action, spec.contexts)
    } else {
        here
    }
}

/// Draws the popup with `title`, its query line after `prompt`, and a rule
/// under it, near the top of `area`. Returns the area left for the list.
pub(super) fn finder(
    state: &State,
    title: &'static str,
    prompt: &'static str,
    query: &str,
    frame: &mut Frame,
    area: Rect,
) -> Rect {
    let theme = state.theme();
    let inside = area.inner(Margin::new(1, 1));
    let width = MAX_WIDTH.min(inside.width);
    let popup = Rect {
        x: inside.x + (inside.width - width) / 2,
        y: inside.y,
        width,
        height: MAX_HEIGHT.min(inside.height),
    };
    let block: Block = super::popup_block(state)
        .title(title)
        .title_bottom(super::close_hint(state));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);

    let room = usize::from(inner.width);
    let query_line = text::fit_line(
        Line::from(vec![
            Span::styled(format!(" {prompt} "), style::accent(theme)),
            Span::raw(query.to_owned()),
            Span::styled(" ", Style::new().add_modifier(Modifier::REVERSED)),
        ]),
        room,
    );
    let rows: Vec<Rect> = inner.rows().collect();
    let [query_row, rule_row, ..] = rows[..] else {
        return Rect::default();
    };
    frame.render_widget(query_line, query_row);
    frame.render_widget(
        Line::styled(
            "─".repeat(usize::from(rule_row.width)),
            style::border(theme),
        ),
        rule_row,
    );
    Rect {
        y: rule_row.y + 1,
        height: inner.height.saturating_sub(2),
        ..inner
    }
}

/// Draws `lines` in `area`, scrolled so the `selected` one shows, and
/// highlights it.
pub(super) fn render_list(
    state: &State,
    lines: Vec<Line<'static>>,
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let rows = usize::from(area.height);
    let offset = (selected + 1).saturating_sub(rows);
    for (row, (index, line)) in area.rows().zip(lines.into_iter().enumerate().skip(offset)) {
        frame.render_widget(Paragraph::new(line), row);
        if index == selected {
            frame.buffer_mut().set_style(row, state.theme().selection);
        }
    }
}

/// A task: its check glyph and content on the left, its project on the
/// right.
fn task_line(state: &State, task: &Task, width: usize) -> Line<'static> {
    let theme = state.theme();
    let project = state
        .projects
        .iter()
        .find(|project| project.id == task.project_id)
        .map_or(String::new(), |project| project.name.clone());
    spread(
        vec![
            Span::styled(state.icons().check(task.checked), style::muted(theme)),
            Span::raw(" "),
            Span::raw(task.content.clone()),
        ],
        vec![Span::styled(project, style::muted(theme))],
        width,
    )
}

/// A project: its glyph and name on the left, the word "project" on the
/// right.
fn project_line(state: &State, project: &Project, width: usize) -> Line<'static> {
    let theme = state.theme();
    let icons = state.icons();
    let glyph = if project.is_inbox() {
        icons.inbox
    } else if project.is_favorite {
        icons.favourite
    } else {
        icons.project
    };
    spread(
        vec![
            Span::styled(glyph, style::muted(theme)),
            Span::raw(" "),
            Span::raw(project.name.clone()),
        ],
        vec![Span::styled("project", style::muted(theme))],
        width,
    )
}

/// `left` and `right` on one line `width` wide, a space in from each edge,
/// with `left` cut short when both do not fit.
fn spread(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let right_width: usize = right.iter().map(Span::width).sum();
    let room = width.saturating_sub(right_width + 3);
    let left = text::fit_line(Line::from(left), room);
    let used = left.width();
    let mut spans = vec![Span::raw(" ")];
    spans.extend(left.spans);
    spans.push(Span::raw(
        " ".repeat(width.saturating_sub(used + right_width + 2).max(1)),
    ));
    spans.extend(right);
    Line::from(spans)
}

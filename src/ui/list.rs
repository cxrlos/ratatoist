//! A task list and the focused task's detail: where the panes go, the task
//! rows and group headers, and the list filter header above them.

use std::rc::Rc;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

use super::icons::Icons;
use super::theme::Theme;
use super::{dates, detail, style, text, upcoming};
use crate::app::{
    Action, Group, GroupKind, ListFilter, ListRow, Modal, Pane, Prompt, Screen, Sort, State,
};
use crate::keys;
use crate::todoist::model::{Priority, Task};

/// The columns a row's content keeps before its due date shows beside it.
const CONTENT_BESIDE_DUE: u16 = 30;

/// The fewest columns a due date longer than that is cut to before it
/// gives way to the content.
const DUE_MIN_COLUMNS: u16 = 10;

/// The widest a project name gets on Today and Upcoming before it is cut.
const PROJECT_WIDTH: usize = 16;

/// A task list screen in `area`: the list filter header when the list is
/// filtered or sorted, then the list, and the focused task's detail beside
/// it from `ui.wide-layout-columns` columns on. Below that only the focused
/// pane shows.
pub fn render(state: &State, frame: &mut Frame, area: Rect) {
    let header = filter_header(state);
    let [header_area, area] = split_header(area, header.is_some());
    if let Some(header) = header {
        text::draw(frame, header, header_area);
    }
    let (list, detail) = panes(state, area);
    if let Some((list_area, focus)) = list {
        render_list(state, frame, list_area, focus);
    }
    if let Some((detail_area, focus)) = detail {
        let editing = prompt_in_place(state, area.width);
        detail::render(state, frame, detail_area, focus, editing);
    }
}

/// Where the panes of a task list screen go in the body `area`: under the
/// list filter header when it shows.
pub fn panes_area(state: &State, area: Rect) -> Rect {
    split_header(area, filter_header(state).is_some())[1]
}

fn split_header(area: Rect, header: bool) -> [Rect; 2] {
    Layout::vertical([Constraint::Length(u16::from(header)), Constraint::Fill(1)]).areas(area)
}

/// Where a pane goes and how it shows focus, or `None` when it does not show.
pub type Placed = Option<(Rect, Focus)>;

/// Where the list and the detail go in `area`, and how each shows focus.
/// From `ui.wide-layout-columns` columns on they sit side by side, the
/// list taking three fifths; below it only the focused one shows. A prompt
/// editing the shown task in place gives the detail the focus.
pub fn panes(state: &State, area: Rect) -> (Placed, Placed) {
    let pane = if prompt_in_place(state, area.width).is_some() {
        Pane::Detail
    } else {
        state.active_pane
    };
    if area.width >= state.wide_layout_columns {
        let [list_area, detail_area] =
            Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2)]).areas(area);
        let focus = |of| {
            if pane == of {
                Focus::Focused
            } else {
                Focus::Unfocused
            }
        };
        (
            Some((list_area, focus(Pane::Tasks))),
            Some((detail_area, focus(Pane::Detail))),
        )
    } else if pane == Pane::Detail {
        (None, Some((area, Focus::Alone)))
    } else {
        (Some((area, Focus::Alone)), None)
    }
}

/// The prompt on top, when it edits the task whose detail shows on a body
/// `width` columns wide: then the detail draws it in place of the field.
/// Never while loading or on an error screen, when the body shows no
/// detail.
pub fn prompt_in_place(state: &State, width: u16) -> Option<&Prompt> {
    let Some(Modal::Prompt(prompt)) = state.top_modal() else {
        return None;
    };
    let detail_shows = !state.loading
        && state.load_error.is_none()
        && (state.active_pane == Pane::Detail
            || (state.active_pane == Pane::Tasks && width >= state.wide_layout_columns));
    let shown = state.selected_task().map(|task| task.id.as_str());
    (detail_shows && shown == Some(prompt.task_id.as_str())).then_some(prompt)
}

/// How a pane shows focus: a thick accented border beside an unfocused pane,
/// a plain one when it is the only pane on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Alone,
    Focused,
    Unfocused,
}

/// A bordered pane drawn in `area`, its title cut with `…` to fit.
pub fn pane_block(
    title: impl Into<Line<'static>>,
    area: Rect,
    focus: Focus,
    theme: &Theme,
) -> Block<'static> {
    let room = usize::from(area.width.saturating_sub(2));
    let block = Block::bordered().title(text::fit_line(title.into(), room));
    match focus {
        Focus::Alone => block.border_style(style::border(theme)),
        Focus::Focused => block
            .border_type(BorderType::Thick)
            .border_style(theme.active_border()),
        Focus::Unfocused => block.border_style(style::muted(theme)),
    }
}

/// The task list in its pane: its rows scrolled so the focused task shows,
/// or one muted sentence when it is empty.
fn render_list(state: &State, frame: &mut Frame, area: Rect, focus: Focus) {
    let theme = state.theme();
    let block = pane_block(state.list_title(), area, focus, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = state.list_rows();
    if rows.is_empty() {
        let empty = Line::styled(state.empty_list_sentence(), style::muted(theme));
        text::draw(frame, empty, inner);
        return;
    }
    let focused_line = focused_line(&rows, state.selected_task);
    let offset = visible_offset(focused_line, inner);
    let cells = row_cells(state, &rows);
    let mut index = rows
        .iter()
        .take(offset)
        .filter(|row| matches!(row, ListRow::Task { .. }))
        .count();
    for (line_area, row) in inner.rows().zip(rows.iter().skip(offset)) {
        match row {
            ListRow::Header(group) => text::draw(frame, group_line(state, group), line_area),
            ListRow::Task { task, depth } => {
                let is_focused = index == state.selected_task;
                render_row(
                    state, frame, line_area, &cells, task, *depth, is_focused, focus,
                );
                index += 1;
            }
        }
    }
}

/// The line of `rows` that holds task `selected`, counting tasks only.
fn focused_line(rows: &[ListRow], selected: usize) -> usize {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| matches!(row, ListRow::Task { .. }))
        .nth(selected)
        .map_or(0, |(line, _)| line)
}

/// The first line to draw so the focused line shows in `inner`.
pub fn visible_offset(focused_line: usize, inner: Rect) -> usize {
    (focused_line + 1).saturating_sub(usize::from(inner.height))
}

/// The columns of a task row: marker, content, priority and, on Today and
/// Upcoming, the project.
pub struct Cells {
    layout: Layout,
    show_project: bool,
}

impl Cells {
    fn split(&self, area: Rect) -> Rc<[Rect]> {
        self.layout.split(area)
    }
}

/// The columns of the rows of `rows`: the priority as wide as its widest
/// glyph and name, the project as wide as the widest project name shown.
pub fn row_cells(state: &State, rows: &[ListRow]) -> Cells {
    let icons = state.icons();
    let priority = Priority::ALL
        .iter()
        .map(|p| text::width(icons.priority(*p)) + 1 + text::width(p.label()))
        .max()
        .unwrap_or(0);
    let show_project = matches!(state.screen, Screen::Today | Screen::Upcoming);
    let mut constraints = vec![
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(u16::try_from(priority).unwrap_or(u16::MAX)),
    ];
    if show_project {
        let project = rows
            .iter()
            .filter_map(|row| match row {
                ListRow::Task { task, .. } => Some(text::width(project_name(state, task))),
                ListRow::Header(_) => None,
            })
            .max()
            .unwrap_or(0)
            .min(PROJECT_WIDTH);
        constraints.push(Constraint::Length(u16::try_from(project).unwrap_or(0)));
    }
    Cells {
        layout: Layout::horizontal(constraints).spacing(1),
        show_project,
    }
}

/// One task row: the marker, then the subtask indent, the check (or the
/// fold of a task whose subtasks nest under it), the content and its
/// muted marks, the due date while the content keeps room, the priority and
/// the project. The focused row is reversed in the focused pane and bold
/// in the other.
#[allow(clippy::too_many_arguments)]
pub fn render_row(
    state: &State,
    frame: &mut Frame,
    line_area: Rect,
    cells: &Cells,
    task: &Task,
    depth: usize,
    is_focused: bool,
    focus: Focus,
) {
    let theme = state.theme();
    let icons = state.icons();
    let areas = cells.split(line_area);
    let (Some(&marker), Some(&body), Some(&priority)) = (areas.first(), areas.get(1), areas.get(2))
    else {
        return;
    };
    let failed = state.rollback_reason(&task.id).is_some();
    if failed {
        text::draw(
            frame,
            Span::styled(icons.failed, style::error(theme)),
            marker,
        );
    } else if is_focused {
        text::draw(frame, Span::raw(icons.focus), marker);
    }

    // The due takes what the content leaves past its 30 columns, cut with
    // `…` when it is longer, as a recurring or not yet parsed due string
    // can be; with less room than the due or its first columns, it gives way.
    let due_room = body.width.saturating_sub(CONTENT_BESIDE_DUE + 1);
    let due = due_span(state, task)
        .map(|due| {
            let width = u16::try_from(text::width(&due.content)).unwrap_or(u16::MAX);
            (width, due)
        })
        .filter(|(width, _)| due_room >= (*width).min(DUE_MIN_COLUMNS))
        .map(|(width, due)| (width.min(due_room), due));
    let content = match due {
        Some((width, due)) => {
            let [content, due_area] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)])
                    .spacing(1)
                    .areas(body);
            text::draw(frame, due, due_area);
            content
        }
        None => body,
    };
    text::draw(
        frame,
        content_line(
            state,
            task,
            depth,
            state.icons(),
            usize::from(content.width),
        ),
        content,
    );

    let level = Priority::from_api(task.priority);
    let glyph_style = if task.checked {
        style::muted(theme)
    } else {
        theme.priority_style(task.priority)
    };
    text::draw(
        frame,
        Line::from(vec![
            Span::styled(icons.priority(level), glyph_style),
            Span::raw(" "),
            Span::raw(level.label()),
        ]),
        priority,
    );
    if cells.show_project
        && let Some(&project) = areas.get(3)
    {
        let name = Span::styled(project_name(state, task).to_owned(), style::muted(theme));
        text::draw(frame, name, project);
    }

    if is_focused {
        let highlight = if focus == Focus::Unfocused {
            theme.unfocused_selection()
        } else {
            theme.selected_item()
        };
        frame.buffer_mut().set_style(line_area, highlight);
    }
}

/// What separates the content and each mark after it.
const MARK_GAP: &str = "  ";

/// The indent, the check or fold, the content and the marks after it
/// (`@labels`, recurring, the deadline and the comment count), drawn with
/// `icons` into `room` cells. A deadline mark whose date would be cut
/// shrinks to its glyph, so no date ever reads as a wrong one.
pub fn content_line(
    state: &State,
    task: &Task,
    depth: usize,
    icons: &Icons,
    room: usize,
) -> Line<'static> {
    let theme = state.theme();
    let muted = style::muted(theme);
    let nests = matches!(state.screen, Screen::Inbox | Screen::Projects);
    let lead = if nests && !task.checked && state.has_children(&task.id) {
        icons.fold(!state.is_collapsed(&task.id))
    } else {
        icons.check(task.checked)
    };
    let content_style = if state.rollback_reason(&task.id).is_some() {
        style::error(theme)
    } else if task.checked {
        muted.add_modifier(Modifier::CROSSED_OUT)
    } else if state.is_context_task(task) {
        muted
    } else {
        style::base(theme)
    };
    let mut spans = vec![
        Span::raw("  ".repeat(depth)),
        Span::styled(format!("{lead} "), muted),
        Span::styled(task.content.clone(), content_style),
    ];
    if task.checked {
        return Line::from(spans);
    }
    let mut marks = Vec::new();
    if !task.labels.is_empty() {
        let labels: Vec<String> = task
            .labels
            .iter()
            .map(|label| format!("@{label}"))
            .collect();
        marks.push((labels.join(" "), muted));
    }
    if task.due.as_ref().is_some_and(|due| due.is_recurring) {
        marks.push((icons.recurring.to_owned(), muted));
    }
    let comments = task
        .note_count
        .filter(|count| *count > 0)
        .map(|count| (format!("{}{count}", icons.comments), muted));
    if let Some(deadline) = task
        .deadline
        .as_ref()
        .and_then(|deadline| dates::format_deadline(deadline, state.today, theme))
    {
        let full = format!("{} {}", icons.deadline, deadline.text);
        let mark_width = |mark: &str| text::width(MARK_GAP) + text::width(mark);
        let before = spans
            .iter()
            .map(|span| text::width(&span.content))
            .sum::<usize>()
            + marks
                .iter()
                .map(|(mark, _)| mark_width(mark))
                .sum::<usize>();
        let end = before + mark_width(&full);
        let after = comments.as_ref().map_or(0, |(mark, _)| mark_width(mark));
        // A line that overflows ends in `…` in its last cell, so the date
        // stays whole only if the line fits or the date ends before that cell.
        let whole = end + after <= room || end < room;
        let mark = if whole {
            full
        } else {
            icons.deadline.to_owned()
        };
        marks.push((mark, deadline.style));
    }
    marks.extend(comments);
    for (mark, style) in marks {
        spans.push(Span::styled(format!("{MARK_GAP}{mark}"), style));
    }
    Line::from(spans)
}

/// When an open `task` is due, styled by how near it is; a due string
/// Todoist has not parsed yet shows muted, ending in `…`.
fn due_span(state: &State, task: &Task) -> Option<Span<'static>> {
    if task.checked {
        return None;
    }
    if let Some(pending) = state.pending_due(&task.id) {
        return Some(Span::styled(
            format!("{pending}{}", text::ELLIPSIS),
            style::muted(state.theme()),
        ));
    }
    let due = dates::format_due(
        task.due.as_ref()?,
        state.today,
        state.now.time_zone(),
        state.theme(),
    );
    Some(Span::styled(due.text, due.style))
}

fn project_name<'a>(state: &'a State, task: &'a Task) -> &'a str {
    state
        .store
        .projects
        .by_id(&task.project_id)
        .map_or(task.project_id.as_str(), |project| project.name.as_str())
}

/// A group header: its glyph, its name in bold and how many tasks it
/// holds, and the fold glyph when they are folded away.
pub fn group_line(state: &State, group: &Group) -> Line<'static> {
    let theme = state.theme();
    let icons = state.icons();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let name = match &group.kind {
        GroupKind::Overdue => {
            spans.push(Span::styled(icons.overdue, theme.due_overdue()));
            spans.push(Span::raw(" "));
            "Overdue".to_owned()
        }
        GroupKind::Today => {
            spans.push(Span::styled(icons.today, theme.due_today()));
            spans.push(Span::raw(" "));
            "Today".to_owned()
        }
        GroupKind::Section(name) => name.clone().unwrap_or_else(|| "No section".to_owned()),
        GroupKind::Day(day) => upcoming::day_label(*day),
        GroupKind::Project(name) => name.clone(),
    };
    spans.push(Span::styled(name, bold));
    spans.push(Span::raw(format!(" ({})", group.count)));
    if group.folded {
        spans.push(Span::styled(
            format!(" {}", icons.fold(false)),
            style::muted(theme),
        ));
    }
    Line::from(spans)
}

/// `f filter: p1 · 3 of 42 · Esc clears`, with `vs sort: priority` when
/// the list is sorted: the list filter and sort in force, with the keys
/// that change them. `None` while both are the defaults.
pub fn filter_header(state: &State) -> Option<Line<'static>> {
    let filtered = state.list_filter != ListFilter::default();
    let sorted = state.sort != Sort::default();
    if !filtered && !sorted {
        return None;
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    if filtered {
        spans.extend(key_span(state, Action::FilterList));
        spans.push(Span::raw(format!(
            "filter: {}",
            state.list_filter.label().to_lowercase()
        )));
        spans.push(Span::styled(
            format!(
                " · {} of {}",
                state.task_count_with(state.list_filter),
                state.task_count_with(ListFilter::default())
            ),
            style::muted(state.theme()),
        ));
    }
    if sorted {
        if filtered {
            spans.push(Span::raw(" · "));
        }
        spans.extend(key_span(state, Action::SortList));
        spans.push(Span::raw(format!(
            "sort: {}",
            state.sort.label().to_lowercase()
        )));
    }
    if filtered {
        spans.push(Span::raw(" · "));
        spans.extend(key_span(state, Action::Close));
        spans.push(Span::raw("clears"));
    }
    Some(Line::from(spans))
}

/// The first key that runs `action` on the screen, accented and followed
/// by a space; nothing when no key does.
fn key_span(state: &State, action: Action) -> Vec<Span<'static>> {
    state
        .keymap
        .keys_for(action, &keys::screen_contexts(state))
        .first()
        .map(|key| {
            vec![
                Span::styled(key.to_string(), style::accent(state.theme())),
                Span::raw(" "),
            ]
        })
        .unwrap_or_default()
}

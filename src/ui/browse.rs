//! The Projects and Labels screens: one row per folder, project or label,
//! the focused one highlighted, each with its open-task count; Enter lists
//! the focused one's tasks.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::list::{self, Focus};
use super::{style, text};
use crate::app::{State, TreeRow};

/// The Projects tree in `area`: the personal space, then each workspace
/// with its folders, the projects indented under their parents.
pub fn render_projects(state: &State, frame: &mut Frame, area: Rect) {
    let theme = state.theme();
    let icons = state.icons();
    let block = list::pane_block("Projects", area, Focus::Alone, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = state.tree_rows();
    if rows.iter().all(|row| matches!(row, TreeRow::Space(_))) {
        empty(state, frame, inner, "No projects.");
        return;
    }
    let focused = state.browse_focus_index();
    let focused_line = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.item(state).is_some())
        .nth(focused)
        .map_or(0, |(line, _)| line);
    let counts: Vec<Option<usize>> = rows
        .iter()
        .map(|row| match row {
            TreeRow::Project { index, .. } => state
                .projects
                .get(*index)
                .map(|project| state.open_task_count(&project.id)),
            TreeRow::Space(_) | TreeRow::Folder { .. } => None,
        })
        .collect();
    let cells = cells();
    let offset = list::visible_offset(focused_line, inner);
    let muted = style::muted(theme);
    for (line, (line_area, (row, count))) in inner
        .rows()
        .zip(rows.iter().zip(&counts).skip(offset))
        .enumerate()
    {
        let is_focused = line + offset == focused_line;
        let [marker, name_area] = cells.areas(line_area);
        let mut name = match row {
            TreeRow::Space(name) => Line::styled(
                name.clone().unwrap_or_else(|| "Personal".to_owned()),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            TreeRow::Folder { name, open, .. } => Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", icons.folder(*open)), muted),
                Span::raw(name.clone()),
            ]),
            TreeRow::Project {
                index,
                depth,
                in_folder,
            } => {
                let Some(project) = state.projects.get(*index) else {
                    continue;
                };
                if state.rolled_back_projects.contains_key(&project.id) {
                    text::draw(
                        frame,
                        Span::styled(icons.failed, style::error(theme)),
                        marker,
                    );
                }
                let indent = 1 + depth + usize::from(*in_folder);
                let glyph = if project.is_favorite {
                    Span::styled(format!("{} ", icons.favourite), theme.favorite_icon())
                } else {
                    Span::styled(
                        format!("{} ", icons.project),
                        theme.project_dot(&project.color),
                    )
                };
                Line::from(vec![
                    Span::raw("  ".repeat(indent)),
                    glyph,
                    Span::raw(project.name.clone()),
                ])
            }
        };
        if is_focused && marker_is_free(state, row) {
            text::draw(frame, Span::raw(icons.focus), marker);
        }
        if let Some(count) = count {
            name.push_span(Span::styled(format!("  {count}"), muted));
        }
        text::draw(frame, name, name_area);
        if is_focused {
            frame.buffer_mut().set_style(line_area, theme.selection);
        }
    }
}

/// Every label with its open-task count, favourites first.
pub fn render_labels(state: &State, frame: &mut Frame, area: Rect) {
    let theme = state.theme();
    let icons = state.icons();
    let block = list::pane_block("Labels", area, Focus::Alone, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = state.label_rows();
    if rows.is_empty() {
        empty(state, frame, inner, "No labels.");
        return;
    }
    let focused = state.browse_focus_index();
    let cells = cells();
    let offset = list::visible_offset(focused, inner);
    let muted = style::muted(theme);
    for (line, (line_area, row)) in inner.rows().zip(rows.iter().skip(offset)).enumerate() {
        let is_focused = line + offset == focused;
        let [marker, name_area] = cells.areas(line_area);
        if is_focused {
            text::draw(frame, Span::raw(icons.focus), marker);
        }
        let mut name = vec![Span::raw(format!("@{}", row.name))];
        if row.favourite {
            name.push(Span::raw(" "));
            name.push(Span::styled(icons.favourite, theme.favorite_icon()));
        }
        name.push(Span::styled(format!("  {}", row.count), muted));
        text::draw(frame, Line::from(name), name_area);
        if is_focused {
            frame.buffer_mut().set_style(line_area, theme.selection);
        }
    }
}

/// The columns of a row: the marker, then the name and its muted count.
fn cells() -> Layout {
    Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).spacing(1)
}

/// Whether `row`'s marker column is free for the focus marker: a project
/// whose write was rolled back keeps the failed mark there.
fn marker_is_free(state: &State, row: &TreeRow) -> bool {
    match row {
        TreeRow::Project { index, .. } => state
            .projects
            .get(*index)
            .is_none_or(|project| !state.rolled_back_projects.contains_key(&project.id)),
        TreeRow::Space(_) | TreeRow::Folder { .. } => true,
    }
}

fn empty(state: &State, frame: &mut Frame, area: Rect, sentence: &str) {
    let line = Line::styled(sentence.to_owned(), style::muted(state.theme()));
    text::draw(frame, line, area);
}

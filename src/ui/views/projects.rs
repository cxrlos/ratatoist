use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding};

use crate::app::{ProjectEntry, State};
use crate::ui::text;

/// The Projects tree in its bordered block, filling `area`.
pub fn render_block(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();
    let block = Block::default()
        .title(" Projects ")
        .title_style(theme.active_title())
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render(frame, state, inner, true);
}

pub fn render(frame: &mut Frame, state: &State, area: Rect, is_active: bool) {
    let theme = state.theme();
    let icons = state.icons();
    let entries = state.project_list_entries();

    let selected_visual = entries.iter().position(|e| match e {
        ProjectEntry::Project(i) => state.folder_cursor.is_none() && *i == state.selected_project,
        ProjectEntry::FolderHeader(fi) => state.folder_cursor == Some(*fi),
        _ => false,
    });

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| match entry {
            ProjectEntry::PersonalHeader => {
                let name = state.current_user_name.as_deref().unwrap_or("Personal");
                Line::from(Span::styled(
                    format!("  {name}"),
                    theme.muted_text().add_modifier(Modifier::BOLD),
                ))
            }

            ProjectEntry::WorkspaceHeader(wi) => {
                let name = state
                    .workspaces
                    .get(*wi)
                    .map(|w| w.name.as_str())
                    .unwrap_or("");
                Line::from(Span::styled(
                    format!("  {name}"),
                    theme.label_tag().add_modifier(Modifier::BOLD),
                ))
            }

            ProjectEntry::FolderHeader(fi) => {
                let folder = state.folders.get(*fi);
                let name = folder.map(|f| f.name.as_str()).unwrap_or("");
                let collapsed = folder
                    .map(|f| state.collapsed_folders.contains(&f.id))
                    .unwrap_or(false);
                let arrow = icons.folder(!collapsed);
                Line::from(Span::styled(
                    format!("    {arrow} {name}"),
                    theme.muted_text(),
                ))
            }

            ProjectEntry::Separator => Line::default(),

            ProjectEntry::Project(i) => {
                let project = &state.projects[*i];
                let indent = "  ".repeat(state.project_indent(project));
                let dot = theme.project_dot(&project.color);
                let icon = if project.is_inbox() {
                    Span::styled(format!("{} ", icons.inbox), theme.inbox_icon())
                } else if project.is_favorite {
                    Span::styled(format!("{} ", icons.favourite), theme.favorite_icon())
                } else {
                    Span::styled(format!("{} ", icons.project), dot)
                };

                Line::from(vec![
                    Span::raw(indent),
                    icon,
                    Span::styled(&project.name, theme.normal_text()),
                ])
            }
        })
        .map(|line| ListItem::new(text::fit_line(line, usize::from(area.width))))
        .collect();

    if items.is_empty() {
        frame.render_widget(List::new(items), area);
        return;
    }

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.unfocused_selection()
    };

    let list = List::new(items).highlight_style(highlight_style);
    let mut list_state = ListState::default().with_selected(selected_visual);
    frame.render_stateful_widget(list, area, &mut list_state);
}

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::{App, ProjectEntry};

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let theme = app.theme();
    let entries = app.project_list_entries();

    let selected_visual = entries.iter().position(|e| match e {
        ProjectEntry::Project(i) => app.folder_cursor.is_none() && *i == app.selected_project,
        ProjectEntry::FolderHeader(fi) => app.folder_cursor == Some(*fi),
        _ => false,
    });

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| match entry {
            ProjectEntry::PersonalHeader => {
                let name = app.current_user_name.as_deref().unwrap_or("Personal");
                ListItem::new(Line::from(Span::styled(
                    format!("  {name}"),
                    theme.muted_text().add_modifier(Modifier::BOLD),
                )))
            }

            ProjectEntry::WorkspaceHeader(wi) => {
                let name = app.workspaces.get(*wi).map(|w| w.name.as_str()).unwrap_or("");
                ListItem::new(Line::from(Span::styled(
                    format!("  {name}"),
                    theme.label_tag().add_modifier(Modifier::BOLD),
                )))
            }

            ProjectEntry::FolderHeader(fi) => {
                let folder = app.folders.get(*fi);
                let name = folder.map(|f| f.name.as_str()).unwrap_or("");
                let collapsed = folder
                    .map(|f| app.collapsed_folders.contains(&f.id))
                    .unwrap_or(false);
                let arrow = if collapsed { "▸" } else { "▾" };
                ListItem::new(Line::from(Span::styled(
                    format!("    {arrow} {name}"),
                    theme.muted_text(),
                )))
            }

            ProjectEntry::Separator => ListItem::new(Line::default()),

            ProjectEntry::Project(i) => {
                let project = &app.projects[*i];
                let indent = "  ".repeat(app.project_indent(project));
                let dot_color = theme.color_for(&project.color);
                let is_parent = app
                    .projects
                    .iter()
                    .any(|p| p.parent_id.as_deref() == Some(project.id.as_str()));

                let icon = if project.is_inbox() {
                    Span::styled(" ", theme.inbox_icon())
                } else if project.is_favorite {
                    Span::styled("★ ", theme.favorite_icon())
                } else if is_parent {
                    Span::styled(" ", Style::default().fg(dot_color))
                } else {
                    Span::styled("# ", Style::default().fg(dot_color))
                };

                ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    icon,
                    Span::styled(&project.name, theme.normal_text()),
                ]))
            }
        })
        .collect();

    if items.is_empty() {
        frame.render_widget(List::new(items), area);
        return;
    }

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);
    let mut state = ListState::default().with_selected(selected_visual);
    frame.render_stateful_widget(list, area, &mut state);
}

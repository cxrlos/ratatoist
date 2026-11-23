use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::{ProjectEntry, State};
use crate::ui::text;

pub fn render(frame: &mut Frame, state: &State, area: Rect, is_active: bool) {
    let theme = state.theme();
    let entries = state.project_list_entries();

    let selected_visual = entries.iter().position(|e| match e {
        ProjectEntry::Project(i) => {
            !state.today_view_active
                && state.folder_cursor.is_none()
                && *i == state.selected_project
        }
        ProjectEntry::FolderHeader(fi) => state.folder_cursor == Some(*fi),
        ProjectEntry::TodayView => state.today_view_active && state.folder_cursor.is_none(),
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
                let arrow = if collapsed { "▸" } else { "▾" };
                Line::from(Span::styled(
                    format!("    {arrow} {name}"),
                    theme.muted_text(),
                ))
            }

            ProjectEntry::Separator => Line::default(),

            ProjectEntry::TodayView => {
                let stats = state.overview_stats();
                let count = stats.overdue + stats.due_today;
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled("⊙ ", Style::default().fg(Color::Yellow)),
                    Span::styled("Today", theme.normal_text()),
                ];
                if count > 0 {
                    spans.push(Span::styled(format!("  {count}"), theme.muted_text()));
                }
                Line::from(spans)
            }

            ProjectEntry::Project(i) => {
                let project = &state.projects[*i];
                let indent = "  ".repeat(state.project_indent(project));
                let dot_color = theme.color_for(&project.color);
                let is_parent = state
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
        theme.subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);
    let mut list_state = ListState::default().with_selected(selected_visual);
    frame.render_stateful_widget(list, area, &mut list_state);
}

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{InputMode, Pane, State, VimState};

pub fn render(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();

    let mode_style = match state.input_mode {
        InputMode::Vim(VimState::Normal) => theme.mode_normal(),
        InputMode::Vim(VimState::Insert) => theme.mode_insert(),
        InputMode::Standard => theme.mode_standard(),
    };

    let mode_label = format!(" {} ", state.input_mode.label());

    let project_name = state.selected_project_name();
    let task_count = state.visible_tasks().len();

    let breadcrumb = match state.active_pane {
        Pane::Projects => format!("  {project_name}"),
        Pane::Tasks => format!("  {project_name} ▸ {task_count} tasks"),
        Pane::Detail => {
            let task_name = state
                .selected_task()
                .map(|t| t.content.as_str())
                .unwrap_or("Task");
            format!("  {project_name} ▸ {task_name}")
        }
        Pane::Settings => "  Settings".to_string(),
        Pane::StatsDock => format!("  {project_name} ▸ weekly progress"),
    };

    let (ws_dot, ws_label, dot_style) = if state.websocket_connected {
        if state.is_idle() {
            (
                "◌",
                format!("Idle (last sync @ {})", state.sync_age_label()),
                theme.muted_text(),
            )
        } else {
            ("●", "Connected".to_string(), theme.success())
        }
    } else {
        ("○", "Offline".to_string(), theme.muted_text())
    };

    let status_str = format!("{ws_label} {ws_dot} ");
    let status_width = status_str.chars().count() as u16;

    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(status_width)]).areas(area);

    let spans = vec![
        Span::styled(mode_label, mode_style),
        Span::styled(breadcrumb, theme.subtle_text()),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.surface_bg()),
        left,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(ws_label, theme.muted_text()),
            Span::styled(format!(" {ws_dot} "), dot_style),
        ]))
        .style(theme.surface_bg()),
        right,
    );
}

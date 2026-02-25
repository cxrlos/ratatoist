use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, InputMode, Pane, VimState};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();

    let mode_style = match app.input_mode {
        InputMode::Vim(VimState::Normal) => theme.mode_normal(),
        InputMode::Vim(VimState::Visual) => theme.mode_visual(),
        InputMode::Vim(VimState::Insert) => theme.mode_insert(),
        InputMode::Standard => theme.mode_standard(),
    };

    let mode_label = format!(" {} ", app.input_mode.label());

    let project_name = app.selected_project_name();
    let task_count = app.visible_tasks().len();

    let breadcrumb = match app.active_pane {
        Pane::Projects => format!("  {project_name}"),
        Pane::Tasks => format!("  {project_name} ▸ {task_count} tasks"),
        Pane::Detail => {
            let task_name = app
                .selected_task()
                .map(|t| t.content.as_str())
                .unwrap_or("Task");
            format!("  {project_name} ▸ {task_name}")
        }
        Pane::Settings => "  Settings".to_string(),
        Pane::StatsDock => format!("  {project_name} ▸ weekly progress"),
    };

    let (ws_dot, ws_label, dot_style) = if app.websocket_connected {
        if app.is_idle() {
            (
                "◌",
                format!("Idle (last sync @ {})", app.sync_age_label()),
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

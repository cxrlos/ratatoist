use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{Pane, State};
use crate::ui::text;

pub fn render(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();

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
    let status_width = u16::try_from(text::width(&status_str)).unwrap_or(u16::MAX);

    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(status_width)]).areas(area);

    let spans = vec![Span::styled(breadcrumb, theme.subtle_text())];

    frame.render_widget(
        Paragraph::new(text::fit_line(Line::from(spans), usize::from(left.width)))
            .style(theme.surface_bg()),
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

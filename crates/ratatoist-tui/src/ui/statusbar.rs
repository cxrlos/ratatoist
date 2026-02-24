use ratatui::Frame;
use ratatui::layout::Rect;
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
    let task_count = app.tasks.len();

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

    let spans = vec![
        Span::styled(mode_label, mode_style),
        Span::styled(breadcrumb, theme.subtle_text()),
    ];

    let bar = Paragraph::new(Line::from(spans)).style(theme.surface_bg());
    frame.render_widget(bar, area);
}

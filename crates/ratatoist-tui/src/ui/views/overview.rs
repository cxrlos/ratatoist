use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};

use crate::app::{App, SortMode};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let stats = app.overview_stats();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.inactive_border())
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let bar_width = 20usize;
    let filled = if stats.week_total > 0 {
        (bar_width as f64 * stats.week_done as f64 / stats.week_total as f64) as usize
    } else {
        0
    };
    let progress_bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);

    let sort_label = app.sort_mode.label();
    let sort_indicator = if app.sort_mode != SortMode::Default {
        format!("  ⟳ {sort_label}")
    } else {
        String::new()
    };

    let line = Line::from(vec![
        Span::styled(
            format!("▲ {}", stats.overdue),
            if stats.overdue > 0 {
                theme.due_overdue()
            } else {
                theme.muted_text()
            },
        ),
        Span::styled("  ", theme.muted_text()),
        Span::styled(format!("◆ {}", stats.due_today), theme.due_today()),
        Span::styled("  ", theme.muted_text()),
        Span::styled(format!("◇ {}", stats.due_week), theme.due_upcoming()),
        Span::styled("  │  ", theme.muted_text()),
        Span::styled(progress_bar, theme.success()),
        Span::styled(format!(" {}%", stats.week_progress), theme.muted_text()),
        Span::styled(sort_indicator, theme.due_upcoming()),
    ]);

    frame.render_widget(Paragraph::new(line), inner);
}

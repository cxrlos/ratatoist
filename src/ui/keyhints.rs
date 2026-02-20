use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Pane};

use super::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match app.active_pane {
        Pane::Projects => vec![
            hint("j/k", "navigate"),
            hint("Enter", "open"),
            hint("Tab", "tasks"),
            hint("q", "quit"),
        ],
        Pane::Tasks => vec![
            hint("j/k", "navigate"),
            hint("Shift-Tab", "projects"),
            hint("q", "quit"),
        ],
    };

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", Theme::muted_text()));
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Theme::muted_text()));
        }
        spans.push(Span::styled(*key, Theme::key_hint()));
        spans.push(Span::styled(format!(" {desc}"), Theme::muted_text()));
    }

    let bar = Paragraph::new(Line::from(spans)).style(Theme::base_bg());
    frame.render_widget(bar, area);
}

fn hint(key: &'static str, desc: &'static str) -> (&'static str, &'static str) {
    (key, desc)
}

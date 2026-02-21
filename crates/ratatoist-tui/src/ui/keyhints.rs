use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, InputMode, Pane};

use super::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match (&app.input_mode, &app.active_pane) {
        (_, Pane::Settings) => vec![
            ("j/k", "navigate"),
            ("Enter/Space", "toggle"),
            ("Esc", "close"),
        ],
        (_, Pane::Detail) => vec![
            ("j/k", "scroll"),
            ("x", "complete"),
            ("Esc/h", "back"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (InputMode::Vim(_), Pane::Projects) => vec![
            ("j/k", "navigate"),
            ("g/G", "top/bottom"),
            ("l/Tab", "tasks"),
            (",", "settings"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (InputMode::Vim(_), Pane::Tasks) => vec![
            ("j/k", "navigate"),
            ("Enter", "open/fold"),
            ("x", "complete"),
            ("a", "add"),
            ("o", "sort"),
            ("za", "fold"),
            ("h", "back"),
            ("q", "quit"),
        ],
        (InputMode::Standard, Pane::Projects) => vec![
            ("↑/↓", "navigate"),
            ("Tab", "tasks"),
            (",", "settings"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (InputMode::Standard, Pane::Tasks) => vec![
            ("↑/↓", "navigate"),
            ("Enter", "open/fold"),
            ("Ctrl-x", "complete"),
            ("Ctrl-a", "add"),
            ("Esc", "projects"),
            ("q", "quit"),
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

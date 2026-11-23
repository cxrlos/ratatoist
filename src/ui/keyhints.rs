use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{Pane, State};
use crate::keys::Preset;
use crate::ui::text;

pub fn render(frame: &mut Frame, state: &State, area: Rect) {
    let theme = state.theme();

    let hints = match (state.keymap.preset(), &state.active_pane) {
        (_, Pane::Detail) => vec![
            ("j/k", "field"),
            ("Ctrl-d/u", "scroll"),
            ("x", "complete"),
            ("Esc/h", "back"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (Preset::Vim, Pane::Projects) => vec![
            ("j/k", "navigate"),
            ("g/G", "top/bottom"),
            ("l/Tab", "tasks"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (Preset::Vim, Pane::Tasks) => vec![
            ("j/k", "navigate"),
            ("Enter", "open/fold"),
            ("x", "complete"),
            ("a", "add"),
            ("f", "filter"),
            ("o", "sort"),
            ("za", "fold"),
            ("Esc/h", "back"),
            ("q", "quit"),
        ],
        (Preset::Standard, Pane::Projects) => vec![
            ("↑/↓", "navigate"),
            ("Tab", "tasks"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (Preset::Standard, Pane::Tasks) => vec![
            ("↑/↓", "navigate"),
            ("Enter", "open/fold"),
            ("Ctrl-x", "complete"),
            ("Ctrl-a", "add"),
            ("f", "filter"),
            ("Esc", "projects"),
            ("q", "quit"),
        ],
    };

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", theme.muted_text()));
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", theme.muted_text()));
        }
        spans.push(Span::styled(*key, theme.key_hint()));
        spans.push(Span::styled(format!(" {desc}"), theme.muted_text()));
    }

    let bar = Paragraph::new(text::fit_line(Line::from(spans), usize::from(area.width)))
        .style(theme.base_bg());
    frame.render_widget(bar, area);
}

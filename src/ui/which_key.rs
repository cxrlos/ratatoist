//! The which-key popup: after a chord's first keys, what can follow.

use super::text;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::style;
use crate::app::State;
use crate::keys::{self, Key, KeySequence};

/// Draws in the bottom-right corner inside `area`'s border.
pub fn render(state: &State, prefix: &[Key], frame: &mut Frame, area: Rect) {
    let area = area.inner(Margin::new(1, 1));
    let contexts = keys::active_contexts(state);
    let lines: Vec<Line> = state
        .keymap
        .continuations(prefix, &contexts)
        .into_iter()
        .filter_map(|(rest, action)| {
            let spec = keys::spec(action)?;
            let (key_style, text_style) = if spec.availability(state).can_run() {
                (style::accent(state.theme()), Style::new())
            } else {
                (style::muted(state.theme()), style::muted(state.theme()))
            };
            Some(Line::from(vec![
                Span::styled(format!(" {rest}  "), key_style),
                Span::styled(format!("{} ", spec.description), text_style),
            ]))
        })
        .collect();

    let title = format!(" {} ", KeySequence::from(prefix));
    let close = super::close_hint(state);
    let width = lines
        .iter()
        .map(Line::width)
        .chain([text::width(&title), close.width()])
        .max()
        .unwrap_or(0)
        + 2;
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let popup = Rect {
        x: area.right().saturating_sub(width),
        y: area.bottom().saturating_sub(height),
        width,
        height,
    };
    let edge = usize::from(popup.width.saturating_sub(2));
    let block = super::popup_block(state)
        .title(text::fit(&title, edge))
        .title_bottom(text::fit_line(close, edge));
    let lines: Vec<Line> = lines
        .into_iter()
        .map(|line| text::fit_line(line, edge))
        .collect();
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

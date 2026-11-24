//! The messages log: the failures of this session, newest first, each with
//! when it happened, its kind, the action it ended, what to do about it and
//! the raw message.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{style, text};
use crate::app::{Action, Failure, State};
use crate::keys;

const MAX_WIDTH: u16 = 80;
/// Before each line of an entry after its first.
const INDENT: &str = "   ";

/// Draws the log centred in `body`, scrolled down by `scroll` rows (no
/// further than its last row).
pub fn render(state: &State, scroll: u16, frame: &mut Frame, body: Rect) {
    let width = popup_width(body);
    let lines = lines(state, width);
    let popup = popup(body, width, &lines);
    let scroll = scroll.min(hidden_rows(popup, &lines));
    let block = super::popup_block(state)
        .title(" Messages ")
        .title_bottom(keys_hint(state));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        popup,
    );
}

/// How many rows of the log do not fit in `body`.
pub fn max_scroll(state: &State, body: Rect) -> u16 {
    let width = popup_width(body);
    let lines = lines(state, width);
    hidden_rows(popup(body, width, &lines), &lines)
}

fn hidden_rows(popup: Rect, lines: &[Line]) -> u16 {
    u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_sub(popup.height.saturating_sub(2))
}

fn popup_width(body: Rect) -> u16 {
    body.width.min(MAX_WIDTH)
}

/// The popup: as tall as its lines inside its border, up to `body`'s
/// height, centred in `body`.
fn popup(body: Rect, width: u16, lines: &[Line]) -> Rect {
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    super::centered(body, width, height)
}

/// The keys that scroll and close the log, as the bottom border shows them.
fn keys_hint(state: &State) -> Line<'static> {
    let contexts = keys::active_contexts(state);
    let first = |action| {
        state
            .keymap
            .keys_for(action, &contexts)
            .first()
            .map(ToString::to_string)
    };
    let theme = state.theme();
    let mut spans = Vec::new();
    let mut part = |keys: String, label: &str| {
        spans.push(Span::styled(format!(" {keys}"), style::accent(theme)));
        spans.push(Span::styled(format!(" {label} "), style::muted(theme)));
    };
    match (first(Action::ScrollDown), first(Action::ScrollUp)) {
        (Some(down), Some(up)) => part(format!("{down}/{up}"), "scroll"),
        (Some(key), None) | (None, Some(key)) => part(key, "scroll"),
        (None, None) => {}
    }
    if let Some(close) = first(Action::Close) {
        part(close, "close");
    }
    Line::from(spans)
}

fn lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let theme = state.theme();
    if state.failures.is_empty() {
        return vec![Line::styled(" No errors yet.", style::muted(theme))];
    }
    // The border and a space inside each side.
    let width = usize::from(width.saturating_sub(4));
    let mut lines = Vec::new();
    for (index, failure) in state.failures.newest_first().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        lines.extend(entry(state, failure, width));
    }
    lines
}

/// One failure: when, its kind and the action it ended on the first line,
/// then what went wrong with what to do about it, then the raw message,
/// muted.
fn entry(state: &State, failure: &Failure, width: usize) -> Vec<Line<'static>> {
    let theme = state.theme();
    let first = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{}  ", failure.at.strftime("%H:%M:%S")),
            style::muted(theme),
        ),
        Span::styled(failure.kind.name().to_owned(), style::error(theme)),
        Span::styled(" · ", style::muted(theme)),
        Span::raw(failure.action.clone()),
    ]);
    let mut lines = vec![text::fit_line(first, width + 1)];
    let advice = format!("{}. {}", failure.kind.summary(), failure.kind.fix());
    lines.extend(indented(&advice, width, Style::new()));
    lines.extend(indented(&failure.message, width, style::muted(theme)));
    lines
}

/// `paragraph` wrapped to `width` cells, each row after [`INDENT`].
fn indented(paragraph: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    let room = width.saturating_sub(text::width(INDENT)).max(1);
    super::wrap(paragraph, room)
        .into_iter()
        .map(|row| Line::styled(format!("{INDENT}{row}"), style))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_area_hides_the_rows_that_do_not_fit() {
        let popup = Rect::new(0, 0, 40, 5);
        let lines = vec![Line::default(); 7];

        assert_eq!(hidden_rows(popup, &lines), 4);
    }
}

//! What the body shows until the tasks arrive: a loading message, or the
//! error screen that says why they cannot load and how to fix it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use super::theme::Theme;
use super::{style, text};
use crate::app::LoadError;
use crate::auth::{AuthError, Source};
use crate::todoist::ClientError;

pub fn render_loading(theme: &Theme, title: &str, frame: &mut Frame, area: Rect) {
    render_line(
        theme,
        title,
        "Loading your tasks from Todoist…",
        frame,
        area,
    );
}

/// One muted line in a pane titled `title`.
fn render_line(theme: &Theme, title: &str, line: &str, frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .border_style(style::border(theme))
        .title(title.to_owned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    text::draw(
        frame,
        Line::styled(line.to_owned(), style::muted(theme)),
        inner,
    );
}

/// The error screen for `error`, in a pane titled `title`. A rejected
/// token is explained with `token_source`, where it came from.
pub fn render(
    error: LoadError,
    token_source: Option<Source>,
    title: &str,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let (heading, explanation) = explain(error, token_source);
    let lines = vec![
        Line::from(Span::styled(heading, style::error(theme))),
        Line::default(),
        Line::from(explanation),
    ];
    let block = Block::bordered()
        .border_style(style::border(theme))
        .title(title.to_owned())
        .padding(Padding::uniform(1));
    let screen = Paragraph::new(lines).wrap(Wrap { trim: true }).block(block);
    frame.render_widget(screen, area);
}

/// A short heading and the full explanation with its fix. They reuse the
/// messages of `ratatoist auth`.
fn explain(error: LoadError, token_source: Option<Source>) -> (&'static str, String) {
    match error {
        LoadError::NoToken => ("Not logged in", AuthError::NoToken.to_string()),
        LoadError::TokenRejected => {
            let message = match token_source {
                Some(source) => AuthError::Rejected(source).to_string(),
                None => ClientError::Unauthorized.to_string(),
            };
            ("Token rejected", sentence(&message))
        }
    }
}

/// `message` with a capital first letter and a closing full stop.
fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    let mut text: String = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if !text.ends_with(['.', '!', '?']) {
        text.push('.');
    }
    text
}

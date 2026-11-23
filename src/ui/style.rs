//! The styles lrat draws with, each from a role of the [`Theme`].

use ratatui::style::{Modifier, Style};

use super::theme::Theme;

/// The theme's text on its background, painted under everything.
pub fn base(theme: &Theme) -> Style {
    Style::new().fg(theme.text).bg(theme.background)
}

/// Keys and other things the viewer can act on now.
pub fn accent(theme: &Theme) -> Style {
    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
}

/// Things that exist but cannot be used right now.
pub fn muted(theme: &Theme) -> Style {
    Style::new().fg(theme.muted)
}

/// Messages about something that went wrong.
pub fn error(theme: &Theme) -> Style {
    Style::new().fg(theme.error)
}

/// The border of a pane alone on screen or of a popup.
pub fn border(theme: &Theme) -> Style {
    Style::new().fg(theme.border)
}

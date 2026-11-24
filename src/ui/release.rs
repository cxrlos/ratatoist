//! The release notice in the tab row.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;

use super::{style, tab_title, text, titles_width};
use crate::app::{Screen, State};

/// The release notice, muted, between the tabs and the sync indicator,
/// `indicator_width` wide. The first thing left out when the row is too
/// narrow: it shows only beside every tab's full title, so the titles never
/// shrink to their keys to make room for it.
pub fn render_release_notice(state: &State, frame: &mut Frame, area: Rect, indicator_width: u16) {
    let Some(version) = state.release.notice() else {
        return;
    };
    let text = format!("v{version} available  ");
    let width = u16::try_from(text::width(&text)).unwrap_or(u16::MAX);
    let full_tabs = titles_width(&Screen::ALL.map(|screen| tab_title(state, screen, false)));
    let taken = full_tabs.saturating_add(indicator_width);
    if taken.saturating_add(width).saturating_add(1) > area.width {
        return;
    }
    let left = area.width - indicator_width - width;
    let notice = Rect {
        x: area.x.saturating_add(left),
        width,
        ..area
    };
    frame.render_widget(Span::styled(text, style::muted(state.theme())), notice);
}

pub mod components;
pub mod dates;
pub mod hints;
pub mod icons;
pub mod keyhints;
pub mod layout;
pub mod release;
pub mod splash;
pub mod statusbar;
pub mod style;
pub mod text;
pub mod theme;
pub mod views;

pub const LOGO: &str = r#"
░█▀▄░█▀█░▀█▀░█▀█░▀█▀░█▀█░▀█▀░█▀▀░▀█▀
░█▀▄░█▀█░░█░░█▀█░░█░░█░█░░█░░▀▀█░░█░
░▀░▀░▀░▀░░▀░░▀░▀░░▀░░▀▀▀░▀▀▀░▀▀▀░░▀░
"#;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Block;

use crate::app::{Modal, State};

/// The most rows the notice above the bottom bar takes.
pub const MAX_NOTICE_ROWS: usize = 3;

/// Draws the whole screen from `state`: the splash until the first sync
/// answers, then the panes, the top overlay and any error.
pub fn draw(frame: &mut Frame, state: &State) {
    if state.loading {
        splash::render(frame, 0.3, "syncing data...", state.theme());
        return;
    }

    layout::render(frame, state);

    match state.top_modal() {
        Some(Modal::ThemePicker { selection }) => {
            components::theme_picker::render(frame, state, *selection);
        }
        Some(Modal::PriorityPicker { selection }) => {
            components::priority_picker::render(frame, *selection, state.theme());
        }
        Some(Modal::Form(form)) => components::task_form::render(frame, state, form),
        Some(Modal::Input(target)) => components::input_popup::render(frame, state, *target),
        Some(Modal::Cheatsheet { .. }) => {
            components::cheatsheet::render(frame, state.keymap.preset(), state.theme());
        }
        Some(
            Modal::WhichKey { .. }
            | Modal::Palette(_)
            | Modal::Picker(_)
            | Modal::Confirm(_)
            | Modal::Messages { .. },
        )
        | None => {}
    }

    if let Some(error) = &state.error {
        components::error_popup::render(frame, error, state.theme());
    }
}

/// The tab row, the body, the notice and the bottom bar, top to bottom. The
/// notice takes `notice_rows` rows, none when it is empty.
pub fn areas(area: Rect, notice_rows: usize) -> [Rect; 4] {
    let notice = notice_rows.min(MAX_NOTICE_ROWS);
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(u16::try_from(notice).unwrap_or(0)),
        Constraint::Length(hints::ROWS),
    ])
    .areas(area)
}

/// The block of a popup: its border in the theme's border colour, and the
/// theme's text and background under it, since `Clear` resets them.
pub fn popup_block(state: &State) -> Block<'static> {
    Block::bordered()
        .border_style(style::border(state.theme()))
        .style(style::base(state.theme()))
}

/// A `width` by `height` box centred in `area`, shrunk to fit.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let theme = app.theme();

    let block = Block::default()
        .title(" Settings ")
        .title_style(if is_active {
            theme.active_title()
        } else {
            theme.title()
        })
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(if is_active {
            theme.active_border()
        } else {
            theme.inactive_border()
        })
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mode_label = app.input_mode.label();
    let mode_item = ListItem::new(Line::from(vec![
        Span::styled("Mode  ", theme.muted_text()),
        Span::styled(mode_label, theme.key_hint()),
    ]));

    let theme_name = &app.themes[app.theme_idx].name;
    let theme_item = ListItem::new(Line::from(vec![
        Span::styled("Theme ", theme.muted_text()),
        Span::styled(theme_name, theme.key_hint()),
    ]));

    let items = vec![mode_item, theme_item];

    let highlight_style = if is_active {
        theme.selected_item()
    } else {
        theme.subtle_text()
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default().with_selected(Some(app.settings_selection));
    frame.render_stateful_widget(list, inner, &mut state);
}

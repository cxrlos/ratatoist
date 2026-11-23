//! The Upcoming screen: open tasks due in the next seven days, grouped by
//! day under a header such as `Mon 23 Nov (3)`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding};

use super::text;
use super::views::tasks::task_line;
use crate::app::{Screen, State};

pub fn render(state: &State, frame: &mut Frame, area: Rect) {
    let theme = state.theme();
    let title = Screen::Upcoming.title();
    let block = Block::default()
        .title(format!(
            " {} ",
            text::fit(title, usize::from(area.width.saturating_sub(4)))
        ))
        .title_style(theme.active_title())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::horizontal(1))
        .style(theme.base_bg());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let tasks = state.visible_tasks();
    if tasks.is_empty() {
        let empty = Line::styled("Nothing is due in the next seven days.", theme.muted_text());
        text::draw(frame, empty, inner);
        return;
    }

    let room = usize::from(inner.width);
    let mut items = Vec::new();
    let mut selected = None;
    let mut day = None;
    for (index, task) in tasks.iter().enumerate() {
        let due = State::due_day(task);
        if index == 0 || due != day {
            day = due;
            let count = tasks.iter().filter(|t| State::due_day(t) == due).count();
            let label = due.map_or_else(
                || "No date".to_owned(),
                |date| date.format("%a %-d %b").to_string(),
            );
            let header = Line::styled(
                format!("{label} ({count})"),
                theme.muted_text().add_modifier(Modifier::BOLD),
            );
            items.push(ListItem::new(text::fit_line(header, room)));
        }
        if index == state.selected_task {
            selected = Some(items.len());
        }
        let row = task_line(task, state, theme, state.icons(), true);
        items.push(ListItem::new(text::fit_line(row, room)));
    }

    let list = List::new(items).highlight_style(theme.selected_item());
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, inner, &mut list_state);
}

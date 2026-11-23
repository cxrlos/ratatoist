use ratatui::layout::{Constraint, Flex, Layout, Rect};

pub fn centered_fixed_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let [_, v, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .areas(area);

    let [h] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(v);

    h
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, v, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(area);

    let [h] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(v);

    h
}

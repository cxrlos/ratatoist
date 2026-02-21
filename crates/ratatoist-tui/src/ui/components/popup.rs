use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

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

pub fn render_dim_overlay(frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(DimOverlay, area);
}

struct DimOverlay;

impl Widget for DimOverlay {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let dim = Style::default()
            .fg(Color::Rgb(60, 55, 80))
            .bg(Color::Rgb(15, 14, 22));

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(dim);
                }
            }
        }
    }
}

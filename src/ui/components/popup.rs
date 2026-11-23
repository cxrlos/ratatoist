use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

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

pub fn render_dim_overlay(frame: &mut Frame, theme: &Theme) {
    let area = frame.area();
    frame.render_widget(
        DimOverlay {
            dim: theme.muted_text().patch(theme.base_bg()),
        },
        area,
    );
}

struct DimOverlay {
    dim: Style,
}

impl Widget for DimOverlay {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let dim = self.dim;
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(dim);
                }
            }
        }
    }
}

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::InputMode;
use crate::ui::theme::Theme;

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, mode: &InputMode) {
    render_dim_overlay(frame);

    let area = frame.area();
    let popup = centered_rect(55, 70, area);

    let block = Block::default()
        .title(" Keybindings ")
        .title_style(Theme::active_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Theme::active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(Theme::base_bg());

    let lines = match mode {
        InputMode::Vim(_) => vim_bindings(),
        InputMode::Standard => standard_bindings(),
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, popup);
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(title.to_string(), Theme::active_title()))
}

fn binding(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<16}"), Theme::key_hint()),
        Span::styled(desc.to_string(), Theme::normal_text()),
    ])
}

fn blank() -> Line<'static> {
    Line::default()
}

fn vim_bindings() -> Vec<Line<'static>> {
    vec![
        section("Navigation"),
        binding("j / k", "Move down / up"),
        binding("h / l", "Switch pane left / right"),
        binding("g / G", "Jump to top / bottom"),
        binding("Tab / Shift-Tab", "Next / previous pane"),
        binding("Enter", "Open project / toggle fold"),
        binding("Esc", "Go back"),
        blank(),
        section("Tasks"),
        binding("x", "Complete / uncomplete"),
        binding("a", "Add task (quick-add)"),
        binding("o", "Cycle sort mode"),
        binding("Enter", "Open detail / toggle fold"),
        binding("Space", "Toggle fold"),
        blank(),
        section("Detail pane"),
        binding("j / k", "Navigate fields"),
        binding("i / Enter", "Edit selected field"),
        binding("c", "Add comment"),
        binding("x", "Complete task"),
        binding("Esc / h", "Back to tasks"),
        blank(),
        section("Projects"),
        binding("s", "Star / unstar"),
        blank(),
        section("Folding"),
        binding("za", "Toggle fold at cursor"),
        binding("zR", "Open all folds"),
        binding("zM", "Close all folds"),
        blank(),
        section("General"),
        binding(",", "Open settings"),
        binding("?", "This help"),
        binding("q", "Quit"),
        binding("Ctrl-c", "Force quit"),
        blank(),
        Line::from(Span::styled("press ? or Esc to close", Theme::muted_text()))
            .alignment(Alignment::Center),
    ]
}

fn standard_bindings() -> Vec<Line<'static>> {
    vec![
        section("Navigation"),
        binding("↑ / ↓", "Move up / down"),
        binding("← / →", "Switch pane"),
        binding("Home / End", "Jump to top / bottom"),
        binding("Tab / Shift-Tab", "Next / previous pane"),
        binding("Enter", "Open detail / toggle fold"),
        binding("Esc", "Go back"),
        blank(),
        section("Tasks"),
        binding("Ctrl-x", "Complete / uncomplete"),
        binding("Ctrl-a", "Add task (quick-add)"),
        blank(),
        section("Detail pane"),
        binding("↑ / ↓", "Navigate fields"),
        binding("Enter", "Edit selected field"),
        blank(),
        section("General"),
        binding(",", "Open settings"),
        binding("?", "This help"),
        binding("q", "Quit"),
        binding("Ctrl-c", "Force quit"),
        blank(),
        Line::from(Span::styled("press ? or Esc to close", Theme::muted_text()))
            .alignment(Alignment::Center),
    ]
}

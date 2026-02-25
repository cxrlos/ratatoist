use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::InputMode;
use crate::ui::theme::Theme;

use super::popup::{centered_rect, render_dim_overlay};

pub fn render(frame: &mut Frame, mode: &InputMode, theme: &Theme) {
    render_dim_overlay(frame, theme);

    let area = frame.area();
    let popup = centered_rect(55, 70, area);

    let block = Block::default()
        .title(" Keybindings ")
        .title_style(theme.active_title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.active_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base_bg());

    let lines = match mode {
        InputMode::Vim(_) => vim_bindings(theme),
        InputMode::Standard => standard_bindings(theme),
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, popup);
}

fn section(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(title.to_string(), theme.active_title()))
}

fn binding(key: &str, desc: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<16}"), theme.key_hint()),
        Span::styled(desc.to_string(), theme.normal_text()),
    ])
}

fn blank() -> Line<'static> {
    Line::default()
}

fn vim_bindings(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section("Navigation", theme),
        binding("j / k", "Move down / up", theme),
        binding("h / l", "Switch pane left / right", theme),
        binding("g / G", "Jump to top / bottom", theme),
        binding("Tab / Shift-Tab", "Next / previous pane", theme),
        binding("Enter", "Open project / toggle fold", theme),
        binding("Esc", "Go back", theme),
        blank(),
        section("Tasks", theme),
        binding("x", "Complete / uncomplete", theme),
        binding("a", "Add task (quick-add)", theme),
        binding("o", "Cycle sort mode", theme),
        binding("Enter", "Open detail / toggle fold", theme),
        binding("Space", "Toggle fold", theme),
        blank(),
        section("Detail pane", theme),
        binding("j / k", "Navigate fields", theme),
        binding("i / Enter", "Edit selected field", theme),
        binding("c", "Add comment", theme),
        binding("x", "Complete task", theme),
        binding("Esc / h", "Back to tasks", theme),
        blank(),
        section("Projects", theme),
        binding("s", "Star / unstar", theme),
        blank(),
        section("Folding", theme),
        binding("za", "Toggle fold at cursor", theme),
        binding("zR", "Open all folds", theme),
        binding("zM", "Close all folds", theme),
        blank(),
        section("General", theme),
        binding(",", "Open settings", theme),
        binding("?", "This help", theme),
        binding("q", "Quit", theme),
        binding("Ctrl-c", "Force quit", theme),
        blank(),
        Line::from(Span::styled("press ? or Esc to close", theme.muted_text()))
            .alignment(Alignment::Center),
    ]
}

fn standard_bindings(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        section("Navigation", theme),
        binding("↑ / ↓", "Move up / down", theme),
        binding("← / →", "Switch pane", theme),
        binding("Home / End", "Jump to top / bottom", theme),
        binding("Tab / Shift-Tab", "Next / previous pane", theme),
        binding("Enter", "Open detail / toggle fold", theme),
        binding("Esc", "Go back", theme),
        blank(),
        section("Tasks", theme),
        binding("Ctrl-x", "Complete / uncomplete", theme),
        binding("Ctrl-a", "Add task (quick-add)", theme),
        blank(),
        section("Detail pane", theme),
        binding("↑ / ↓", "Navigate fields", theme),
        binding("Enter", "Edit selected field", theme),
        blank(),
        section("General", theme),
        binding(",", "Open settings", theme),
        binding("?", "This help", theme),
        binding("q", "Quit", theme),
        binding("Ctrl-c", "Force quit", theme),
        blank(),
        Line::from(Span::styled("press ? or Esc to close", theme.muted_text()))
            .alignment(Alignment::Center),
    ]
}

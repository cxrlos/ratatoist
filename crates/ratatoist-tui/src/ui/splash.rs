use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::ui::theme::Theme;

const LOGO: &str = r#"
                $$\                $$\               $$\             $$\
                $$ |               $$ |              \__|            $$ |
 $$$$$$\  $$$$$$\ $$$$$$\    $$$$$$\ $$$$$$\    $$$$$$\  $$\  $$$$$$$\ $$$$$$\
$$  __$$\ \____$$\\_$$  _|   \____$$\\_$$  _|  $$  __$$\ $$ |$$  _____|\_$$  _|
$$ |  \__|$$$$$$$ | $$ |     $$$$$$$ | $$ |    $$ /  $$ |$$ |\$$$$$$\    $$ |
$$ |     $$  __$$ | $$ |$$\ $$  __$$ | $$ |$$\ $$ |  $$ |$$ | \____$$\   $$ |$$\
$$ |     \$$$$$$$ | \$$$$  |\$$$$$$$ | \$$$$  |\$$$$$$  |$$ |$$$$$$$  |  \$$$$  |
\__|      \_______|  \____/  \_______|  \____/  \______/ \__|\_______/    \____/
"#;

pub fn render(frame: &mut Frame, progress: f64, status: &str, theme: &Theme) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme.base_bg()), area);

    let logo_lines: Vec<&str> = LOGO.lines().filter(|l| !l.is_empty()).collect();
    let logo_height = logo_lines.len() as u16;

    let [_, logo_area, _, bar_area, status_area, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(logo_height),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);

    let logo_text: Vec<Line> = logo_lines
        .iter()
        .map(|line| Line::from(Span::styled((*line).to_string(), theme.subtle_text())))
        .collect();

    let logo = Paragraph::new(logo_text).alignment(Alignment::Center);
    frame.render_widget(logo, logo_area);

    render_progress_bar(frame, bar_area, progress, theme);

    let status_line = Paragraph::new(Line::from(Span::styled(status, theme.muted_text())))
        .alignment(Alignment::Center);
    frame.render_widget(status_line, status_area);
}

fn render_progress_bar(frame: &mut Frame, area: Rect, progress: f64, theme: &Theme) {
    let gradient = [
        theme.muted,
        theme.red,
        theme.orange,
        theme.yellow,
        theme.green,
        theme.cyan,
        theme.blue,
        theme.purple,
        theme.text,
    ];

    let bar_width = (area.width as usize).min(60);
    let padding = (area.width as usize).saturating_sub(bar_width) / 2;
    let filled = ((bar_width as f64) * progress.clamp(0.0, 1.0)) as usize;

    let unfilled_color = theme.overlay;

    let mut spans = Vec::new();
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }

    for i in 0..bar_width {
        let gradient_pos = (i as f64 / bar_width as f64 * (gradient.len() - 1) as f64) as usize;
        let color: Color = gradient[gradient_pos.min(gradient.len() - 1)];

        if i < filled {
            spans.push(Span::styled("━", Style::default().fg(color)));
        } else {
            spans.push(Span::styled("━", Style::default().fg(unfilled_color)));
        }
    }

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

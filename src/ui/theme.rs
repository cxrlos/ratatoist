use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

const BASE: Color = Color::Rgb(25, 23, 36);
const SURFACE: Color = Color::Rgb(31, 29, 46);
const OVERLAY: Color = Color::Rgb(38, 35, 58);
const MUTED: Color = Color::Rgb(110, 106, 134);
const SUBTLE: Color = Color::Rgb(144, 140, 170);
const TEXT: Color = Color::Rgb(224, 222, 244);
const LOVE: Color = Color::Rgb(235, 111, 146);
const GOLD: Color = Color::Rgb(246, 193, 119);
const ROSE: Color = Color::Rgb(235, 188, 186);
#[allow(dead_code)]
const PINE: Color = Color::Rgb(49, 116, 143);
const FOAM: Color = Color::Rgb(156, 207, 216);
const IRIS: Color = Color::Rgb(196, 167, 231);

impl Theme {
    pub fn base_bg() -> Style {
        Style::default().bg(BASE)
    }

    pub fn active_border() -> Style {
        Style::default().fg(FOAM)
    }

    pub fn inactive_border() -> Style {
        Style::default().fg(OVERLAY)
    }

    pub fn selected_item() -> Style {
        Style::default().fg(FOAM).bg(SURFACE)
    }

    pub fn normal_text() -> Style {
        Style::default().fg(TEXT)
    }

    pub fn muted_text() -> Style {
        Style::default().fg(MUTED)
    }

    pub fn subtle_text() -> Style {
        Style::default().fg(SUBTLE)
    }

    pub fn title() -> Style {
        Style::default().fg(IRIS).add_modifier(Modifier::BOLD)
    }

    pub fn active_title() -> Style {
        Style::default().fg(FOAM).add_modifier(Modifier::BOLD)
    }

    pub fn key_hint() -> Style {
        Style::default().fg(FOAM)
    }

    #[allow(dead_code)]
    pub fn success() -> Style {
        Style::default().fg(PINE)
    }

    pub fn inbox_icon() -> Style {
        Style::default().fg(IRIS)
    }

    pub fn favorite_icon() -> Style {
        Style::default().fg(GOLD)
    }

    pub fn priority_style(priority: u8) -> Style {
        let color = match priority {
            4 => LOVE,
            3 => GOLD,
            2 => ROSE,
            _ => MUTED,
        };
        Style::default().fg(color)
    }

    pub fn priority_dot(priority: u8) -> &'static str {
        match priority {
            4 => "● ",
            3 => "● ",
            2 => "● ",
            _ => "  ",
        }
    }
}

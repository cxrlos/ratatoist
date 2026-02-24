use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Base16Scheme {
    pub name: String,
    #[serde(rename = "base00")]
    base00: String,
    #[serde(rename = "base01")]
    base01: String,
    #[serde(rename = "base02")]
    base02: String,
    #[serde(rename = "base03")]
    base03: String,
    #[serde(rename = "base04")]
    base04: String,
    #[serde(rename = "base05")]
    base05: String,
    #[serde(rename = "base06")]
    base06: String,
    #[serde(rename = "base07")]
    base07: String,
    #[serde(rename = "base08")]
    base08: String,
    #[serde(rename = "base09")]
    base09: String,
    #[serde(rename = "base0A")]
    base0a: String,
    #[serde(rename = "base0B")]
    base0b: String,
    #[serde(rename = "base0C")]
    base0c: String,
    #[serde(rename = "base0D")]
    base0d: String,
    #[serde(rename = "base0E")]
    base0e: String,
    #[serde(rename = "base0F")]
    base0f: String,
}

#[allow(dead_code)]
pub struct Theme {
    pub name: String,
    pub base: Color,
    pub surface: Color,
    pub overlay: Color,
    pub muted: Color,
    pub subtle: Color,
    pub text: Color,
    pub bg_alt: Color,
    pub fg_alt: Color,
    pub red: Color,
    pub orange: Color,
    pub yellow: Color,
    pub green: Color,
    pub cyan: Color,
    pub blue: Color,
    pub purple: Color,
    pub maroon: Color,
}

fn parse_hex(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    Color::Rgb(r, g, b)
}

impl Theme {
    pub fn from_scheme(s: &Base16Scheme) -> Self {
        Self {
            name: s.name.clone(),
            base: parse_hex(&s.base00),
            surface: parse_hex(&s.base01),
            overlay: parse_hex(&s.base02),
            muted: parse_hex(&s.base03),
            subtle: parse_hex(&s.base04),
            text: parse_hex(&s.base05),
            bg_alt: parse_hex(&s.base06),
            fg_alt: parse_hex(&s.base07),
            red: parse_hex(&s.base08),
            orange: parse_hex(&s.base09),
            yellow: parse_hex(&s.base0a),
            green: parse_hex(&s.base0b),
            cyan: parse_hex(&s.base0c),
            blue: parse_hex(&s.base0d),
            purple: parse_hex(&s.base0e),
            maroon: parse_hex(&s.base0f),
        }
    }

    pub fn builtin() -> Vec<Self> {
        [
            include_str!("../../themes/rose-pine.json"),
            include_str!("../../themes/gruvbox-dark.json"),
            include_str!("../../themes/dracula.json"),
            include_str!("../../themes/nord.json"),
            include_str!("../../themes/one-dark.json"),
            include_str!("../../themes/solarized-dark.json"),
            include_str!("../../themes/catppuccin-mocha.json"),
            include_str!("../../themes/tokyo-night.json"),
            include_str!("../../themes/monokai.json"),
            include_str!("../../themes/material-dark.json"),
        ]
        .iter()
        .filter_map(|src| serde_json::from_str::<Base16Scheme>(src).ok())
        .map(|s| Self::from_scheme(&s))
        .collect()
    }

    pub fn load_user_themes(dir: &std::path::Path) -> Vec<Self> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut themes = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(src) = std::fs::read_to_string(&path)
                && let Ok(scheme) = serde_json::from_str::<Base16Scheme>(&src)
            {
                themes.push(Self::from_scheme(&scheme));
            }
        }
        themes.sort_by(|a, b| a.name.cmp(&b.name));
        themes
    }

    pub fn base_bg(&self) -> Style {
        Style::default().bg(self.base)
    }

    pub fn surface_bg(&self) -> Style {
        Style::default().bg(self.surface)
    }

    pub fn active_border(&self) -> Style {
        Style::default().fg(self.cyan)
    }

    pub fn inactive_border(&self) -> Style {
        Style::default().fg(self.overlay)
    }

    pub fn selected_item(&self) -> Style {
        Style::default().fg(self.cyan).bg(self.surface)
    }

    pub fn dock_focused_item(&self) -> Style {
        Style::default()
            .fg(self.base)
            .bg(self.cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal_text(&self) -> Style {
        Style::default().fg(self.text)
    }

    pub fn muted_text(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn subtle_text(&self) -> Style {
        Style::default().fg(self.subtle)
    }

    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.purple)
            .add_modifier(Modifier::BOLD)
    }

    pub fn active_title(&self) -> Style {
        Style::default().fg(self.cyan).add_modifier(Modifier::BOLD)
    }

    pub fn key_hint(&self) -> Style {
        Style::default().fg(self.cyan)
    }

    pub fn success(&self) -> Style {
        Style::default().fg(self.green)
    }

    pub fn inbox_icon(&self) -> Style {
        Style::default().fg(self.purple)
    }

    pub fn favorite_icon(&self) -> Style {
        Style::default().fg(self.orange)
    }

    pub fn label_tag(&self) -> Style {
        Style::default().fg(self.purple)
    }

    pub fn error_title(&self) -> Style {
        Style::default().fg(self.red).add_modifier(Modifier::BOLD)
    }

    pub fn error_border(&self) -> Style {
        Style::default().fg(self.red)
    }

    pub fn due_today(&self) -> Style {
        Style::default().fg(self.orange)
    }

    pub fn due_overdue(&self) -> Style {
        Style::default().fg(self.red)
    }

    pub fn due_upcoming(&self) -> Style {
        Style::default().fg(self.cyan)
    }

    pub fn due_future(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn mode_normal(&self) -> Style {
        Style::default()
            .fg(self.base)
            .bg(self.cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn mode_visual(&self) -> Style {
        Style::default()
            .fg(self.base)
            .bg(self.purple)
            .add_modifier(Modifier::BOLD)
    }

    pub fn mode_insert(&self) -> Style {
        Style::default()
            .fg(self.base)
            .bg(self.orange)
            .add_modifier(Modifier::BOLD)
    }

    pub fn mode_standard(&self) -> Style {
        Style::default()
            .fg(self.base)
            .bg(self.green)
            .add_modifier(Modifier::BOLD)
    }

    pub fn priority_style(&self, priority: u8) -> Style {
        let color = match priority {
            4 => self.red,
            3 => self.orange,
            2 => self.yellow,
            _ => self.muted,
        };
        Style::default().fg(color)
    }

    pub fn priority_dot(priority: u8) -> &'static str {
        match priority {
            2..=4 => "● ",
            _ => "  ",
        }
    }

    pub fn dim_overlay(&self) -> (Color, Color) {
        let bg = match self.base {
            Color::Rgb(r, g, b) => Color::Rgb(r / 2, g / 2, b / 2),
            c => c,
        };
        (self.muted, bg)
    }

    pub fn user_colors(&self) -> [Color; 6] {
        [
            self.cyan,
            self.purple,
            self.maroon,
            self.orange,
            self.green,
            self.red,
        ]
    }
}

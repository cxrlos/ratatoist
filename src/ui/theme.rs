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

#[derive(Debug, Clone)]
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

/// `#rgb` or `#rrggbb`, in either case; `#rgb` doubles each digit. The `#`
/// is optional, as Base16 files (the built-ins too) leave it out.
fn parse_hex(hex: &str) -> Result<Color, String> {
    let digits: Option<Vec<u8>> = hex
        .strip_prefix('#')
        .unwrap_or(hex)
        .chars()
        .map(|c| c.to_digit(16).and_then(|d| u8::try_from(d).ok()))
        .collect();
    match digits.as_deref() {
        Some(&[r, g, b]) => Ok(Color::Rgb(r * 17, g * 17, b * 17)),
        Some(&[r1, r0, g1, g0, b1, b0]) => Ok(Color::Rgb(r1 * 16 + r0, g1 * 16 + g0, b1 * 16 + b0)),
        _ => Err(format!("`{hex}` is not a #rgb or #rrggbb colour")),
    }
}

/// Maps a Todoist color name to a semantic slot in the active theme.
pub fn color_scheme(name: &str) -> &'static str {
    match name {
        "berry_red" | "red" | "salmon" => "red",
        "orange" | "yellow" => "orange",
        "olive_green" | "lime_green" | "green" | "mint_green" => "green",
        "teal" | "sky_blue" | "light_blue" => "cyan",
        "blue" => "blue",
        "grape" | "violet" | "lavender" | "magenta" => "purple",
        "charcoal" | "grey" | "taupe" => "muted",
        _ => "subtle",
    }
}

impl Theme {
    /// The theme a Base16 scheme describes, or which key holds a colour
    /// that is not `#rgb` or `#rrggbb`.
    pub fn from_scheme(s: &Base16Scheme) -> Result<Self, String> {
        let colour = |key: &str, value: &str| {
            parse_hex(value).map_err(|problem| format!("{key}: {problem}"))
        };
        Ok(Self {
            name: s.name.clone(),
            base: colour("base00", &s.base00)?,
            surface: colour("base01", &s.base01)?,
            overlay: colour("base02", &s.base02)?,
            muted: colour("base03", &s.base03)?,
            subtle: colour("base04", &s.base04)?,
            text: colour("base05", &s.base05)?,
            bg_alt: colour("base06", &s.base06)?,
            fg_alt: colour("base07", &s.base07)?,
            red: colour("base08", &s.base08)?,
            orange: colour("base09", &s.base09)?,
            yellow: colour("base0A", &s.base0a)?,
            green: colour("base0B", &s.base0b)?,
            cyan: colour("base0C", &s.base0c)?,
            blue: colour("base0D", &s.base0d)?,
            purple: colour("base0E", &s.base0e)?,
            maroon: colour("base0F", &s.base0f)?,
        })
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
        .filter_map(|s| Self::from_scheme(&s).ok())
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
                && let Ok(theme) = Self::from_scheme(&scheme)
            {
                themes.push(theme);
            }
        }
        themes.sort_by(|a, b| a.name.cmp(&b.name));
        themes
    }

    /// Map a Todoist color name to the closest `Color` in this theme.
    pub fn color_for(&self, todoist_color: &str) -> Color {
        match color_scheme(todoist_color) {
            "red" => self.red,
            "orange" => self.orange,
            "green" => self.green,
            "cyan" => self.cyan,
            "blue" => self.blue,
            "purple" => self.purple,
            "muted" => self.muted,
            _ => self.subtle,
        }
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

//! Themes: the colours of every role ratatoist draws with.
//!
//! The default theme, `terminal`, uses the terminal's own palette and paints
//! no background. Every other built-in theme is a Base16 scheme and paints
//! its `base00` behind the whole screen, because its colours are only
//! legible against that background. A user's own theme is a Base16 JSON
//! file in `~/.config/ratatoist/themes/`.
//!
//! Base16 slots map to roles as follows: `base00` background, `base05`
//! text, `base03` muted for borders and inactive things (lifted toward
//! `base05` until it reaches [`MUTED_MIN_CONTRAST`]), `base0C` accent,
//! `base08` error. Priorities: p1 `base08`, p2 `base09`, p3 `base0D`, p4
//! muted, as Todoist's red, orange, blue and none. Due dates: overdue error,
//! today accent, the next six days text, later muted. Deadlines: p2, and
//! error once their day has passed. Todoist's project
//! colour names map to `base08`, `base09`, `base0B` to `base0E`, for project
//! dots only.
//!
//! `NO_COLOR` replaces whichever theme is configured with
//! [`Theme::no_color`]; it is read once at startup, never while drawing.

use std::ffi::OsStr;
use std::fmt;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

use crate::todoist::model::Priority;

/// The name of the default theme.
pub const DEFAULT: &str = "terminal";

/// The environment variable that turns colour off, per no-color.org.
pub const NO_COLOR_VAR: &str = "NO_COLOR";

/// Whether `NO_COLOR` with `value` asks for no colour: it is set and not
/// empty. An empty value counts as unset.
pub fn no_color_requested(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// A theme name as `[theme] name` takes it: lower case, with hyphens for
/// spaces and underscores, so `rose-pine` finds "Rose Pine".
pub fn slug(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '_'], "-")
}

/// The least contrast ratio the muted role keeps against the background in
/// a Base16 theme, the WCAG minimum for large text and interface parts.
pub const MUTED_MIN_CONTRAST: f64 = 3.0;

/// The built-in Base16 schemes, by the name `[theme] name` takes.
const BASE16: [(&str, &str); 10] = [
    (
        "catppuccin-mocha",
        include_str!("themes/catppuccin-mocha.json"),
    ),
    ("dracula", include_str!("themes/dracula.json")),
    ("gruvbox-dark", include_str!("themes/gruvbox-dark.json")),
    ("material-dark", include_str!("themes/material-dark.json")),
    ("monokai", include_str!("themes/monokai.json")),
    ("nord", include_str!("themes/nord.json")),
    ("one-dark", include_str!("themes/one-dark.json")),
    ("rose-pine", include_str!("themes/rose-pine.json")),
    ("solarized-dark", include_str!("themes/solarized-dark.json")),
    ("tokyo-night", include_str!("themes/tokyo-night.json")),
];

/// The colour of each role ratatoist draws with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The name the theme picker shows.
    pub name: String,
    /// Painted behind everything; [`Color::Reset`] keeps the terminal's own.
    pub background: Color,
    /// Everything that has no other role.
    pub text: Color,
    /// What is inactive or secondary: unavailable keys, the pane without
    /// focus, labels and field names.
    pub muted: Color,
    /// What is active: keys the user can use now, the focused pane.
    pub accent: Color,
    /// Messages about something that went wrong.
    pub error: Color,
    /// Borders of a pane alone on screen or of a popup.
    pub border: Color,
    /// The focused row. Reversing keeps each cell's own contrast.
    pub selection: Style,
    /// The glyph and label of each priority.
    pub priorities: PriorityColors,
    /// A due date, by how near it is.
    pub due: DueColors,
    /// The dot of a project, by its Todoist colour.
    pub projects: ProjectColors,
}

/// The colour of each priority, p1 the most urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityColors {
    pub p1: Color,
    pub p2: Color,
    pub p3: Color,
    pub p4: Color,
}

/// The colour of a due date in each [`crate::ui::dates::DueBucket`] that has
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueColors {
    pub overdue: Color,
    pub today: Color,
    /// The six days after today.
    pub this_week: Color,
    pub later: Color,
}

/// The colours Todoist's project colour names map to. Greys map to muted
/// and anything unknown to text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectColors {
    pub red: Color,
    pub orange: Color,
    pub green: Color,
    pub cyan: Color,
    pub blue: Color,
    pub purple: Color,
}

impl Default for Theme {
    /// The `terminal` theme: the terminal's own palette, cyan accent and dark
    /// grey muted, on the terminal's background.
    fn default() -> Self {
        Self {
            name: "Terminal".to_owned(),
            background: Color::Reset,
            text: Color::Reset,
            muted: Color::DarkGray,
            accent: Color::Cyan,
            error: Color::Red,
            border: Color::Reset,
            selection: Style::new().add_modifier(Modifier::REVERSED),
            priorities: PriorityColors {
                p1: Color::Red,
                p2: Color::LightRed,
                p3: Color::Blue,
                p4: Color::DarkGray,
            },
            due: DueColors {
                overdue: Color::Red,
                today: Color::Cyan,
                this_week: Color::Reset,
                later: Color::DarkGray,
            },
            projects: ProjectColors {
                red: Color::Red,
                orange: Color::LightRed,
                green: Color::Green,
                cyan: Color::Cyan,
                blue: Color::Blue,
                purple: Color::Magenta,
            },
        }
    }
}

impl Theme {
    /// The theme `NO_COLOR` asks for: every role the terminal's default
    /// foreground on its default background. Modifiers still mark the
    /// focused row (reversed) and what the user can act on (bold).
    pub fn no_color() -> Self {
        Self {
            name: "No colour".to_owned(),
            background: Color::Reset,
            text: Color::Reset,
            muted: Color::Reset,
            accent: Color::Reset,
            error: Color::Reset,
            border: Color::Reset,
            selection: Style::new().add_modifier(Modifier::REVERSED),
            priorities: PriorityColors {
                p1: Color::Reset,
                p2: Color::Reset,
                p3: Color::Reset,
                p4: Color::Reset,
            },
            due: DueColors {
                overdue: Color::Reset,
                today: Color::Reset,
                this_week: Color::Reset,
                later: Color::Reset,
            },
            projects: ProjectColors {
                red: Color::Reset,
                orange: Color::Reset,
                green: Color::Reset,
                cyan: Color::Reset,
                blue: Color::Reset,
                purple: Color::Reset,
            },
        }
    }

    /// The names of the built-in themes, the default first.
    pub fn built_in_names() -> impl Iterator<Item = &'static str> {
        std::iter::once(DEFAULT).chain(BASE16.iter().map(|(name, _)| *name))
    }

    /// The built-in theme called `name`, if there is one.
    pub fn built_in(name: &str) -> Option<Self> {
        if name == DEFAULT {
            return Some(Self::default());
        }
        let (_, json) = BASE16.iter().find(|(built_in, _)| *built_in == name)?;
        Self::from_base16_json(json).ok()
    }

    /// Every built-in theme, the default first.
    pub fn built_ins() -> Vec<Self> {
        Self::built_in_names().filter_map(Self::built_in).collect()
    }

    /// A theme from a Base16 scheme in JSON: `name`, and `base00` to
    /// `base0F` as `#rgb` or `#rrggbb` colours, the `#` optional.
    pub fn from_base16_json(json: &str) -> Result<Self, Base16Error> {
        let scheme: Base16 = serde_json::from_str(json).map_err(|error| {
            let text = error.to_string();
            // serde_json appends the position to every message.
            let message = match text.rsplit_once(" at line ") {
                Some((message, _)) => message.to_owned(),
                None => text,
            };
            Base16Error {
                at: Some((error.line(), error.column())),
                message,
            }
        })?;
        Self::from_base16(&scheme)
    }

    fn from_base16(s: &Base16) -> Result<Self, Base16Error> {
        let [
            base00,
            _base01,
            _base02,
            base03,
            _base04,
            base05,
            _base06,
            _base07,
            base08,
            base09,
            _base0a,
            base0b,
            base0c,
            base0d,
            base0e,
            _base0f,
        ] = s.colours()?;
        let background = base00;
        let text = base05;
        let muted = legible(base03, text, background);
        let accent = base0c;
        let error = base08;
        Ok(Self {
            name: s.name.clone(),
            background,
            text,
            muted,
            accent,
            error,
            border: muted,
            selection: Style::new().add_modifier(Modifier::REVERSED),
            priorities: PriorityColors {
                p1: base08,
                p2: base09,
                p3: base0d,
                p4: muted,
            },
            due: DueColors {
                overdue: error,
                today: accent,
                this_week: text,
                later: muted,
            },
            projects: ProjectColors {
                red: base08,
                orange: base09,
                green: base0b,
                cyan: base0c,
                blue: base0d,
                purple: base0e,
            },
        })
    }

    /// The colour of `priority`.
    pub fn priority(&self, priority: Priority) -> Color {
        let priorities = &self.priorities;
        match priority {
            Priority::P1 => priorities.p1,
            Priority::P2 => priorities.p2,
            Priority::P3 => priorities.p3,
            Priority::P4 => priorities.p4,
        }
    }

    /// The colour of a project dot whose Todoist colour is `todoist_color`.
    pub fn color_for(&self, todoist_color: &str) -> Color {
        let projects = &self.projects;
        match todoist_color {
            "berry_red" | "red" | "salmon" => projects.red,
            "orange" | "yellow" => projects.orange,
            "olive_green" | "lime_green" | "green" | "mint_green" => projects.green,
            "teal" | "sky_blue" | "light_blue" => projects.cyan,
            "blue" => projects.blue,
            "grape" | "violet" | "lavender" | "magenta" => projects.purple,
            "charcoal" | "grey" | "taupe" => self.muted,
            _ => self.text,
        }
    }

    /// The dot of a project whose Todoist colour is `todoist_color`.
    pub fn project_dot(&self, todoist_color: &str) -> Style {
        Style::new().fg(self.color_for(todoist_color))
    }

    /// A few of the theme's colours, for the theme picker's swatch.
    pub fn swatch(&self) -> [Color; 5] {
        [
            self.accent,
            self.priorities.p1,
            self.priorities.p2,
            self.priorities.p3,
            self.muted,
        ]
    }

    pub fn base_bg(&self) -> Style {
        Style::new().bg(self.background)
    }

    pub fn active_border(&self) -> Style {
        Style::new().fg(self.accent)
    }

    pub fn inactive_border(&self) -> Style {
        Style::new().fg(self.border)
    }

    /// The focused row of the focused pane.
    pub fn selected_item(&self) -> Style {
        self.selection
    }

    pub fn normal_text(&self) -> Style {
        Style::new().fg(self.text)
    }

    pub fn muted_text(&self) -> Style {
        Style::new().fg(self.muted)
    }

    /// Secondary text that still reads as content: headings, breadcrumbs.
    pub fn subtle_text(&self) -> Style {
        self.normal_text()
    }

    /// The focused row of a pane without focus: bold, not reversed.
    pub fn unfocused_selection(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    /// The title of a pane without focus.
    pub fn title(&self) -> Style {
        self.muted_text()
    }

    pub fn active_title(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn key_hint(&self) -> Style {
        self.active_title()
    }

    /// Something that went right: the live connection, a completed task.
    pub fn success(&self) -> Style {
        self.normal_text()
    }

    pub fn inbox_icon(&self) -> Style {
        self.normal_text()
    }

    pub fn favorite_icon(&self) -> Style {
        self.normal_text()
    }

    pub fn label_tag(&self) -> Style {
        self.muted_text()
    }

    pub fn error_title(&self) -> Style {
        Style::new().fg(self.error).add_modifier(Modifier::BOLD)
    }

    pub fn error_border(&self) -> Style {
        Style::new().fg(self.error)
    }

    pub fn due_today(&self) -> Style {
        Style::new().fg(self.due.today)
    }

    pub fn due_overdue(&self) -> Style {
        Style::new().fg(self.due.overdue)
    }

    pub fn due_upcoming(&self) -> Style {
        Style::new().fg(self.due.this_week)
    }

    pub fn due_future(&self) -> Style {
        Style::new().fg(self.due.later)
    }

    /// A task's deadline whose day has not passed.
    pub fn deadline(&self) -> Style {
        Style::new().fg(self.priorities.p2)
    }

    /// A task's deadline whose day has passed.
    pub fn deadline_missed(&self) -> Style {
        Style::new().fg(self.error)
    }

    pub fn mode_insert(&self) -> Style {
        self.selection.fg(self.text).add_modifier(Modifier::BOLD)
    }

    /// The colour of a priority, from the API's number.
    pub fn priority_style(&self, priority: u8) -> Style {
        Style::new().fg(self.priority(Priority::from_api(priority)))
    }
}

/// A Base16 scheme that could not be read: the line and column where the
/// JSON broke, or `None` for a colour that is not one, which the message
/// names by its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base16Error {
    pub at: Option<(usize, usize)>,
    pub message: String,
}

impl fmt::Display for Base16Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Base16Error {}

/// The WCAG 2 contrast ratio of two RGB colours, from 1 to 21. `None` for a
/// named or indexed colour, whose value only the terminal knows.
pub fn contrast_ratio(a: Color, b: Color) -> Option<f64> {
    let (a, b) = (luminance(a)?, luminance(b)?);
    let (light, dark) = if a > b { (a, b) } else { (b, a) };
    Some((light + 0.05) / (dark + 0.05))
}

fn luminance(color: Color) -> Option<f64> {
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
    let channel = |value: u8| {
        let c = f64::from(value) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b))
}

/// `color`, or the first mix of it toward `toward` that reaches
/// [`MUTED_MIN_CONTRAST`] against `background`. Most schemes put a `base03`
/// too dark to read on their own `base00`.
fn legible(color: Color, toward: Color, background: Color) -> Color {
    const STEPS: u16 = 16;
    (0..=STEPS)
        .map(|step| mix(color, toward, step, STEPS))
        .find(|&mixed| {
            contrast_ratio(mixed, background).is_some_and(|ratio| ratio >= MUTED_MIN_CONTRAST)
        })
        .unwrap_or(toward)
}

fn mix(from: Color, to: Color, step: u16, steps: u16) -> Color {
    let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (from, to) else {
        return from;
    };
    let channel = |a: u8, b: u8| {
        let (a, b) = (u16::from(a), u16::from(b));
        let mixed = (a * (steps - step) + b * step) / steps;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    Color::Rgb(channel(r1, r2), channel(g1, g2), channel(b1, b2))
}

#[derive(Deserialize)]
struct Base16 {
    name: String,
    base00: String,
    base01: String,
    base02: String,
    base03: String,
    base04: String,
    base05: String,
    base06: String,
    base07: String,
    base08: String,
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

impl Base16 {
    /// `base00` to `base0F`, each checked even where no role draws with it,
    /// so a broken file is reported whole.
    fn colours(&self) -> Result<[Color; 16], Base16Error> {
        let slots = [
            ("base00", &self.base00),
            ("base01", &self.base01),
            ("base02", &self.base02),
            ("base03", &self.base03),
            ("base04", &self.base04),
            ("base05", &self.base05),
            ("base06", &self.base06),
            ("base07", &self.base07),
            ("base08", &self.base08),
            ("base09", &self.base09),
            ("base0A", &self.base0a),
            ("base0B", &self.base0b),
            ("base0C", &self.base0c),
            ("base0D", &self.base0d),
            ("base0E", &self.base0e),
            ("base0F", &self.base0f),
        ];
        let mut colours = [Color::Reset; 16];
        for (colour, (key, value)) in colours.iter_mut().zip(slots) {
            *colour = parse_hex(value).ok_or_else(|| Base16Error {
                at: None,
                message: format!("{key}: `{value}` is not a #rgb or #rrggbb colour"),
            })?;
        }
        Ok(colours)
    }
}

/// `#rgb` or `#rrggbb`, in either case; `#rgb` doubles each digit. The `#`
/// is optional, as Base16 files (the built-ins too) leave it out.
fn parse_hex(hex: &str) -> Option<Color> {
    let digits: Option<Vec<u8>> = hex
        .strip_prefix('#')
        .unwrap_or(hex)
        .chars()
        .map(|c| c.to_digit(16).and_then(|d| u8::try_from(d).ok()))
        .collect();
    match digits?[..] {
        [r, g, b] => Some(Color::Rgb(r * 17, g * 17, b * 17)),
        [r1, r0, g1, g0, b1, b0] => Some(Color::Rgb(r1 * 16 + r0, g1 * 16 + g0, b1 * 16 + b0)),
        _ => None,
    }
}

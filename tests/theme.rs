// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use chrono::{Local, TimeZone};
use insta::assert_debug_snapshot;
use ratatoist::app::{App, Event, State};
use ratatoist::config::{self, Config};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::demo;
use ratatoist::ui::theme::{MUTED_MIN_CONTRAST, Theme, contrast_ratio};
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};

/// A Base16 scheme whose `base03` is far too dark on its `base00`.
const DIM: &str = r#"{
  "name": "Dim",
  "base00": "101010", "base01": "202020", "base02": "303030", "base03": "1c1c1c",
  "base04": "b0b0b0", "base05": "e0e0e0", "base06": "f0f0f0", "base07": "ffffff",
  "base08": "ff5555", "base09": "ffaa55", "base0A": "ffff55", "base0B": "55ff55",
  "base0C": "55ffff", "base0D": "5555ff", "base0E": "ff55ff", "base0F": "aa5500"
}"#;

/// The demo account's main screen at 80x24, drawn with `config`.
fn demo_with(config: &Config) -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let state = initial_state_from(config, &UiSettings::default(), "*".to_owned(), now, false);
    let mut app = App::new(TestBackend::new(80, 24), state).unwrap();
    let sync = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    app
}

fn named(theme: &str) -> Config {
    Config {
        theme: Some(theme.to_owned()),
        ..Config::default()
    }
}

#[test]
fn there_are_the_terminal_default_and_ten_base16_built_in_themes() {
    let names: Vec<&str> = Theme::built_in_names().collect();

    assert_eq!(
        names,
        [
            "terminal",
            "catppuccin-mocha",
            "dracula",
            "gruvbox-dark",
            "material-dark",
            "monokai",
            "nord",
            "one-dark",
            "rose-pine",
            "solarized-dark",
            "tokyo-night",
        ]
    );
    for name in names {
        assert!(
            Theme::built_in(name).is_some(),
            "a built-in theme does not load"
        );
    }
}

#[test]
fn a_new_state_draws_with_the_terminal_theme() {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();

    assert_eq!(*State::new(now).theme(), Theme::default());
}

#[test]
fn muted_meets_three_to_one_in_every_base16_built_in_after_the_lift() {
    for name in Theme::built_in_names().skip(1) {
        let theme = Theme::built_in(name).unwrap();
        let ratio = contrast_ratio(theme.muted, theme.background).unwrap();

        assert!(
            ratio >= MUTED_MIN_CONTRAST,
            "a built-in theme's muted text is below 3:1"
        );
        assert_eq!(
            theme.border, theme.muted,
            "a built-in theme's border is not muted"
        );
    }
}

#[test]
fn a_muted_colour_too_dark_for_its_background_is_lifted_toward_the_text() {
    let theme = Theme::from_base16_json(DIM).unwrap();

    let ratio = contrast_ratio(theme.muted, theme.background).unwrap();
    assert!(ratio >= MUTED_MIN_CONTRAST);
    assert_ne!(theme.muted, Color::Rgb(0x1c, 0x1c, 0x1c));
    assert!(
        contrast_ratio(theme.muted, theme.background)
            < contrast_ratio(theme.text, theme.background)
    );
}

#[test]
fn base16_slots_map_to_the_roles() {
    let theme = Theme::from_base16_json(DIM).unwrap();

    assert_eq!(theme.name, "Dim");
    assert_eq!(theme.background, Color::Rgb(0x10, 0x10, 0x10));
    assert_eq!(theme.text, Color::Rgb(0xe0, 0xe0, 0xe0));
    assert_eq!(theme.accent, Color::Rgb(0x55, 0xff, 0xff));
    assert_eq!(theme.error, Color::Rgb(0xff, 0x55, 0x55));
    assert_eq!(theme.selection.add_modifier, Modifier::REVERSED);
    let p = theme.priorities;
    assert_eq!(
        [p.p1, p.p2, p.p3, p.p4],
        [
            Color::Rgb(0xff, 0x55, 0x55),
            Color::Rgb(0xff, 0xaa, 0x55),
            Color::Rgb(0x55, 0x55, 0xff),
            theme.muted,
        ]
    );
    let due = theme.due;
    assert_eq!(
        [due.overdue, due.today, due.this_week, due.later],
        [theme.error, theme.accent, theme.text, theme.muted]
    );
}

#[test]
fn todoist_colour_names_map_to_theme_colours_for_project_dots() {
    let theme = Theme::from_base16_json(DIM).unwrap();

    assert_eq!(theme.color_for("berry_red"), Color::Rgb(0xff, 0x55, 0x55));
    assert_eq!(theme.color_for("blue"), Color::Rgb(0x55, 0x55, 0xff));
    assert_eq!(theme.color_for("grape"), Color::Rgb(0xff, 0x55, 0xff));
    assert_eq!(theme.color_for("charcoal"), theme.muted);
}

#[test]
fn a_user_theme_file_loads_and_can_be_chosen() {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("ratatoist-theme-user-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("themes")).unwrap();
    fs::write(dir.join("config.toml"), "[theme]\nname = \"dim\"\n").unwrap();
    fs::write(dir.join("themes/dim.json"), DIM).unwrap();

    let config = config::load(&dir);
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(config.warnings, Vec::<String>::new());
    assert_eq!(config.theme.as_deref(), Some("Dim"));
    let app = demo_with(&config);
    assert_eq!(app.state().theme().name, "Dim");
    assert_eq!(app.state().theme().background, Color::Rgb(0x10, 0x10, 0x10));
}

#[test]
fn contrast_ratio_matches_the_wcag_extremes() {
    let black = Color::Rgb(0, 0, 0);
    let white = Color::Rgb(255, 255, 255);

    assert_eq!(contrast_ratio(black, white), Some(21.0));
    assert_eq!(contrast_ratio(white, white), Some(1.0));
    assert_eq!(contrast_ratio(Color::Cyan, white), None);
}

#[test]
fn a_base16_theme_paints_its_background_and_the_terminal_theme_keeps_the_terminals() {
    let nord = demo_with(&named("nord"));
    let terminal = demo_with(&Config::default());

    for cell in nord.backend().buffer().content() {
        assert_eq!(cell.bg, Color::Rgb(0x2e, 0x34, 0x40));
    }
    for cell in terminal.backend().buffer().content() {
        assert_eq!(cell.bg, Color::Reset);
    }
}

#[test]
fn the_main_screen_in_the_terminal_theme() {
    let app = demo_with(&Config::default());

    assert_debug_snapshot!(app.backend().buffer());
}

#[test]
fn the_main_screen_in_the_nord_theme() {
    let app = demo_with(&named("nord"));

    assert_debug_snapshot!(app.backend().buffer());
}

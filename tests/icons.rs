//! Icon sets: `ui.icons = "unicode" | "nerd"`, and every glyph the views
//! draw for a state, priority, fold, comment or project kind coming from
//! the set chosen.

// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::collections::HashSet;
use std::fs;

use chrono::{Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Event};
use ratatoist::config::{self, Config};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::demo;
use ratatoist::todoist::model::Priority;
use ratatoist::ui::icons::{IconSet, Icons};
use ratatui::backend::TestBackend;

/// The demo account at 120x36 with `icons`, its task list focused.
fn task_list(icons: IconSet) -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let config = Config {
        icons,
        ..Config::default()
    };
    let state = initial_state_from(&config, &UiSettings::default(), "*".to_owned(), now, false);
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    let sync = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('l'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    app
}

fn screen(app: &App<TestBackend>) -> String {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every glyph of `icons`, in field order.
fn all(icons: &Icons) -> Vec<&'static str> {
    let mut glyphs = vec![icons.open, icons.completed];
    glyphs.extend(icons.priorities);
    glyphs.extend([
        icons.recurring,
        icons.inbox,
        icons.project,
        icons.favourite,
        icons.folder_open,
        icons.folder_closed,
        icons.fold_open,
        icons.fold_closed,
        icons.comments,
        icons.focus,
        icons.failed,
        icons.overdue,
        icons.today,
    ]);
    glyphs
}

#[test]
fn priority_and_check_glyphs_are_pairwise_distinct_in_both_sets() {
    for set in IconSet::ALL {
        let icons = set.icons();
        let mut glyphs: Vec<&str> = Priority::ALL.map(|p| icons.priority(p)).to_vec();
        glyphs.extend([icons.check(false), icons.check(true)]);

        let distinct: HashSet<&str> = glyphs.iter().copied().collect();

        assert_eq!(distinct.len(), glyphs.len(), "{set:?}: {glyphs:?}");
    }
}

#[test]
fn the_nerd_set_changes_every_glyph() {
    let unicode = all(IconSet::Unicode.icons());
    let nerd = all(IconSet::Nerd.icons());

    for (unicode, nerd) in unicode.iter().zip(&nerd) {
        assert_ne!(unicode, nerd);
    }
}

#[test]
fn every_nerd_glyph_is_one_private_use_character() {
    for glyph in all(IconSet::Nerd.icons()) {
        let chars: Vec<char> = glyph.chars().collect();
        assert_eq!(chars.len(), 1, "{glyph:?}");
        assert!(('\u{e000}'..='\u{f8ff}').contains(&chars[0]), "{glyph:?}");
    }
}

#[test]
fn ui_icons_picks_the_set_and_unicode_is_the_default() {
    let dir = std::env::temp_dir().join(format!("ratatoist-icons-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let default = config::load(&dir);
    fs::write(dir.join("config.toml"), "[ui]\nicons = \"nerd\"\n").unwrap();
    let nerd = config::load(&dir);
    fs::write(dir.join("config.toml"), "[ui]\nicons = \"emoji\"\n").unwrap();
    let bad = config::load(&dir);
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(default.icons, IconSet::Unicode);
    assert_eq!(nerd.icons, IconSet::Nerd);
    assert_eq!(nerd.warnings, Vec::<String>::new());
    assert_eq!(bad.icons, IconSet::Unicode);
    assert_eq!(
        bad.warnings,
        ["config.toml:2:9: `ui.icons` must be \"unicode\" or \"nerd\"; [ui] uses its defaults"]
    );
}

#[test]
fn the_views_draw_glyphs_from_the_set_chosen() {
    let unicode = screen(&task_list(IconSet::Unicode));
    let nerd = screen(&task_list(IconSet::Nerd));
    let icons = IconSet::Nerd.icons();

    for glyph in [
        icons.open,
        icons.inbox,
        icons.project,
        icons.favourite,
        icons.today,
        icons.overdue,
        icons.priority(Priority::P1),
        icons.priority(Priority::P4),
        icons.comments,
    ] {
        assert!(nerd.contains(glyph), "{glyph:?} missing:\n{nerd}");
        assert!(!unicode.contains(glyph), "{glyph:?} in the unicode set");
    }
}

#[test]
fn a_task_list_in_the_unicode_set() {
    assert_snapshot!(screen(&task_list(IconSet::Unicode)));
}

#[test]
fn a_task_list_in_the_nerd_set() {
    assert_snapshot!(screen(&task_list(IconSet::Nerd)));
}

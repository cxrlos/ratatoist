//! Icon sets: `ui.icons = "unicode" | "nerd"`, and every glyph the views
//! draw for a state, priority, fold, comment or project kind coming from
//! the set chosen.

// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::collections::HashSet;
use std::fs;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::tz::TimeZone;
use ratatoist::app::{App, Event, Screen, State};
use ratatoist::config::{self, Config};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::demo;
use ratatoist::todoist::model::Priority;
use ratatoist::ui::icons::{IconSet, Icons};
use ratatoist::ui::list;
use ratatui::backend::TestBackend;

/// The demo account at 120x36 with `icons`, on the Today list.
fn task_list(icons: IconSet) -> App<TestBackend> {
    let now = demo::NOW.to_zoned(TimeZone::UTC).unwrap();
    let config = Config {
        icons,
        ..Config::default()
    };
    let state = initial_state_from(&config, &UiSettings::default(), "*".to_owned(), now, false);
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    let sync =
        serde_json::from_value(demo::account(&demo::NOW.to_zoned(TimeZone::UTC).unwrap()).sync)
            .unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    app
}

/// The demo account at 120x36 with `icons`, on the Projects tree.
fn projects_tree(icons: IconSet) -> App<TestBackend> {
    let mut app = task_list(icons);
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('4'),
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
        icons.deadline,
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

        assert_eq!(distinct.len(), glyphs.len());
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
        assert_eq!(chars.len(), 1);
        assert!(('\u{e000}'..='\u{f8ff}').contains(&chars[0]));
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
    let drawn = |icons| screen(&task_list(icons)) + &screen(&projects_tree(icons));
    let unicode = drawn(IconSet::Unicode);
    let nerd = drawn(IconSet::Nerd);
    let icons = IconSet::Nerd.icons();

    // The Inbox has its own screen, so its glyph shows in the palette and
    // the project picker rather than the tree.
    for glyph in [
        icons.open,
        icons.project,
        icons.favourite,
        icons.overdue,
        icons.priority(Priority::P1),
        icons.priority(Priority::P4),
        icons.comments,
        icons.deadline,
    ] {
        assert!(
            nerd.contains(glyph),
            "a nerd glyph is missing from the rendered list"
        );
        assert!(
            !unicode.contains(glyph),
            "a nerd glyph shows in the unicode set"
        );
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

/// The Inbox of the app fixture, its "Plan the winter trip" parent given
/// three comments so one row carries both a fold and a comment glyph.
fn trip_with_comments() -> State {
    let now = demo::NOW.to_zoned(TimeZone::UTC).unwrap();
    let mut app = App::new(TestBackend::new(120, 36), State::new(now)).unwrap();
    let sync =
        serde_json::from_str(include_str!("fixtures/app/HYPOTHETICAL_full_sync.json")).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    let mut state = app.state().clone();
    state.screen = Screen::Inbox;
    let trip = state.store.tasks.by_id_mut("t-trip").unwrap();
    trip.note_count = Some(3);
    state
}

/// The task row for "Plan the winter trip", drawn with `icons`, its
/// subtasks shown or folded.
fn trip_row(icons: &Icons, folded: bool) -> String {
    let mut state = trip_with_comments();
    if folded {
        state.collapsed.insert("t-trip".to_owned());
    }
    let trip = state.store.tasks.iter().find(|t| t.id == "t-trip").unwrap();
    list::content_line(&state, trip, 0, icons, usize::MAX)
        .to_string()
        .trim_end()
        .to_owned()
}

#[test]
fn the_default_set_draws_the_shipped_fold_and_comment_glyphs_in_a_row() {
    let icons = IconSet::Unicode.icons();

    assert!(trip_row(icons, false).starts_with("▿ "));
    assert!(trip_row(icons, true).starts_with("▹ "));
    assert!(trip_row(icons, false).contains("✉3"));
}

/// The fold and comment alternatives, each drawn into a real task row, for
/// picking the glyphs that ship.
#[test]
fn fold_and_comment_alternatives_in_a_real_row() {
    let unicode = IconSet::Unicode.icons();
    let mut rows = Vec::new();
    for (open, closed, comments) in [("▿", "▹", "✉"), ("▾", "▸", "✎"), ("-", "+", "c")]
    {
        let icons = Icons {
            fold_open: open,
            fold_closed: closed,
            comments,
            ..unicode.clone()
        };
        rows.push(format!("{open} / {closed} / {comments}"));
        rows.push(format!("  shown:  {}", trip_row(&icons, false)));
        rows.push(format!("  folded: {}", trip_row(&icons, true)));
    }
    assert_snapshot!(rows.join("\n"));
}

#[test]
fn the_unicode_focus_marker_is_a_single_angle_quote_unlike_a_closed_folder() {
    let icons = IconSet::Unicode.icons();

    assert_eq!(icons.focus, "\u{203a}");
    assert_ne!(icons.focus, icons.folder_closed);
}

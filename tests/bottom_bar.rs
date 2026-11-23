//! The bottom bar in each context, at 120x36 and 80x24: moves, actions, and
//! a third row with the task edits or the screen's own keys.

#![cfg(test)]

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Event, Modal, State};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");
const WIDE: (u16, u16) = (120, 36);
const NARROW: (u16, u16) = (80, 24);

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
}

/// The app at `size` with `config` as `config.toml`, after the full sync.
fn configured(config: &str, (columns, rows): (u16, u16)) -> App<TestBackend> {
    let config = ratatoist::config::parse(config, &[]);
    assert_eq!(config.warnings, Vec::<String>::new());
    let state = initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    );
    let mut app = App::new(TestBackend::new(columns, rows), state).unwrap();
    let sync: Box<SyncResponse> = Box::new(serde_json::from_str(FULL_SYNC).unwrap());
    app.handle(Event::Synced(Ok(sync))).unwrap();
    app
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn enter() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) {
    for event in events {
        app.handle(event).unwrap();
    }
}

fn row(app: &App<TestBackend>, y: u16) -> String {
    let buffer = app.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

/// The bar's three rows, trailing blanks trimmed.
fn bar(app: &App<TestBackend>) -> String {
    let height = app.backend().buffer().area.height;
    (height - 3..height)
        .map(|y| row(app, y).trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The foreground of the first cell of `text` in the bar.
fn fg_of(app: &App<TestBackend>, text: &str) -> Option<Color> {
    let height = app.backend().buffer().area.height;
    (height - 3..height).find_map(|y| {
        let line = row(app, y);
        let x = line[..line.find(text)?].chars().count();
        let cell = app.backend().buffer().cell((u16::try_from(x).ok()?, y))?;
        Some(cell.fg)
    })
}

/// The Inbox's task list, focused.
fn list(size: (u16, u16)) -> App<TestBackend> {
    let mut app = configured("", size);
    send(&mut app, [key('1')]);
    app
}

/// The detail of the Inbox's second task.
fn detail(size: (u16, u16)) -> App<TestBackend> {
    let mut app = list(size);
    send(&mut app, [key('j'), enter()]);
    app
}

fn picker(size: (u16, u16)) -> App<TestBackend> {
    let mut app = list(size);
    send(&mut app, [key('p')]);
    assert!(matches!(app.state().top_modal(), Some(Modal::Picker(_))));
    app
}

/// The comment prompt, a text field.
fn text_field(size: (u16, u16)) -> App<TestBackend> {
    let mut app = list(size);
    send(&mut app, [key('c')]);
    assert!(matches!(app.state().top_modal(), Some(Modal::Prompt(_))));
    app
}

fn accent(app: &App<TestBackend>) -> Option<Color> {
    Some(app.state().theme().accent)
}

fn muted(app: &App<TestBackend>) -> Option<Color> {
    Some(app.state().theme().muted)
}

#[test]
fn the_bar_takes_the_last_three_rows_with_labels() {
    let app = list(NARROW);

    assert!(row(&app, 21).starts_with(" Move │ "), "{}", bar(&app));
    assert!(row(&app, 22).starts_with(" Do   │ "), "{}", bar(&app));
    assert!(row(&app, 23).starts_with(" View │ "), "{}", bar(&app));
}

#[test]
fn keys_sharing_a_word_merge_and_chords_collapse_to_their_prefix() {
    let app = list(WIDE);

    let bar = bar(&app);
    assert!(bar.contains("C-d/C-u page"), "{bar}");
    assert!(bar.contains("z fold"), "{bar}");
    assert!(!bar.contains("zR"), "{bar}");
}

#[test]
fn a_key_remapped_in_config_shows_its_new_key() {
    let mut app = configured("[keys.list]\ntoggle-complete = \"X\"\n", NARROW);
    send(&mut app, [key('1')]);

    let bar = bar(&app);
    assert!(bar.contains("X complete"), "{bar}");
    assert!(!bar.contains("x complete"), "{bar}");
}

#[test]
fn complete_is_muted_on_an_empty_list_and_accented_on_a_task() {
    let mut state: State = list(NARROW).state().clone();
    state.tasks.clear();
    let empty = App::new(TestBackend::new(80, 24), state).unwrap();
    assert_eq!(fg_of(&empty, "x complete"), muted(&empty));

    let app = list(NARROW);
    assert_eq!(fg_of(&app, "x complete"), accent(&app));
}

#[test]
fn moving_between_panes_is_muted_on_a_narrow_terminal() {
    let app = list(NARROW);
    assert_eq!(fg_of(&app, "h/l pane"), muted(&app));

    let app = list(WIDE);
    assert_eq!(fg_of(&app, "h/l pane"), accent(&app));
}

#[test]
fn a_picker_shows_only_its_own_keys() {
    let app = picker(NARROW);

    let bar = bar(&app);
    assert!(bar.contains("Enter choose"), "{bar}");
    assert!(bar.contains("Esc close"), "{bar}");
    assert!(!bar.contains("complete"), "{bar}");
    assert!(!bar.contains("quit"), "{bar}");
}

#[test]
fn a_picker_leaves_out_the_keys_its_filter_takes() {
    let mut app = picker(NARROW);
    send(&mut app, [key('j')]);
    assert!(
        matches!(app.state().top_modal(), Some(Modal::Picker(picker)) if picker.query() == "j")
    );

    let bar = bar(&app);
    assert!(!bar.contains("j/k"));
    assert!(bar.contains("Esc close"));
}

#[test]
fn a_text_field_leaves_out_the_keys_that_would_type() {
    let app = text_field(NARROW);

    let bar = bar(&app);
    assert!(bar.contains("Enter set") || bar.contains("Enter send"));
    assert!(bar.contains("Esc close"));
    assert!(!bar.contains("j/"));
}

#[test]
fn an_overlay_leaves_the_bar_keys_accented() {
    let mut form = list(NARROW);
    send(&mut form, [key('a')]);
    assert!(matches!(form.state().top_modal(), Some(Modal::Form(_))));

    for app in [picker(NARROW), text_field(NARROW), form] {
        assert_eq!(fg_of(&app, "Esc close"), accent(&app));
    }
}

#[test]
fn items_that_do_not_fit_are_dropped_whole() {
    let app = configured("", (40, 24));

    for line in bar(&app).lines() {
        assert!(!line.ends_with('…'), "{line}");
        assert!(ratatoist::ui::text::width(line) <= 40, "{line}");
    }
}

#[test]
fn no_view_or_component_writes_a_key_hint() {
    let sources = [
        include_str!("../src/ui/views/detail.rs"),
        include_str!("../src/ui/views/tasks.rs"),
        include_str!("../src/ui/views/projects.rs"),
        include_str!("../src/ui/prompt.rs"),
        include_str!("../src/ui/components/task_form.rs"),
        include_str!("../src/ui/picker.rs"),
        include_str!("../src/ui/confirm.rs"),
    ];
    for source in sources {
        for hint in ["\"Esc", "\"Enter", "\"press ", "Esc cancel", "Tab next"] {
            assert!(!source.contains(hint));
        }
    }
}

#[test]
fn bar_on_the_list_at_120x36() {
    assert_snapshot!(bar(&list(WIDE)));
}

#[test]
fn bar_on_the_list_at_80x24() {
    assert_snapshot!(bar(&list(NARROW)));
}

#[test]
fn bar_on_the_detail_at_120x36() {
    assert_snapshot!(bar(&detail(WIDE)));
}

#[test]
fn bar_on_the_detail_at_80x24() {
    assert_snapshot!(bar(&detail(NARROW)));
}

#[test]
fn bar_in_a_picker_at_120x36() {
    assert_snapshot!(bar(&picker(WIDE)));
}

#[test]
fn bar_in_a_picker_at_80x24() {
    assert_snapshot!(bar(&picker(NARROW)));
}

#[test]
fn bar_in_a_text_field_at_120x36() {
    assert_snapshot!(bar(&text_field(WIDE)));
}

#[test]
fn bar_in_a_text_field_at_80x24() {
    assert_snapshot!(bar(&text_field(NARROW)));
}

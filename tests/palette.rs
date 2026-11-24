//! The palette: `:` or `C-k`, a query that fuzzy-finds available actions,
//! loaded tasks and projects, and Enter to run, jump or open.

#![cfg(test)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::Zoned;
use jiff::civil::date;
use jiff::tz::TimeZone;
use ratatoist::app::{
    self, Action, App, Event, Modal, Palette, PaletteEntry, Pane, PickerKind, State,
};
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> Zoned {
    date(2026, 11, 22)
        .at(9, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
}

/// The app at `columns` x `rows` after the full sync, on the projects.
fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
    let sync: Box<SyncResponse> = Box::new(serde_json::from_str(FULL_SYNC).unwrap());
    app.handle(Event::Synced(Ok(sync))).unwrap();
    app
}

/// The Inbox's task list at `columns` x `rows`.
fn list(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = sized(columns, rows);
    send(&mut app, [key('1')]);
    app
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn ctrl(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn typed(text: &str) -> Vec<Event> {
    text.chars().map(key).collect()
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) {
    for event in events {
        app.handle(event).unwrap();
    }
}

fn screen(app: &App<TestBackend>) -> Vec<String> {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn text(app: &App<TestBackend>) -> String {
    screen(app).join("\n")
}

fn palette(app: &App<TestBackend>) -> Option<&Palette> {
    match app.state().top_modal() {
        Some(Modal::Palette(palette)) => Some(palette),
        _ => None,
    }
}

/// The palette's rows under its rule, inside its border, trimmed.
fn rows(app: &App<TestBackend>) -> Vec<String> {
    let rows = screen(app);
    let top = rows
        .iter()
        .position(|row| row.contains("┌ Palette"))
        .unwrap();
    let left = rows[top].chars().position(|c| c == '┌').unwrap();
    let right = rows[top].chars().position(|c| c == '┐').unwrap();
    rows.iter()
        .skip(top + 3)
        .take_while(|row| row.chars().nth(left) != Some('└'))
        .map(|row| {
            let inside: String = row.chars().skip(left + 1).take(right - left - 1).collect();
            inside.trim().to_owned()
        })
        .collect()
}

#[test]
fn colon_and_ctrl_k_open_the_palette_and_esc_closes_it() {
    let mut app = list(120, 36);

    send(&mut app, [key(':')]);
    assert_eq!(palette(&app), Some(&Palette::default()));
    send(&mut app, [press(KeyCode::Esc)]);
    assert!(app.state().modals.is_empty());

    send(&mut app, [ctrl('k')]);
    assert_eq!(palette(&app), Some(&Palette::default()));
}

#[test]
fn typing_filters_and_letters_type_rather_than_move() {
    let mut app = list(120, 36);

    send(&mut app, [key(':')]);
    send(&mut app, typed("jkq"));
    assert_eq!(palette(&app).map(|p| p.query.as_str()), Some("jkq"));

    send(&mut app, [press(KeyCode::Backspace)]);
    assert_eq!(palette(&app).map(|p| p.query.as_str()), Some("jk"));
}

#[test]
fn typing_prio_lists_the_priority_action_with_its_key_first() {
    let mut app = list(120, 36);

    send(&mut app, [key(':')]);
    send(&mut app, typed("prio"));

    let matches = app::palette_matches(app.state(), "prio");
    assert!(matches!(
        matches.first(),
        Some(PaletteEntry::Action(spec)) if spec.id == "edit-priority"
    ));
    let rows = rows(&app);
    let first = rows.iter().find(|row| !row.is_empty()).unwrap();
    assert!(first.starts_with("Change the priority"), "{first}");
    assert!(first.ends_with(" p"), "{first}");
}

#[test]
fn unavailable_actions_are_not_offered() {
    let app = list(80, 24);

    let offered = |query: &str| {
        app::palette_matches(app.state(), query)
            .into_iter()
            .any(|entry| matches!(entry, PaletteEntry::Action(spec) if spec.id == "focus-left"))
    };
    assert!(!offered("focus the pane on the left"), "not wide");
    let release = |entry: &PaletteEntry| {
        matches!(entry, PaletteEntry::Action(spec)
            if spec.id == "release-notes" || spec.id == "skip-release")
    };
    assert!(!app::palette_matches(app.state(), "").iter().any(release));

    let wide = list(120, 36);
    assert!(
        app::palette_matches(wide.state(), "focus the pane on the left")
            .iter()
            .any(|entry| matches!(entry, PaletteEntry::Action(spec) if spec.id == "focus-left"))
    );
}

#[test]
fn with_no_query_actions_come_first_then_tasks_then_projects() {
    let app = list(120, 36);

    let kinds: Vec<u8> = app::palette_matches(app.state(), "")
        .iter()
        .map(|entry| match entry {
            PaletteEntry::Action(_) => 0,
            PaletteEntry::Task(_) => 1,
            PaletteEntry::Project(_) => 2,
        })
        .collect();
    let mut sorted = kinds.clone();
    sorted.sort_unstable();
    assert_eq!(kinds, sorted);
    for kind in [0, 1, 2] {
        assert!(kinds.contains(&kind));
    }
}

#[test]
fn down_up_and_ctrl_n_ctrl_p_move_the_highlight() {
    let mut app = list(120, 36);
    send(&mut app, [key(':')]);

    send(&mut app, [press(KeyCode::Down), ctrl('n')]);
    assert_eq!(palette(&app).map(|p| p.selected), Some(2));

    send(&mut app, [press(KeyCode::Up)]);
    assert_eq!(palette(&app).map(|p| p.selected), Some(1));
    send(&mut app, [ctrl('p'), ctrl('p')]);
    assert_eq!(palette(&app).map(|p| p.selected), Some(0));

    send(&mut app, [press(KeyCode::Down), key('x')]);
    assert_eq!(
        palette(&app).map(|p| p.selected),
        Some(0),
        "typing resets it"
    );
}

#[test]
fn enter_on_an_action_runs_it() {
    let mut app = list(120, 36);
    send(&mut app, [key(':')]);
    send(&mut app, typed("choose a theme"));

    send(&mut app, [press(KeyCode::Enter)]);

    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Picker(picker)) if picker.kind == PickerKind::Theme
    ));
    assert_eq!(app.state().modals.len(), 1);
}

#[test]
fn enter_on_a_task_focuses_it_in_its_project_s_list() {
    let mut app = sized(120, 36);
    send(&mut app, [key(':')]);
    send(&mut app, typed("roadmap"));

    send(&mut app, [press(KeyCode::Enter)]);

    let state = app.state();
    assert!(state.modals.is_empty());
    assert_eq!(state.active_pane, Pane::Tasks);
    assert_eq!(state.selected_project_name(), "Work");
    assert_eq!(
        state.selected_task().map(|task| task.id.as_str()),
        Some("t-roadmap")
    );
    assert!(text(&app).contains("Draft the roadmap"));
}

#[test]
fn enter_on_a_folded_subtask_unfolds_its_parent() {
    let mut app = list(80, 24);
    send(&mut app, [key('j'), key('j'), key('j'), key('z'), key('a')]);
    assert!(!text(&app).contains("Book the train"));

    send(&mut app, [key(':')]);
    send(&mut app, typed("book the train"));
    send(&mut app, [press(KeyCode::Enter)]);

    assert_eq!(
        app.state().selected_task().map(|task| task.id.as_str()),
        Some("t-trip-train")
    );
}

#[test]
fn enter_on_a_project_opens_its_list() {
    let mut app = sized(120, 36);
    send(&mut app, [key(':')]);
    send(&mut app, typed("launch"));

    send(&mut app, [press(KeyCode::Enter)]);

    let state = app.state();
    assert!(state.modals.is_empty());
    assert_eq!(state.active_pane, Pane::Tasks);
    assert_eq!(state.selected_project_name(), "Launch");
    assert_eq!(state.selected_task, 0);
}

#[test]
fn enter_with_nothing_matching_keeps_the_palette_open() {
    let mut app = list(120, 36);
    send(&mut app, [key(':')]);
    send(&mut app, typed("zzzzqqq"));

    send(&mut app, [press(KeyCode::Enter)]);

    assert!(palette(&app).is_some());
    assert!(text(&app).contains("Nothing matches."));
    let accept = ratatoist::keys::spec(Action::Accept).unwrap();
    assert!(!accept.availability(app.state()).can_run());
}

#[test]
fn a_long_query_keeps_its_last_typed_characters_and_the_cursor() {
    let mut app = list(80, 24);
    let query = "the plants, the winter trip, the reading group chapter, the dentist, the train and the end";
    send(&mut app, [key(':')]);
    send(&mut app, typed(query));

    let rows = screen(&app);
    let line = rows.iter().find(|row| row.contains(" : ")).unwrap();
    let after_prompt = &line[line.find(" : ").unwrap() + 3..];

    assert!(after_prompt.starts_with('…'));
    assert!(after_prompt.contains("train and the end "));
}

#[test]
fn palette_typing_prio_at_120x36() {
    let mut app = list(120, 36);
    send(&mut app, [key(':')]);
    send(&mut app, typed("prio"));

    assert_snapshot!(text(&app));
}

#[test]
fn palette_typing_prio_at_80x24() {
    let mut app = list(80, 24);
    send(&mut app, [key(':')]);
    send(&mut app, typed("prio"));

    assert_snapshot!(text(&app));
}

#[test]
fn a_highlight_past_the_matches_takes_the_last_one() {
    let mut state = sized(120, 36).state().clone();
    state.modals.push(Modal::Palette(Palette {
        query: "roadmap".to_owned(),
        selected: 40,
    }));
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    assert!(matches!(
        app::palette_matches(app.state(), "roadmap").last(),
        Some(PaletteEntry::Task(task)) if task.id == "t-roadmap"
    ));
    let accept = ratatoist::keys::spec(Action::Accept).unwrap();
    assert!(accept.availability(app.state()).can_run());
    assert!(!app::palette_can_step_down(app.state()));

    send(&mut app, [press(KeyCode::Enter)]);

    assert_eq!(
        app.state().selected_task().map(|task| task.id.as_str()),
        Some("t-roadmap")
    );
}

#[test]
fn palette_with_no_query_at_80x24() {
    let mut app = list(80, 24);
    send(&mut app, [key(':')]);

    assert_snapshot!(text(&app));
}

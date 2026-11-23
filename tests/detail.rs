#![cfg(test)]

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, Modal, Pane, State, update};
use ratatoist::todoist::sync::SyncResponse;
use ratatoist::ui::views::detail::DetailField;
use ratatui::backend::TestBackend;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
}

fn response(json: &str) -> Box<SyncResponse> {
    Box::new(serde_json::from_str(json).unwrap())
}

/// The app at `columns` x `rows` after the full sync, told its size as the
/// runtime's first resize would.
fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
    app.handle(Event::Synced(Ok(response(FULL_SYNC)))).unwrap();
    app.handle(Event::Resize {
        width: columns,
        height: rows,
    })
    .unwrap();
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

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
    events
        .into_iter()
        .flat_map(|event| app.handle(event).unwrap())
        .collect()
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

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    screen(app).iter().any(|line| line.contains(text))
}

/// Every command type sent in `effects`.
fn command_types(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendCommands(commands) => Some(commands),
            _ => None,
        })
        .flatten()
        .map(|command| command.r#type.clone())
        .collect()
}

/// Opens the detail of "Call the dentist", the second Inbox task.
fn open_call_detail(app: &mut App<TestBackend>) {
    send(app, [key('l'), key('j'), press(KeyCode::Enter)]);
    assert_eq!(app.state().active_pane, Pane::Detail);
}

fn switch_to_standard(app: &mut App<TestBackend>) {
    send(app, [key(','), press(KeyCode::Enter), press(KeyCode::Esc)]);
    assert!(shows(app, "STANDARD"));
}

#[test]
fn esc_in_vim_mode_cancels_a_comment_and_sends_nothing() {
    let mut app = sized(120, 36);
    open_call_detail(&mut app);
    send(&mut app, [key('c')]);
    send(&mut app, "Never mind".chars().map(key));

    let effects = send(&mut app, [press(KeyCode::Esc)]);

    assert!(command_types(&effects).is_empty(), "{effects:?}");
    assert!(app.state().modals.is_empty());
    assert!(!shows(&app, "Never mind"));
    assert!(shows(&app, "NORMAL"));
}

#[test]
fn esc_in_vim_mode_cancels_a_field_edit_and_sends_nothing() {
    let mut app = sized(120, 36);
    open_call_detail(&mut app);
    send(&mut app, [key('i')]);
    send(&mut app, " soon".chars().map(key));

    let effects = send(&mut app, [press(KeyCode::Esc)]);

    assert!(command_types(&effects).is_empty(), "{effects:?}");
    assert!(app.state().modals.is_empty());
    assert!(shows(&app, "Call the dentist"));
    assert!(!shows(&app, "Call the dentist soon"));
}

#[test]
fn esc_in_standard_mode_cancels_a_comment_and_a_field_edit() {
    let mut app = sized(120, 36);
    switch_to_standard(&mut app);
    send(&mut app, [press(KeyCode::Tab), press(KeyCode::Down)]);
    send(&mut app, [press(KeyCode::Enter)]);
    assert_eq!(app.state().active_pane, Pane::Detail);

    send(&mut app, [key('c')]);
    send(&mut app, "Never mind".chars().map(key));
    let comment = send(&mut app, [press(KeyCode::Esc)]);
    send(&mut app, [press(KeyCode::Enter)]);
    send(&mut app, " soon".chars().map(key));
    let field = send(&mut app, [press(KeyCode::Esc)]);

    assert!(command_types(&comment).is_empty(), "{comment:?}");
    assert!(command_types(&field).is_empty(), "{field:?}");
    assert!(app.state().modals.is_empty());
}

#[test]
fn enter_submits_a_field_edit_as_one_item_update() {
    let mut app = sized(120, 36);
    open_call_detail(&mut app);
    send(&mut app, [key('i')]);
    send(&mut app, " soon".chars().map(key));

    let effects = send(&mut app, [press(KeyCode::Enter)]);

    assert_eq!(command_types(&effects), ["item_update"]);
    assert!(shows(&app, "Call the dentist soon"));
}

/// Opens the detail, moves the field cursor down `steps` and starts the
/// edit.
fn edit_field(steps: usize) -> App<TestBackend> {
    let mut app = sized(120, 36);
    open_call_detail(&mut app);
    send(&mut app, (0..steps).map(|_| key('j')));
    send(&mut app, [key('i')]);
    app
}

#[test]
fn editing_the_content_says_so_in_the_title() {
    let app = edit_field(0);
    assert_eq!(app.state().detail_field, DetailField::Content);
    assert!(shows(&app, " Edit Content "), "{:#?}", screen(&app));
}

#[test]
fn editing_the_priority_opens_the_priority_picker() {
    let app = edit_field(1);
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::PriorityPicker { .. })
    ));
    assert!(shows(&app, " Priority "), "{:#?}", screen(&app));
}

#[test]
fn editing_the_due_date_says_so_in_the_title() {
    let app = edit_field(2);
    assert!(shows(&app, " Edit Due Date "), "{:#?}", screen(&app));
}

#[test]
fn editing_the_description_says_so_in_the_title() {
    let app = edit_field(3);
    assert!(shows(&app, " Edit Description "), "{:#?}", screen(&app));
}

/// "Call the dentist" with a description of 60 numbered lines.
fn long_description() -> Box<SyncResponse> {
    let description: Vec<String> = (1..=60).map(|n| format!("Line {n} of the notes")).collect();
    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "fixture-token-2",
        "items": [{
            "id": "t-call",
            "content": "Call the dentist",
            "description": description.join("\n"),
            "checked": false,
            "child_order": 2,
            "priority": 3,
            "project_id": "p-inbox",
            "due": { "date": "2026-11-22", "is_recurring": false, "string": "today" },
            "added_at": "2026-11-19T08:30:00Z"
        }]
    });
    response(&delta.to_string())
}

fn long_detail_at_80x24() -> App<TestBackend> {
    let mut app = sized(80, 24);
    app.handle(Event::Synced(Ok(long_description()))).unwrap();
    open_call_detail(&mut app);
    app
}

#[test]
fn ctrl_d_scrolls_a_long_description_half_a_page() {
    let mut app = long_detail_at_80x24();
    let before = screen(&app);

    send(&mut app, [ctrl('d')]);

    assert_ne!(screen(&app), before);
    assert!(shows(&app, "Line 20 of the notes"), "{:#?}", screen(&app));
    assert_snapshot!(app.backend());
}

#[test]
fn scrolling_stops_at_the_end_of_the_detail() {
    let mut app = long_detail_at_80x24();
    send(&mut app, (0..20).map(|_| ctrl('d')));
    let at_end = screen(&app);
    let scroll = app.state().detail_scroll;

    send(&mut app, [ctrl('d'), ctrl('e')]);

    assert_eq!(screen(&app), at_end);
    assert_eq!(app.state().detail_scroll, scroll);
    assert!(shows(&app, "Line 60 of the notes"));
    assert!(shows(&app, "Thanks, I will call after ten."));
    assert!(shows(&app, "i edit  c comment"), "the last row is in view");
    assert_snapshot!(app.backend());
}

#[test]
fn ctrl_u_from_the_end_moves_back_up_at_once() {
    let mut app = long_detail_at_80x24();
    send(&mut app, (0..20).map(|_| ctrl('d')));
    let at_end = screen(&app);

    send(&mut app, [ctrl('u')]);

    assert_ne!(screen(&app), at_end);
}

#[test]
fn ctrl_e_and_ctrl_y_scroll_one_line() {
    let mut app = long_detail_at_80x24();

    send(&mut app, [ctrl('e'), ctrl('e'), ctrl('e')]);
    assert_eq!(app.state().detail_scroll, 3);

    send(&mut app, [ctrl('y')]);
    assert_eq!(app.state().detail_scroll, 2);

    send(&mut app, (0..5).map(|_| ctrl('y')));
    assert_eq!(app.state().detail_scroll, 0);
}

#[test]
fn the_scroll_keys_work_in_standard_mode() {
    let mut app = sized(80, 24);
    app.handle(Event::Synced(Ok(long_description()))).unwrap();
    switch_to_standard(&mut app);
    send(&mut app, [press(KeyCode::Tab), press(KeyCode::Down)]);
    send(&mut app, [press(KeyCode::Enter)]);

    send(&mut app, [ctrl('d')]);
    let half = app.state().detail_scroll;
    send(&mut app, [ctrl('e')]);

    assert!(half > 0);
    assert_eq!(app.state().detail_scroll, half + 1);
}

#[test]
fn a_detail_that_fits_does_not_scroll() {
    let mut app = sized(120, 36);
    open_call_detail(&mut app);

    send(&mut app, [ctrl('d'), ctrl('e')]);

    assert_eq!(app.state().detail_scroll, 0);
}

#[test]
fn a_taller_screen_lowers_the_scroll_limit() {
    let mut state = State::new(now());
    update(&mut state, Event::Synced(Ok(response(FULL_SYNC))));
    update(&mut state, Event::Synced(Ok(long_description())));
    for event in [key('l'), key('j'), press(KeyCode::Enter)] {
        update(&mut state, event);
    }
    let scroll_to_end = |state: &mut State| {
        for _ in 0..40 {
            update(state, ctrl('d'));
        }
        state.detail_scroll
    };

    update(
        &mut state,
        Event::Resize {
            width: 80,
            height: 24,
        },
    );
    let small = scroll_to_end(&mut state);
    update(
        &mut state,
        Event::Resize {
            width: 120,
            height: 60,
        },
    );
    let tall = state.detail_scroll;

    assert!(tall < small, "{tall} < {small}");
    assert_eq!(scroll_to_end(&mut state), tall);
}

#[test]
fn the_hint_bar_names_the_scroll_keys() {
    let mut app = sized(80, 24);
    open_call_detail(&mut app);

    assert!(shows(&app, "Ctrl-d/u scroll"), "{:#?}", screen(&app));
    assert!(!shows(&app, "j/k scroll"));
}

#[test]
fn the_cheatsheet_names_the_scroll_keys() {
    let mut app = sized(120, 60);
    open_call_detail(&mut app);

    send(&mut app, [key('?')]);

    assert!(shows(&app, "Ctrl-d / Ctrl-u"), "{:#?}", screen(&app));
    assert!(shows(&app, "Ctrl-e / Ctrl-y"), "{:#?}", screen(&app));
}

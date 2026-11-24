#![cfg(test)]

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::civil::date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Zoned};
use ratatoist::app::{App, Effect, ErrorKind, Event, Modal, Pane, PickerKind, State, update};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::model::Task;
use ratatoist::todoist::sync::{SyncCommand, SyncResponse};
use ratatoist::todoist::{ClientError, CompletedWindow, LiveMessage};
use ratatui::backend::TestBackend;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> Zoned {
    date(2026, 11, 22)
        .at(9, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
}

fn full_sync() -> Box<SyncResponse> {
    Box::new(serde_json::from_str(FULL_SYNC).unwrap())
}

/// The app at `columns` x `rows`, after the fixture's full sync answered.
fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
    app.handle(Event::Synced(Ok(full_sync()))).unwrap();
    app
}

fn app() -> App<TestBackend> {
    sized(120, 36)
}

/// The app at 120x36 after the full sync, with `config` as `config.toml`.
fn configured(config: &str) -> App<TestBackend> {
    let config = ratatoist::config::parse(config, &[]);
    assert_eq!(config.warnings, Vec::<String>::new());
    let state = initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    );
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    app.handle(Event::Synced(Ok(full_sync()))).unwrap();
    app
}

/// `,` opens the theme picker, which has no default key.
const THEME_ON_COMMA: &str = "[keys.global]\nchoose-theme = \",\"\n";

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

fn task<'a>(app: &'a App<TestBackend>, id: &str) -> &'a ratatoist::todoist::model::Task {
    app.state().store.tasks.iter().find(|t| t.id == id).unwrap()
}

/// The one command in a flush; fails if a flush carries more or none.
fn single_command(effects: &[Effect]) -> SyncCommand {
    let flushes: Vec<&Vec<SyncCommand>> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendCommands(commands) => Some(commands),
            _ => None,
        })
        .collect();
    assert_eq!(flushes.len(), 1, "{effects:?}");
    assert_eq!(flushes[0].len(), 1, "one command per flush");
    flushes[0][0].clone()
}

fn delta(json: &str) -> Box<SyncResponse> {
    Box::new(serde_json::from_str(json).unwrap())
}

#[test]
fn shows_the_loading_line_until_the_first_sync_answers() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    assert!(shows(&app, "Loading your tasks from Todoist…"));

    let effects = app.handle(Event::Start).unwrap();

    let [Effect::Sync(request)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.sync_token, "*");
    assert!(request.resource_types.contains(&"user".to_owned()));
    app.handle(Event::Synced(Ok(full_sync()))).unwrap();
    assert!(!shows(&app, "Loading your tasks"));
    assert!(shows(&app, "synced 0s ago"));
}

#[test]
fn the_first_sync_opens_the_live_connection_and_forgets_the_old_sync_token() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();

    let effects = app.handle(Event::Synced(Ok(full_sync()))).unwrap();

    assert_eq!(app.state().sync_token, "fixture-token-1");
    assert_eq!(
        effects,
        [
            Effect::ForgetOldSyncState,
            Effect::OpenLive {
                url: "wss://live.example.invalid/ws".to_owned()
            },
        ]
    );
}

#[test]
fn a_failed_first_sync_shows_the_error_in_the_notice() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();

    app.handle(Event::Synced(Err(ClientError::Network(
        "offline".to_owned(),
    ))))
    .unwrap();

    assert!(
        shows(&app, "Could not load your tasks"),
        "{:#?}",
        screen(&app)
    );
}

#[test]
fn main_layout_with_the_inbox_selected_at_120x36() {
    let app = sized(120, 36);

    assert_eq!(
        app.state().store.projects[app.state().selected_project].name,
        "Inbox"
    );
    assert_snapshot!(app.backend());
}

#[test]
fn main_layout_with_the_inbox_selected_at_80x24() {
    assert_snapshot!(sized(80, 24).backend());
}

#[test]
fn task_detail_at_120x36() {
    let mut app = sized(120, 36);

    send(&mut app, [key('1'), key('j'), press(KeyCode::Enter)]);

    assert_eq!(app.state().active_pane, Pane::Detail);
    assert_snapshot!(app.backend());
}

#[test]
fn task_detail_at_80x24() {
    let mut app = sized(80, 24);

    send(&mut app, [key('1'), key('j'), press(KeyCode::Enter)]);

    assert_snapshot!(app.backend());
}

#[test]
fn cheatsheet_at_120x36() {
    let mut app = sized(120, 36);

    send(&mut app, [key('?')]);

    assert_eq!(app.state().modals, [Modal::Cheatsheet { scroll: 0 }]);
    assert_snapshot!(app.backend());
}

#[test]
fn cheatsheet_at_80x24() {
    let mut app = sized(80, 24);

    send(&mut app, [key('?')]);

    assert_snapshot!(app.backend());
}

#[test]
fn the_cheatsheet_closes_on_esc_and_swallows_other_keys() {
    let mut app = app();
    send(&mut app, [key('?'), key('j'), key('l')]);
    assert_eq!(app.state().active_pane, Pane::Tasks);

    send(&mut app, [press(KeyCode::Esc)]);

    assert!(app.state().modals.is_empty());
}

/// A picker takes printable keys as its filter, so only F5 is unbound there.
#[test]
fn an_unbound_key_under_an_overlay_does_not_redraw() {
    let cheatsheet = (vec![key('?')], vec![key('y'), press(KeyCode::F(5))]);
    let theme_picker = (vec![key(',')], vec![press(KeyCode::F(5))]);
    let priority_picker = (vec![key('1'), key('p')], vec![press(KeyCode::F(5))]);
    for (open, unbound) in [cheatsheet, theme_picker, priority_picker] {
        let mut app = configured(THEME_ON_COMMA);
        send(&mut app, open);
        let overlay = app.state().top_modal().cloned();
        assert!(overlay.is_some());
        let frames = app.frames_drawn();

        send(&mut app, unbound);

        assert_eq!(app.frames_drawn(), frames);
        assert_eq!(app.state().top_modal().cloned(), overlay);
    }
}

#[test]
fn the_screen_draws_with_an_out_of_range_theme() {
    let mut state = State::new(now());
    update(&mut state, Event::Synced(Ok(full_sync())));
    state.theme_idx = state.themes.len();

    let app = App::new(TestBackend::new(120, 36), state).unwrap();

    assert!(shows(&app, "Inbox"));
}

#[test]
fn a_key_that_changes_nothing_does_not_redraw() {
    let mut app = app();
    let frames = app.frames_drawn();

    send(&mut app, [key('y'), key('Y'), press(KeyCode::F(5))]);

    assert_eq!(app.frames_drawn(), frames);
}

#[test]
fn a_tick_that_changes_nothing_does_not_redraw() {
    let mut app = app();
    send(
        &mut app,
        [Event::Tick {
            now: now() + SignedDuration::from_secs(61),
        }],
    );
    let frames = app.frames_drawn();

    send(
        &mut app,
        [Event::Tick {
            now: now() + SignedDuration::from_secs(62),
        }],
    );

    assert_eq!(app.frames_drawn(), frames);
}

#[test]
fn a_key_that_moves_redraws_once() {
    let mut app = app();
    let frames = app.frames_drawn();

    send(&mut app, [key('j')]);

    assert_eq!(app.frames_drawn(), frames + 1);
}

#[test]
fn q_and_ctrl_c_ask_to_quit() {
    assert_eq!(app().handle(key('q')).unwrap(), [Effect::Quit]);
    assert_eq!(app().handle(ctrl('c')).unwrap(), [Effect::Quit]);
    assert_eq!(app().handle(Event::Shutdown).unwrap(), [Effect::Quit]);
}

#[test]
fn completing_a_task_checks_it_at_once_and_sends_one_item_close() {
    let mut app = app();

    let effects = send(&mut app, [key('1'), key('x')]);

    let command = single_command(&effects);
    assert_eq!(command.r#type, "item_close");
    assert_eq!(command.args["id"], "t-milk");
    assert!(task(&app, "t-milk").checked);
    assert!(!shows(&app, "Buy oat milk"), "the Active filter hides it");
}

#[test]
fn completing_a_recurring_task_sends_item_complete() {
    let mut app = app();

    let effects = send(&mut app, [key('1'), key('j'), key('j'), key('x')]);

    let command = single_command(&effects);
    assert_eq!(command.args["id"], "t-plants");
    assert_eq!(command.r#type, "item_complete");
}

#[test]
fn a_completed_recurring_task_stays_open_with_its_due_pending_until_the_answer() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('j'), key('j'), key('x')]));

    assert!(!task(&app, "t-plants").checked);
    assert_eq!(app.state().pending_due("t-plants"), Some("every 2 days"));
    assert!(shows(&app, "every 2 days…"), "{:#?}", screen(&app));

    let answer = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-2",
            "sync_status":{{"{}":"ok"}},
            "items":[{{"id":"t-plants","content":"Water the plants","checked":false,
              "priority":1,"project_id":"p-inbox","child_order":3,
              "due":{{"date":"2026-11-26","is_recurring":true,"string":"every 2 days"}}}}]}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Ok(delta(&answer)),
    })
    .unwrap();

    let plants = task(&app, "t-plants");
    assert_eq!(plants.due.as_ref().unwrap().date, "2026-11-26");
    assert!(!plants.checked);
    assert_eq!(app.state().pending_due("t-plants"), None);
    assert!(!app.state().is_pending(&command.uuid));
}

#[test]
fn a_rejected_recurring_completion_drops_the_pending_due() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('j'), key('j'), key('x')]));

    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Err(ClientError::Network("offline".to_owned())),
    })
    .unwrap();

    assert_eq!(app.state().pending_due("t-plants"), None);
    assert!(!task(&app, "t-plants").checked);
}

#[test]
fn a_flush_answer_merges_the_changes_it_carries_and_takes_its_token() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    let answer = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-2",
            "sync_status":{{"{}":"ok"}},
            "items":[
              {{"id":"t-milk","content":"Buy oat milk","checked":true,"priority":4,
               "project_id":"p-inbox","child_order":1}},
              {{"id":"t-read","content":"Read chapter two","checked":false,"priority":1,
               "project_id":"p-inbox","child_order":5}}
            ]}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Ok(delta(&answer)),
    })
    .unwrap();

    assert!(task(&app, "t-milk").checked);
    assert_eq!(task(&app, "t-read").content, "Read chapter two");
    assert_eq!(app.state().sync_token, "fixture-token-2");
}

#[test]
fn a_flush_answer_lands_todoist_s_version_of_the_task_it_settles() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    let answer = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-2",
            "sync_status":{{"{}":"ok"}},
            "items":[{{"id":"t-milk","content":"Buy oat milk (normalised)","checked":true,
              "priority":4,"project_id":"p-inbox","child_order":1}}]}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Ok(delta(&answer)),
    })
    .unwrap();

    assert_eq!(task(&app, "t-milk").content, "Buy oat milk (normalised)");
}

#[test]
fn a_failed_flush_restores_the_completed_task_and_says_so() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Err(ClientError::Network("connection reset".to_owned())),
    })
    .unwrap();

    assert!(!task(&app, "t-milk").checked);
    assert!(!app.state().is_pending(&command.uuid));
    assert!(
        shows(&app, "Could not complete \"Buy oat milk\""),
        "{:#?}",
        screen(&app)
    );
}

#[test]
fn a_rejected_command_restores_the_task_and_shows_todoist_s_message() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    let rejection = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-2",
            "sync_status":{{"{}":{{"error_code":22,"error":"Item not found"}}}}}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Ok(delta(&rejection)),
    })
    .unwrap();

    assert!(!task(&app, "t-milk").checked);
    assert!(
        shows(&app, "Todoist refused the change"),
        "{:#?}",
        screen(&app)
    );
    let logged = app.state().failures.newest().map(|f| f.message.as_str());
    assert_eq!(logged, Some("Item not found"));
}

#[test]
fn the_next_key_clears_the_error_notice_and_still_runs() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Err(ClientError::Network("offline".to_owned())),
    })
    .unwrap();

    send(&mut app, [key('j')]);

    assert!(!app.state().show_failure);
    assert!(!shows(&app, "Could not"));
    assert_eq!(app.state().failures.len(), 1);
    assert_eq!(app.state().selected_task, 1);
}

#[test]
fn a_racing_delta_does_not_clobber_a_task_with_a_pending_command() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    app.handle(Event::Synced(Ok(delta(
        r#"{"full_sync":false,"sync_token":"fixture-token-2",
            "items":[
              {"id":"t-milk","content":"Buy oat milk","checked":false,"priority":4,
               "project_id":"p-inbox","child_order":1},
              {"id":"t-read","content":"Read chapter two","checked":false,"priority":1,
               "project_id":"p-inbox","child_order":5}
            ]}"#,
    ))))
    .unwrap();

    assert!(app.state().is_pending(&command.uuid));
    assert!(task(&app, "t-milk").checked, "the pending value survives");
    assert_eq!(task(&app, "t-read").content, "Read chapter two");
}

const RENAMED_MILK: &str = r#"{"full_sync":false,"sync_token":"fixture-token-2",
    "items":[{"id":"t-milk","content":"Buy oat milk and bread","checked":false,
      "priority":4,"project_id":"p-inbox","child_order":1}]}"#;

#[test]
fn a_racing_delta_is_parked_and_lands_once_the_command_succeeds() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));
    app.handle(Event::Synced(Ok(delta(RENAMED_MILK)))).unwrap();
    assert_eq!(app.state().sync_token, "fixture-token-2");
    assert_eq!(task(&app, "t-milk").content, "Buy oat milk");

    let answer = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-3",
            "sync_status":{{"{}":"ok"}}}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Ok(delta(&answer)),
    })
    .unwrap();

    assert_eq!(task(&app, "t-milk").content, "Buy oat milk and bread");
}

#[test]
fn a_parked_delta_wins_over_the_rollback_snapshot() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));
    app.handle(Event::Synced(Ok(delta(RENAMED_MILK)))).unwrap();

    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Err(ClientError::Network("offline".to_owned())),
    })
    .unwrap();

    let milk = task(&app, "t-milk");
    assert_eq!(milk.content, "Buy oat milk and bread");
    assert!(!milk.checked);
    let reason = app.state().rollback_reason("t-milk").map(|f| &f.kind);
    assert_eq!(reason, Some(&ErrorKind::Offline));
}

#[test]
fn a_rejected_write_rolls_back_with_the_validation_kind() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    let rejection = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-2",
            "sync_status":{{"{}":{{"error_code":22,"error":"Item not found"}}}}}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Ok(delta(&rejection)),
    })
    .unwrap();

    let reason = app.state().rollback_reason("t-milk").map(|f| &f.kind);
    assert_eq!(reason, Some(&ErrorKind::Validation));
}

#[test]
fn a_new_task_s_temp_id_is_a_version_4_uuid() {
    let mut app = app();
    send(&mut app, [key('1'), key('a')]);
    send(&mut app, "Pay rent".chars().map(key));
    let command = single_command(&send(&mut app, [press(KeyCode::Enter)]));

    let temp_id = command.temp_id.unwrap();
    assert_eq!(temp_id.len(), 36);
    assert_eq!(&temp_id[14..15], "4");
    assert!(matches!(&temp_id[19..20], "8" | "9" | "a" | "b"));
    assert_ne!(temp_id, command.uuid);
}

#[test]
fn a_sync_that_brought_the_new_task_first_leaves_one_copy() {
    let mut app = app();
    send(&mut app, [key('1'), key('a')]);
    send(&mut app, "Pay rent".chars().map(key));
    let command = single_command(&send(&mut app, [press(KeyCode::Enter)]));
    let temp_id = command.temp_id.clone().unwrap();
    app.handle(Event::Synced(Ok(delta(
        r#"{"full_sync":false,"sync_token":"fixture-token-2",
            "items":[{"id":"t-rent","content":"Pay rent","checked":false,"priority":1,
              "project_id":"p-inbox","child_order":9}]}"#,
    ))))
    .unwrap();

    let confirmation = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-3",
            "sync_status":{{"{}":"ok"}},"temp_id_mapping":{{"{temp_id}":"t-rent"}}}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Ok(delta(&confirmation)),
    })
    .unwrap();

    let copies = app
        .state()
        .store
        .tasks
        .iter()
        .filter(|t| t.content == "Pay rent")
        .count();
    assert_eq!(copies, 1);
}

#[test]
fn a_confirmed_command_maps_the_temp_id_of_a_new_task() {
    let mut app = app();
    send(&mut app, [key('1'), key('a')]);
    send(&mut app, "Pay rent".chars().map(key));
    let command = single_command(&send(&mut app, [press(KeyCode::Enter)]));
    assert_eq!(command.r#type, "item_add");
    assert_eq!(command.args["content"], "Pay rent");
    assert_eq!(command.args["project_id"], "p-inbox");
    let temp_id = command.temp_id.clone().unwrap();
    assert!(shows(&app, "Pay rent"));

    let confirmation = format!(
        r#"{{"full_sync":false,"sync_token":"fixture-token-2",
            "sync_status":{{"{}":"ok"}},"temp_id_mapping":{{"{temp_id}":"t-rent"}}}}"#,
        command.uuid
    );
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Ok(delta(&confirmation)),
    })
    .unwrap();

    assert_eq!(task(&app, "t-rent").content, "Pay rent");
    assert!(!app.state().is_pending(&command.uuid));
}

#[test]
fn a_failed_add_removes_the_optimistic_task() {
    let mut app = app();
    send(&mut app, [key('1'), key('a')]);
    send(&mut app, "Pay rent".chars().map(key));
    let command = single_command(&send(&mut app, [press(KeyCode::Enter)]));

    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Err(ClientError::Network("offline".to_owned())),
    })
    .unwrap();

    assert!(
        app.state()
            .store
            .tasks
            .iter()
            .all(|t| t.content != "Pay rent")
    );
}

#[test]
fn setting_a_priority_sends_one_item_update() {
    let mut app = app();
    send(&mut app, [key('1'), key('p')]);
    assert!(matches!(
        app.state().modals.as_slice(),
        [Modal::Picker(picker)] if picker.kind == PickerKind::Priority
    ));

    let command = single_command(&send(&mut app, [key('3')]));

    assert_eq!(command.r#type, "item_update");
    assert_eq!(command.args["priority"], 2);
    assert_eq!(task(&app, "t-milk").priority, 2);
    assert!(app.state().modals.is_empty());
}

#[test]
fn a_comment_shows_at_once_and_goes_out_as_note_add() {
    let mut app = app();
    let effects = send(&mut app, [key('1'), key('j'), press(KeyCode::Enter)]);
    assert!(effects.contains(&Effect::FetchComments {
        task_id: "t-call".to_owned(),
        fetch_seq: 1,
    }));
    send(&mut app, [key('c')]);
    send(&mut app, "On my way".chars().map(key));

    let command = single_command(&send(&mut app, [press(KeyCode::Enter)]));

    assert_eq!(command.r#type, "note_add");
    assert_eq!(command.args["item_id"], "t-call");
    assert!(shows(&app, "On my way"), "{:#?}", screen(&app));
}

#[test]
fn comments_from_an_older_fetch_do_not_replace_the_open_ones() {
    let mut app = app();
    send(&mut app, [key('1'), key('j'), press(KeyCode::Enter)]);

    app.handle(Event::Comments {
        task_id: "t-call".to_owned(),
        fetch_seq: 0,
        result: Ok(Vec::new()),
    })
    .unwrap();

    assert!(shows(&app, "Their office opens at nine."));
}

#[test]
fn force_resync_drops_pending_writes_and_reads_everything() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('1'), key('x')]));

    let effects = send(&mut app, [key('R'), key('y')]);

    assert!(!app.state().is_pending(&command.uuid));
    assert_eq!(app.state().sync_token, "*");
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Sync(request) if request.sync_token == "*"))
    );
}

/// How many syncs `effects` ask for.
fn sync_count(effects: &[Effect]) -> usize {
    syncs(effects).len()
}

fn live(message: LiveMessage) -> Event {
    Event::Live(message)
}

#[test]
fn ten_live_changes_in_a_second_give_one_sync_after_the_debounce() {
    let mut app = app();
    send(&mut app, [live(LiveMessage::Connected)]);

    let burst = send(&mut app, (0..10).map(|_| live(LiveMessage::Changed)));
    let due = send(&mut app, [Event::LiveSyncDue]);

    assert_eq!(
        burst,
        vec![Effect::ScheduleLiveSync {
            after: Duration::from_secs(2)
        }]
    );
    let [Effect::Sync(request)] = due.as_slice() else {
        panic!("{due:?}");
    };
    assert_eq!(request.sync_token, "fixture-token-1");
}

#[test]
fn a_steady_stream_of_live_changes_still_syncs_once_per_debounce() {
    let mut app = app();
    send(&mut app, [live(LiveMessage::Connected)]);

    let mut synced = 0;
    for _ in 0..3 {
        let scheduled = send(&mut app, (0..5).map(|_| live(LiveMessage::Changed)));
        assert_eq!(scheduled.len(), 1, "{scheduled:?}");
        synced += sync_count(&send(&mut app, [Event::LiveSyncDue]));
    }

    assert_eq!(synced, 3);
}

#[test]
fn a_dropped_live_connection_polls_every_minute() {
    let mut app = app();
    send(
        &mut app,
        [
            live(LiveMessage::Connected),
            live(LiveMessage::Disconnected),
        ],
    );

    let polls: Vec<_> = [1, 30, 61, 90, 121]
        .map(|secs| sync_count(&send(&mut app, [tick(secs)])))
        .into();

    assert_eq!(polls, vec![0, 0, 1, 0, 1]);
}

#[test]
fn a_live_connection_that_never_came_up_polls_too() {
    let mut app = app();

    send(&mut app, [tick(1)]);
    let polled = send(&mut app, [tick(61)]);

    assert_eq!(sync_count(&polled), 1);
}

#[test]
fn a_live_connection_that_is_up_polls_nothing() {
    let mut app = app();
    send(&mut app, [live(LiveMessage::Connected)]);

    let effects = send(&mut app, (1..=200).map(tick));

    assert_eq!(sync_count(&effects), 0);
}

#[test]
fn reconnecting_catches_up_once_and_stops_polling() {
    let mut app = app();
    send(
        &mut app,
        [
            live(LiveMessage::Connected),
            live(LiveMessage::Disconnected),
            tick(1),
        ],
    );

    let reconnected = send(&mut app, [live(LiveMessage::Connected)]);
    let later = send(&mut app, [tick(61), tick(121)]);

    assert_eq!(
        reconnected,
        vec![Effect::ScheduleLiveSync {
            after: Duration::from_secs(2)
        }]
    );
    assert_eq!(sync_count(&later), 0);
}

#[test]
fn nothing_polls_before_the_account_loads() {
    let mut app = App::new(TestBackend::new(120, 36), State::new(now())).unwrap();

    let effects = send(&mut app, [tick(1), tick(61), tick(121)]);

    assert_eq!(sync_count(&effects), 0);
}

#[test]
fn idle_pauses_polling_and_live_syncs_until_the_next_key() {
    let mut app = app();
    send(&mut app, [live(LiveMessage::Connected)]);
    send(&mut app, [live(LiveMessage::Changed)]);
    send(&mut app, [live(LiveMessage::Disconnected), tick(6 * 60)]);
    assert!(shows(&app, "idle, synced 6m ago"), "{:#?}", screen(&app));

    let while_idle = send(
        &mut app,
        [
            Event::LiveSyncDue,
            live(LiveMessage::Changed),
            tick(7 * 60),
            tick(9 * 60),
        ],
    );
    let on_key = send(&mut app, [key('y')]);

    assert_eq!(sync_count(&while_idle), 0, "{while_idle:?}");
    assert!(
        !while_idle
            .iter()
            .any(|e| matches!(e, Effect::ScheduleLiveSync { .. })),
        "{while_idle:?}"
    );
    assert_eq!(sync_count(&on_key), 1, "{on_key:?}");
    assert!(shows(&app, "live, synced 9m ago"));
}

#[test]
fn the_first_key_after_idle_syncs_even_without_live_changes() {
    let mut app = app();
    send(&mut app, [live(LiveMessage::Connected), tick(6 * 60)]);

    let on_key = send(&mut app, [key('y')]);
    let next_key = send(&mut app, [key('y')]);

    assert_eq!(sync_count(&on_key), 1, "{on_key:?}");
    assert_eq!(sync_count(&next_key), 0, "{next_key:?}");
}

#[test]
fn the_indicator_says_how_changes_arrive() {
    let mut app = app();
    assert!(shows(&app, "polling, synced 0s ago"), "{:#?}", screen(&app));

    send(&mut app, [live(LiveMessage::Connected)]);
    assert!(shows(&app, "live, synced 0s ago"), "{:#?}", screen(&app));

    send(&mut app, [live(LiveMessage::Disconnected)]);
    assert!(shows(&app, "polling, synced 0s ago"), "{:#?}", screen(&app));

    send(
        &mut app,
        [Event::Synced(Err(ClientError::Network(
            "connection refused".to_owned(),
        )))],
    );
    let indicator = app.state().indicator().unwrap();
    assert_eq!(indicator.text, "offline");
    assert!(indicator.alert);
}

#[test]
fn the_z_chord_folds_the_subtasks() {
    let mut app = app();
    send(&mut app, [key('1'), key('j'), key('j'), key('j')]);
    assert!(shows(&app, "Book the train"));

    send(&mut app, [key('z'), key('a')]);

    let listed = app.state().visible_tasks();
    assert!(listed.iter().all(|task| task.content != "Book the train"));
    assert!(app.state().modals.is_empty(), "the chord is over");
}

#[test]
fn the_standard_preset_completes_with_ctrl_x() {
    let mut app = configured("[keys]\npreset = \"standard\"\n");

    let effects = send(&mut app, [key('1'), ctrl('x')]);

    assert_eq!(single_command(&effects).r#type, "item_close");
}

#[test]
fn choosing_a_theme_saves_only_the_theme() {
    let mut app = configured(THEME_ON_COMMA);
    send(&mut app, [key(','), press(KeyCode::Down)]);

    let effects = send(&mut app, [press(KeyCode::Enter)]);

    assert!(matches!(effects.as_slice(), [Effect::SaveTheme(_)]));
    assert_ne!(app.state().theme_idx, 0);
}

#[test]
fn there_is_no_settings_pane_and_nothing_saves_settings() {
    let mut app = app();

    let effects = send(&mut app, [key(','), key('j'), press(KeyCode::Enter)]);

    assert!(!shows(&app, "Settings"));
    assert!(effects.iter().all(|effect| !matches!(effect, Effect::Quit)));
    assert_eq!(app.state().theme_idx, 0);
}

#[test]
fn the_completed_filter_asks_for_the_project_s_completed_tasks() {
    let mut app = app();

    let effects = send(&mut app, [key('1'), key('f'), key('2')]);

    assert!(matches!(
        effects.as_slice(),
        [Effect::FetchCompletedTasks(query)] if query.project_id.as_deref() == Some("p-inbox")
            && query.window == CompletedWindow::latest(now().date())
    ));
}

fn completed(id: &str, content: &str) -> Task {
    Task {
        id: id.to_owned(),
        content: content.to_owned(),
        project_id: "p-inbox".to_owned(),
        checked: true,
        ..Task::default()
    }
}

fn run_from_palette(app: &mut App<TestBackend>, query: &str) -> Vec<Effect> {
    let mut events = vec![key(':')];
    events.extend(query.chars().map(key));
    events.push(press(KeyCode::Enter));
    send(app, events)
}

#[test]
fn load_older_completed_walks_back_one_window_at_a_time() {
    let mut app = app();
    let latest = CompletedWindow::latest(now().date());
    let Effect::FetchCompletedTasks(first) = send(&mut app, [key('1'), key('f'), key('2')])
        .pop()
        .unwrap()
    else {
        panic!("the completed filter asks for the completed tasks");
    };
    assert_eq!(run_from_palette(&mut app, "load older"), Vec::new());

    send(
        &mut app,
        [Event::CompletedTasks {
            query: first,
            result: Ok(vec![completed("t-new", "Done this month")]),
        }],
    );
    let effects = run_from_palette(&mut app, "load older");

    let [Effect::FetchCompletedTasks(older)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(older.project_id.as_deref(), Some("p-inbox"));
    assert_eq!(older.window, latest.before());
    assert_eq!(older.window.until, latest.since);

    send(
        &mut app,
        [Event::CompletedTasks {
            query: older.clone(),
            result: Ok(vec![
                completed("t-old", "Done last summer"),
                completed("t-new", "Done this month"),
            ]),
        }],
    );

    let cached = &app.state().completed_cache["p-inbox"];
    let ids: Vec<&str> = cached.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["t-new", "t-old"]);
    assert!(shows(&app, "Done last summer"));
    assert!(shows(
        &app,
        &format!("Completed tasks loaded back to {}", older.window.since)
    ));
    let next = run_from_palette(&mut app, "load older");
    assert!(matches!(
        next.as_slice(),
        [Effect::FetchCompletedTasks(query)] if query.window == older.window.before()
    ));
}

fn tick(after_secs: i64) -> Event {
    Event::Tick {
        now: now() + SignedDuration::from_secs(after_secs),
    }
}

fn cache_saves(effects: &[Effect]) -> usize {
    effects.iter().filter(|e| **e == Effect::SaveCache).count()
}

fn empty_delta(token: &str) -> Event {
    Event::Synced(Ok(delta(&format!(
        r#"{{"full_sync": false, "sync_token": "{token}"}}"#
    ))))
}

/// The sync tokens of the syncs among `effects`.
fn syncs(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Sync(request) => Some(request.sync_token.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_burst_of_merges_writes_the_cache_once_after_the_debounce() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    let mut effects = send(
        &mut app,
        [
            Event::Synced(Ok(full_sync())),
            empty_delta("t2"),
            empty_delta("t3"),
        ],
    );
    assert_eq!(cache_saves(&effects), 0, "nothing is written at once");
    assert_eq!(cache_saves(&send(&mut app, [tick(1)])), 0, "nor within 2 s");

    effects.extend(send(&mut app, [tick(2), tick(3), tick(10)]));

    assert_eq!(cache_saves(&effects), 1);
}

#[test]
fn the_cache_waits_for_unanswered_commands() {
    let mut app = app();
    send(&mut app, [tick(3)]);
    send(&mut app, [key('1'), key('x'), empty_delta("t2")]);

    let effects = send(&mut app, [tick(10)]);

    assert_eq!(cache_saves(&effects), 0);
    assert_eq!(app.state().unsaved_cache(), None);
}

#[test]
fn a_dated_full_sync_is_followed_by_one_incremental_sync() {
    let mut dated: serde_json::Value = serde_json::from_str(FULL_SYNC).unwrap();
    dated["full_sync_date_utc"] = "2026-11-20T09:00:00Z".into();
    let dated: Box<SyncResponse> = Box::new(serde_json::from_value(dated).unwrap());

    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    let after_dated = app.handle(Event::Synced(Ok(dated))).unwrap();
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    let after_undated = app.handle(Event::Synced(Ok(full_sync()))).unwrap();

    assert_eq!(syncs(&after_dated), ["fixture-token-1"]);
    assert!(syncs(&after_undated).is_empty());
}

#[test]
fn a_start_from_the_cache_shows_its_tasks_and_asks_for_what_changed() {
    let cached = sized(80, 24).state().store.clone();
    let mut state = State::new(now());
    state.paint_from_cache(cached, "cached-token".to_owned(), "u-1".to_owned());
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    assert!(!shows(&app, "Loading your tasks"));

    let effects = app.handle(Event::Start).unwrap();

    let [Effect::Sync(request)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.sync_token, "cached-token");
    assert!(request.resource_types.contains(&"user".to_owned()));
}

#[test]
fn a_first_answer_from_another_user_discards_the_cache() {
    let cached = sized(80, 24).state().store.clone();
    let mut state = State::new(now());
    state.paint_from_cache(cached, "cached-token".to_owned(), "someone-else".to_owned());
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();

    let effects = app
        .handle(Event::Synced(Ok(delta(
            r#"{"full_sync": false, "sync_token": "t2", "user": {"id": "u-1"}}"#,
        ))))
        .unwrap();

    assert!(app.state().store.tasks.is_empty());
    assert!(shows(&app, "Loading your tasks"));
    assert_eq!(syncs(&effects), ["*"]);
}

#[test]
fn r_asks_for_the_cache_to_be_rewritten() {
    let mut app = app();
    send(&mut app, [tick(3)]);

    send(&mut app, [key('R')]);
    let effects = send(&mut app, [tick(10)]);

    assert_eq!(app.state().sync_token, "*");
    assert_eq!(cache_saves(&effects), 1);
}

/// The app painted from a cache of the fixture's store for `user_id`.
fn from_cache(user_id: &str) -> App<TestBackend> {
    let cached = sized(80, 24).state().store.clone();
    let mut state = State::new(now());
    state.paint_from_cache(cached, "cached-token".to_owned(), user_id.to_owned());
    App::new(TestBackend::new(120, 36), state).unwrap()
}

fn user(id: &str) -> ratatoist::todoist::model::UserInfo {
    ratatoist::todoist::model::UserInfo {
        id: id.to_owned(),
        websocket_url: Some("wss://live.example.invalid/ws".to_owned()),
        ..Default::default()
    }
}

fn fetches_of_the_user(effects: &[Effect]) -> usize {
    effects.iter().filter(|e| **e == Effect::FetchUser).count()
}

#[test]
fn a_first_answer_without_the_user_asks_for_it_once_then_opens_live() {
    let mut app = from_cache("u-1");

    let mut effects = send(&mut app, [empty_delta("t2"), empty_delta("t3")]);
    assert_eq!(fetches_of_the_user(&effects), 1);
    assert!(!effects.iter().any(|e| matches!(e, Effect::OpenLive { .. })));

    effects = send(&mut app, [Event::UserFetched(Ok(user("u-1")))]);

    assert_eq!(
        effects,
        [Effect::OpenLive {
            url: "wss://live.example.invalid/ws".to_owned()
        }]
    );
    assert!(!app.state().store.tasks.is_empty(), "the cached tasks stay");
}

#[test]
fn a_first_answer_with_the_user_does_not_ask_for_it() {
    let mut app = from_cache("u-1");

    let effects = app
        .handle(Event::Synced(Ok(delta(
            r#"{"full_sync": false, "sync_token": "t2", "user": {"id": "u-1"}}"#,
        ))))
        .unwrap();

    assert_eq!(fetches_of_the_user(&effects), 0);
}

#[test]
fn another_user_from_the_fetch_discards_the_cache() {
    let mut app = from_cache("someone-else");
    send(&mut app, [empty_delta("t2")]);

    let effects = send(&mut app, [Event::UserFetched(Ok(user("u-1")))]);

    assert!(app.state().store.tasks.is_empty());
    assert!(shows(&app, "Loading your tasks"));
    assert_eq!(syncs(&effects), ["*"]);
}

#[test]
fn a_failed_fetch_of_the_user_is_logged_and_keeps_the_cached_tasks() {
    let mut app = from_cache("u-1");
    send(&mut app, [empty_delta("t2")]);

    let effects = send(
        &mut app,
        [Event::UserFetched(Err(ClientError::Network(
            "offline".to_owned(),
        )))],
    );

    assert!(effects.is_empty());
    assert_eq!(app.state().failures.len(), 1);
    assert!(!app.state().store.tasks.is_empty());
    let later = send(&mut app, [empty_delta("t3")]);
    assert_eq!(fetches_of_the_user(&later), 0);
    assert_eq!(app.state().sync_token, "t3");
}

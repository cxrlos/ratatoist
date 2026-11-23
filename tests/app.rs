#![cfg(test)]

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Effect, Event, Modal, Pane, State};
use ratatoist::todoist::sync::{SyncCommand, SyncResponse};
use ratatoist::todoist::{ClientError, LiveMessage};
use ratatui::backend::TestBackend;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
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
    app.state().tasks.iter().find(|t| t.id == id).unwrap()
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
fn shows_the_splash_until_the_first_sync_answers() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();
    assert!(shows(&app, "syncing data..."));

    let effects = app.handle(Event::Start).unwrap();

    let [Effect::Sync(request)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.sync_token, "*");
    assert!(request.resource_types.contains(&"user".to_owned()));
    app.handle(Event::Synced(Ok(full_sync()))).unwrap();
    assert!(shows(&app, "Projects"));
}

#[test]
fn the_first_sync_opens_the_live_connection_and_saves_the_token() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();

    let effects = app.handle(Event::Synced(Ok(full_sync()))).unwrap();

    assert_eq!(
        effects,
        [
            Effect::PersistSyncToken("fixture-token-1".to_owned()),
            Effect::OpenLive {
                url: "wss://live.example.invalid/ws".to_owned()
            },
        ]
    );
}

#[test]
fn a_failed_first_sync_shows_the_error_over_the_empty_screen() {
    let mut app = App::new(TestBackend::new(80, 24), State::new(now())).unwrap();

    app.handle(Event::Synced(Err(ClientError::Network(
        "offline".to_owned(),
    ))))
    .unwrap();

    assert!(shows(&app, "Initial sync failed"), "{:#?}", screen(&app));
}

#[test]
fn main_layout_with_the_inbox_selected_at_120x36() {
    let app = sized(120, 36);

    assert_eq!(
        app.state().projects[app.state().selected_project].name,
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

    send(&mut app, [key('l'), key('j'), press(KeyCode::Enter)]);

    assert_eq!(app.state().active_pane, Pane::Detail);
    assert_snapshot!(app.backend());
}

#[test]
fn task_detail_at_80x24() {
    let mut app = sized(80, 24);

    send(&mut app, [key('l'), key('j'), press(KeyCode::Enter)]);

    assert_snapshot!(app.backend());
}

#[test]
fn cheatsheet_at_120x36() {
    let mut app = sized(120, 36);

    send(&mut app, [key('?')]);

    assert_eq!(app.state().modals, [Modal::Help]);
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
    assert_eq!(app.state().active_pane, Pane::Projects);

    send(&mut app, [press(KeyCode::Esc)]);

    assert!(app.state().modals.is_empty());
}

#[test]
fn draws_nothing_without_events() {
    let app = app();
    let frames = app.frames_drawn();

    assert_eq!(app.frames_drawn(), frames);
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
    let frames = app.frames_drawn();

    send(
        &mut app,
        [Event::Tick {
            now: now() + chrono::Duration::seconds(1),
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

    let effects = send(&mut app, [key('l'), key('x')]);

    let command = single_command(&effects);
    assert_eq!(command.r#type, "item_close");
    assert_eq!(command.args["id"], "t-milk");
    assert!(task(&app, "t-milk").checked);
    assert!(!shows(&app, "Buy oat milk"), "the Active filter hides it");
}

#[test]
fn completing_a_recurring_task_sends_item_complete() {
    let mut app = app();

    let effects = send(&mut app, [key('l'), key('j'), key('j'), key('x')]);

    let command = single_command(&effects);
    assert_eq!(command.args["id"], "t-plants");
    assert_eq!(command.r#type, "item_complete");
}

#[test]
fn a_failed_flush_restores_the_completed_task_and_says_so() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('l'), key('x')]));

    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid.clone()],
        result: Err(ClientError::Network("connection reset".to_owned())),
    })
    .unwrap();

    assert!(!task(&app, "t-milk").checked);
    assert!(!app.state().is_pending(&command.uuid));
    assert!(shows(&app, "Sync failed"), "{:#?}", screen(&app));
    assert!(shows(&app, "Buy oat milk"));
}

#[test]
fn a_rejected_command_restores_the_task_and_shows_todoist_s_message() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('l'), key('x')]));

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
    assert!(shows(&app, "Item not found"), "{:#?}", screen(&app));
}

#[test]
fn any_key_dismisses_the_error_and_does_nothing_else() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('l'), key('x')]));
    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Err(ClientError::Network("offline".to_owned())),
    })
    .unwrap();

    let effects = send(&mut app, [key('q')]);

    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.state().error.is_none());
    assert!(!shows(&app, "Sync failed"));
}

#[test]
fn a_racing_delta_does_not_clobber_a_task_with_a_pending_command() {
    let mut app = app();
    let command = single_command(&send(&mut app, [key('l'), key('x')]));

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

#[test]
fn a_confirmed_command_maps_the_temp_id_of_a_new_task() {
    let mut app = app();
    send(&mut app, [key('l'), key('a')]);
    send(&mut app, "Pay rent".chars().map(key));
    send(&mut app, [press(KeyCode::Enter)]);
    let command = single_command(&send(&mut app, [press(KeyCode::Tab)]));
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
    send(&mut app, [key('l'), key('a')]);
    send(&mut app, "Pay rent".chars().map(key));
    send(&mut app, [press(KeyCode::Enter)]);
    let command = single_command(&send(&mut app, [press(KeyCode::Tab)]));

    app.handle(Event::CommandsSent {
        uuids: vec![command.uuid],
        result: Err(ClientError::Network("offline".to_owned())),
    })
    .unwrap();

    assert!(app.state().tasks.iter().all(|t| t.content != "Pay rent"));
}

#[test]
fn setting_a_priority_from_the_detail_sends_one_item_update() {
    let mut app = app();
    send(
        &mut app,
        [key('l'), press(KeyCode::Enter), key('j'), key('i')],
    );
    assert!(matches!(
        app.state().modals.as_slice(),
        [Modal::PriorityPicker { selection: 4 }]
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
    let effects = send(&mut app, [key('l'), key('j'), press(KeyCode::Enter)]);
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
    send(&mut app, [key('l'), key('j'), press(KeyCode::Enter)]);

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
    let command = single_command(&send(&mut app, [key('l'), key('x')]));

    let effects = send(&mut app, [key('R')]);

    assert!(!app.state().is_pending(&command.uuid));
    assert!(effects.contains(&Effect::PersistSyncToken("*".to_owned())));
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Sync(request) if request.sync_token == "*"))
    );
}

#[test]
fn a_live_change_syncs_from_the_last_token() {
    let mut app = app();

    let effects = send(&mut app, [Event::Live(LiveMessage::Changed)]);

    let [Effect::Sync(request)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.sync_token, "fixture-token-1");
    assert!(shows(&app, "Connected"));
}

#[test]
fn a_live_change_while_idle_waits_for_the_next_key() {
    let mut app = app();
    send(&mut app, [Event::Live(LiveMessage::Connected)]);
    send(
        &mut app,
        [Event::Tick {
            now: now() + chrono::Duration::minutes(6),
        }],
    );
    assert!(shows(&app, "Idle"), "{:#?}", screen(&app));

    let while_idle = send(&mut app, [Event::Live(LiveMessage::Changed)]);
    let on_key = send(&mut app, [key('y')]);

    assert!(while_idle.is_empty(), "{while_idle:?}");
    assert!(matches!(on_key.as_slice(), [Effect::Sync(_)]), "{on_key:?}");
    assert!(shows(&app, "Connected"));
}

#[test]
fn the_z_chord_folds_the_subtasks() {
    let mut app = app();
    send(&mut app, [key('l'), key('j'), key('j'), key('j')]);
    assert!(shows(&app, "Book the train"));

    send(&mut app, [key('z'), key('a')]);

    assert!(!shows(&app, "Book the train"));
    assert!(!app.state().pending_z);
}

#[test]
fn standard_mode_completes_with_ctrl_x() {
    let mut app = app();
    send(
        &mut app,
        [key(','), press(KeyCode::Enter), press(KeyCode::Esc)],
    );
    assert!(shows(&app, "STANDARD"));

    let effects = send(&mut app, [press(KeyCode::Tab), ctrl('x')]);

    assert_eq!(single_command(&effects).r#type, "item_close");
}

#[test]
fn choosing_a_theme_saves_the_ui_settings() {
    let mut app = app();
    send(
        &mut app,
        [key(','), key('j'), press(KeyCode::Enter), key('j')],
    );

    let effects = send(&mut app, [press(KeyCode::Enter)]);

    assert_eq!(
        effects,
        [Effect::SaveUiSettings {
            theme: app.state().theme().name.clone(),
            idle_timeout_secs: 300,
        }]
    );
    assert_ne!(app.state().theme_idx, 0);
}

#[test]
fn the_done_filter_asks_for_the_project_s_completed_tasks() {
    let mut app = app();

    let effects = send(&mut app, [key('l'), key('f')]);

    assert!(matches!(
        effects.as_slice(),
        [Effect::FetchCompletedTasks(query)] if query.project_id.as_deref() == Some("p-inbox")
    ));
}

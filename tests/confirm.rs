#![cfg(test)]

//! `R` and `q` ask first while commands are not sent yet, and run at once
//! when nothing is pending. Commands stay unanswered because the tests
//! never deliver `Event::CommandsSent`, as a client holding them would.

use chrono::{Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Confirmed, Effect, Event, Modal, State};
use ratatoist::todoist::demo;
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;

/// The demo account at `columns` x `rows`, on the Inbox's task list.
fn demo_at(columns: u16, rows: u16) -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now)).unwrap();
    let sync: SyncResponse = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    send(&mut app, [enter()]);
    app
}

fn demo() -> App<TestBackend> {
    demo_at(80, 24)
}

/// The demo with `count` completions sent and not answered, of the tasks at
/// the bottom of the list so the rows above stay put.
fn pending(columns: u16, rows: u16, count: usize) -> App<TestBackend> {
    let mut app = demo_at(columns, rows);
    send(&mut app, [key('G')]);
    for _ in 0..count {
        let effects = send(&mut app, [key('x')]);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::SendCommands(_))),
            "completing a task sends at once"
        );
        assert!(app.state().modals.is_empty(), "completing asks nothing");
    }
    app
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn enter() -> Event {
    press(KeyCode::Enter)
}

fn esc() -> Event {
    press(KeyCode::Esc)
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
    events
        .into_iter()
        .flat_map(|event| app.handle(event).unwrap())
        .collect()
}

fn resyncs(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::Sync(request) if request.sync_token == "*"))
}

fn quits(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| matches!(effect, Effect::Quit))
}

fn confirmed(app: &App<TestBackend>) -> Option<Confirmed> {
    match app.state().top_modal() {
        Some(Modal::Confirm(confirm)) => Some(confirm.confirmed),
        _ => None,
    }
}

#[test]
fn r_with_nothing_pending_resyncs_at_once() {
    let mut app = demo();

    let effects = send(&mut app, [key('R')]);

    assert!(resyncs(&effects));
    assert!(app.state().modals.is_empty());
}

#[test]
fn q_with_nothing_pending_quits_at_once() {
    let mut app = demo();

    let effects = send(&mut app, [key('q')]);

    assert!(quits(&effects));
    assert!(app.state().modals.is_empty());
}

#[test]
fn r_with_pending_commands_asks_first() {
    let mut app = pending(80, 24, 2);

    let effects = send(&mut app, [key('R')]);

    assert!(!resyncs(&effects));
    assert_eq!(confirmed(&app), Some(Confirmed::Resync));
    let Some(Modal::Confirm(confirm)) = app.state().top_modal() else {
        panic!("a confirmation is on top");
    };
    assert_eq!(confirm.title, "Re-sync everything?");
    assert_eq!(
        confirm.consequences,
        ["2 changes are not sent yet and will be dropped."]
    );
}

#[test]
fn y_and_enter_run_the_resync() {
    for yes in [key('y'), enter()] {
        let mut app = pending(80, 24, 1);
        send(&mut app, [key('R')]);

        let effects = send(&mut app, [yes]);

        assert!(resyncs(&effects));
        assert!(app.state().modals.is_empty());
    }
}

#[test]
fn n_and_esc_cancel_the_resync_and_keep_the_pending_commands() {
    for no in [key('n'), esc()] {
        let mut app = pending(80, 24, 1);
        send(&mut app, [key('R')]);

        let effects = send(&mut app, [no]);

        assert!(effects.is_empty());
        assert!(app.state().modals.is_empty());
        let effects = send(&mut app, [key('R')]);
        assert!(!resyncs(&effects), "the command is still pending");
    }
}

#[test]
fn q_with_pending_commands_asks_first() {
    let mut app = pending(80, 24, 1);

    let effects = send(&mut app, [key('q')]);

    assert!(!quits(&effects));
    let Some(Modal::Confirm(confirm)) = app.state().top_modal() else {
        panic!("a confirmation is on top");
    };
    assert_eq!(confirm.title, "Quit with unsent changes?");
    assert_eq!(confirm.consequences, ["1 change is not sent yet."]);
    assert_eq!(confirm.confirmed, Confirmed::Quit);
}

#[test]
fn y_and_enter_quit() {
    for yes in [key('y'), enter()] {
        let mut app = pending(80, 24, 1);
        send(&mut app, [key('q')]);

        let effects = send(&mut app, [yes]);

        assert!(quits(&effects));
    }
}

#[test]
fn n_and_esc_stay() {
    for no in [key('n'), esc()] {
        let mut app = pending(80, 24, 1);
        send(&mut app, [key('q')]);

        let effects = send(&mut app, [no]);

        assert!(!quits(&effects));
        assert!(app.state().modals.is_empty());
    }
}

#[test]
fn an_answered_command_no_longer_counts_as_pending() {
    let mut app = demo();
    let effects = send(&mut app, [key('x')]);
    let uuids: Vec<String> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendCommands(commands) => Some(commands.iter().map(|c| c.uuid.clone())),
            _ => None,
        })
        .flatten()
        .collect();
    let status: serde_json::Map<String, serde_json::Value> = uuids
        .iter()
        .map(|uuid| (uuid.clone(), serde_json::json!("ok")))
        .collect();
    let answer: SyncResponse = serde_json::from_value(serde_json::json!({
        "full_sync": false,
        "sync_token": "answered",
        "sync_status": status,
    }))
    .unwrap();
    app.handle(Event::CommandsSent {
        uuids,
        result: Ok(Box::new(answer)),
    })
    .unwrap();

    let effects = send(&mut app, [key('q')]);

    assert!(quits(&effects));
}

#[test]
fn the_resync_confirm_at_80x24() {
    let mut app = pending(80, 24, 2);
    send(&mut app, [key('R')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_resync_confirm_at_120x36() {
    let mut app = pending(120, 36, 2);
    send(&mut app, [key('R')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_quit_confirm_at_80x24() {
    let mut app = pending(80, 24, 1);
    send(&mut app, [key('q')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_quit_confirm_at_120x36() {
    let mut app = pending(120, 36, 1);
    send(&mut app, [key('q')]);
    assert_snapshot!(app.backend());
}

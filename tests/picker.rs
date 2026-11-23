#![cfg(test)]

//! The one picker, driven through each of its openers on the demo account:
//! typing filters, 1 to 9 choose, and `✓` marks the value in force.

use chrono::{Local, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use ratatoist::app::{App, Choice, Effect, Event, ListFilter, Modal, Picker, Sort, State};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::demo;
use ratatoist::todoist::model::Priority;
use ratatoist::todoist::sync::{SyncCommand, SyncResponse};
use ratatui::backend::TestBackend;

/// `,` runs choose-theme, which has no default key; the palette runs it
/// otherwise.
const THEME_ON_COMMA: &str = "[keys.global]\nchoose-theme = \",\"\n";

/// The demo account at `columns` x `rows` with `config` as `config.toml`,
/// on the Inbox's task list.
fn demo_with(columns: u16, rows: u16, config: &str) -> App<TestBackend> {
    let now = Local.from_local_datetime(&demo::NOW).earliest().unwrap();
    let config = ratatoist::config::parse(config, &[]);
    let state = initial_state_from(&config, &UiSettings::default(), "*".to_owned(), now, false);
    let mut app = App::new(TestBackend::new(columns, rows), state).unwrap();
    let sync: SyncResponse = serde_json::from_value(demo::account(demo::NOW.date()).sync).unwrap();
    app.handle(Event::Synced(Ok(Box::new(sync)))).unwrap();
    send(&mut app, [key('1')]);
    app
}

fn demo_at(columns: u16, rows: u16) -> App<TestBackend> {
    demo_with(columns, rows, "")
}

fn demo() -> App<TestBackend> {
    demo_at(80, 24)
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

fn typed(text: &str) -> Vec<Event> {
    text.chars().map(key).collect()
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
    events
        .into_iter()
        .flat_map(|event| app.handle(event).unwrap())
        .collect()
}

fn commands(effects: &[Effect]) -> Vec<SyncCommand> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendCommands(commands) => Some(commands.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn picker(app: &App<TestBackend>) -> &Picker {
    match app.state().top_modal() {
        Some(Modal::Picker(picker)) => picker,
        _ => panic!("no picker on top"),
    }
}

/// The labels the picker's filter leaves, in order.
fn labels(app: &App<TestBackend>) -> Vec<String> {
    picker(app)
        .visible()
        .iter()
        .map(|(_, entry)| entry.label.clone())
        .collect()
}

/// The label the picker marks with `✓`.
fn current(app: &App<TestBackend>) -> Option<String> {
    let picker = picker(app);
    picker
        .visible()
        .iter()
        .find(|(index, _)| picker.is_current(*index))
        .map(|(_, entry)| entry.label.clone())
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

/// The screen line that carries `✓`.
fn marked_line(app: &App<TestBackend>) -> String {
    let lines: Vec<String> = screen(app)
        .into_iter()
        .filter(|line| line.contains('✓'))
        .collect();
    assert_eq!(lines.len(), 1, "one line carries the mark");
    lines[0].clone()
}

/// Whether the `✓` is drawn on the row of the value in force.
fn marks_current(app: &App<TestBackend>) -> bool {
    current(app).is_some_and(|label| marked_line(app).contains(&format!(" {label} ")))
}

fn focused(state: &State) -> String {
    state
        .selected_task()
        .map(|task| task.id.clone())
        .unwrap_or_default()
}

#[test]
fn p_opens_the_priority_picker_on_the_focused_task() {
    let mut app = demo();
    let task = app.state().selected_task().cloned().unwrap();

    send(&mut app, [key('p')]);

    assert_eq!(picker(&app).title, format!("Priority of {}", task.content));
    assert_eq!(labels(&app), ["P1", "P2", "P3", "P4"]);
    assert_eq!(
        current(&app).as_deref(),
        Some(Priority::from_api(task.priority).label())
    );
    assert!(shows(&app, "> type to filter"));
    assert!(marks_current(&app));
}

#[test]
fn a_number_chooses_the_priority_and_sends_one_item_update() {
    let mut app = demo();
    let task = focused(app.state());

    let effects = send(&mut app, [key('p'), key('1')]);

    let sent = commands(&effects);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].r#type, "item_update");
    assert_eq!(sent[0].args["id"], task.as_str());
    assert_eq!(sent[0].args["priority"], 4);
    assert!(app.state().modals.is_empty());
}

#[test]
fn typing_filters_the_priorities_and_numbers_count_what_is_left() {
    let mut app = demo();
    send(&mut app, [key('p')]);

    send(&mut app, typed("p"));
    assert_eq!(labels(&app), ["P1", "P2", "P3", "P4"]);
    send(&mut app, [press(KeyCode::Backspace)]);
    send(&mut app, typed("pq"));
    assert!(labels(&app).is_empty());
    assert!(shows(&app, "No matches"));

    send(&mut app, [press(KeyCode::Backspace)]);
    assert_eq!(picker(&app).query(), "p");
}

#[test]
fn a_digit_with_no_entry_to_pick_types_into_the_filter() {
    let mut app = demo();
    send(&mut app, [key('p')]);

    send(&mut app, [key('7')]);
    assert_eq!(picker(&app).query(), "7", "7 has no entry to pick");
    assert!(labels(&app).is_empty());

    send(&mut app, [press(KeyCode::Backspace), key('3')]);
    assert!(app.state().modals.is_empty(), "3 picks P3");
}

#[test]
fn moving_and_enter_choose_the_highlighted_entry() {
    let mut app = demo();
    send(&mut app, [key('p')]);
    let start = picker(&app).selected().cloned();

    send(&mut app, [press(KeyCode::Down)]);
    let moved = picker(&app).selected().cloned();
    assert_ne!(moved, start);
    let effects = send(&mut app, [enter()]);

    let Some(Choice::Priority(priority)) = moved else {
        panic!("a priority is highlighted");
    };
    assert_eq!(commands(&effects)[0].args["priority"], priority.api());
}

#[test]
fn esc_closes_the_picker_without_choosing() {
    let mut app = demo();
    let effects = send(&mut app, [key('p'), esc()]);

    assert!(app.state().modals.is_empty());
    assert!(commands(&effects).is_empty());
}

#[test]
fn vs_opens_the_sort_picker_with_the_current_sort_marked() {
    let mut app = demo();

    send(&mut app, [key('v'), key('s')]);

    assert_eq!(picker(&app).title, "Sort Inbox by");
    assert_eq!(labels(&app), ["Order", "Priority", "Due date", "Created"]);
    assert_eq!(current(&app).as_deref(), Some("Order"));
    assert!(marks_current(&app));
    send(&mut app, typed("date"));
    assert_eq!(labels(&app), ["Due date"]);
}

#[test]
fn choosing_a_sort_keeps_focus_on_the_same_task() {
    let mut app = demo();
    send(&mut app, [key('j'), key('j'), key('j')]);
    let task = focused(app.state());
    let row = app.state().selected_task;

    send(&mut app, [key('v'), key('s'), key('2')]);

    assert_eq!(app.state().sort, Sort::Priority);
    assert_eq!(focused(app.state()), task);
    assert_ne!(app.state().selected_task, row, "the task moved in the list");

    send(&mut app, [key('v'), key('s')]);
    assert_eq!(current(&app).as_deref(), Some("Priority"));
    assert!(marks_current(&app));
}

#[test]
fn f_opens_the_list_filter_picker_with_open_marked() {
    let mut app = demo();

    send(&mut app, [key('f')]);

    assert_eq!(picker(&app).title, "Show in Inbox");
    assert_eq!(
        labels(&app),
        [
            "Open",
            "Completed",
            "All",
            "Overdue",
            "Due today",
            "Due this week",
            "P1",
            "P2",
            "P3",
            "P4"
        ]
    );
    assert_eq!(current(&app).as_deref(), Some("Open"));
    assert!(marks_current(&app));
    send(&mut app, typed("due"));
    assert_eq!(labels(&app), ["Overdue", "Due today", "Due this week"]);
}

#[test]
fn choosing_a_list_filter_narrows_the_list_and_keeps_focus_on_the_same_task() {
    let mut app = demo();
    send(&mut app, [key('j'), key('j'), key('j')]);
    let row = app.state().selected_task;
    let task = app.state().selected_task().cloned().unwrap();
    let priority = Priority::from_api(task.priority);
    let wanted = ListFilter::Priority(priority);
    let below_open = ListFilter::ALL.iter().position(|f| *f == wanted).unwrap();

    send(&mut app, [key('f')]);
    send(&mut app, (0..below_open).map(|_| press(KeyCode::Down)));
    send(&mut app, [enter()]);

    assert_eq!(app.state().list_filter, wanted);
    let visible = app.state().visible_tasks();
    assert!(visible.len() > 1);
    assert!(
        visible
            .iter()
            .all(|t| Priority::from_api(t.priority) == priority)
    );
    assert_eq!(focused(app.state()), task.id);
    assert_ne!(app.state().selected_task, 0, "the task is not first");
    assert_ne!(app.state().selected_task, row, "the task moved in the list");
}

#[test]
fn choosing_completed_asks_for_the_completed_tasks() {
    let mut app = demo();

    let effects = send(&mut app, [key('f'), key('2')]);

    assert_eq!(app.state().list_filter, ListFilter::Completed);
    assert!(matches!(
        effects.as_slice(),
        [Effect::FetchCompletedTasks(_)]
    ));
}

#[test]
fn esc_on_the_list_clears_a_list_filter_and_keeps_focus() {
    let mut app = demo();
    send(&mut app, [key('f'), key('4')]);
    assert_eq!(app.state().list_filter, ListFilter::Overdue);
    send(&mut app, [key('j')]);
    let task = focused(app.state());

    send(&mut app, [esc()]);

    assert_eq!(app.state().list_filter, ListFilter::Open);
    assert_eq!(focused(app.state()), task);
}

#[test]
fn the_add_forms_project_field_opens_the_project_picker() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    send(&mut app, typed("Prune the roses"));
    send(
        &mut app,
        [
            press(KeyCode::Tab),
            press(KeyCode::Tab),
            press(KeyCode::Tab),
        ],
    );

    send(&mut app, [enter()]);

    assert_eq!(picker(&app).title, "Project of Prune the roses");
    assert_eq!(current(&app).as_deref(), Some("Inbox"));
    assert!(marks_current(&app));
    assert!(labels(&app).contains(&"Garden".to_owned()));
    send(&mut app, typed("gard"));
    assert_eq!(labels(&app), ["Garden"]);
    send(&mut app, [key('1')]);
    assert!(matches!(
        app.state().modals.as_slice(),
        [Modal::Form(form)] if form.project_id == "p-garden"
    ));

    send(&mut app, [press(KeyCode::Tab)]);
    let effects = send(&mut app, [enter()]);

    let sent = commands(&effects);
    assert_eq!(sent[0].r#type, "item_add");
    assert_eq!(sent[0].args["content"], "Prune the roses");
    assert_eq!(sent[0].args["project_id"], "p-garden");
}

#[test]
fn the_add_forms_priority_field_opens_the_priority_picker_for_the_new_task() {
    let mut app = demo();
    send(&mut app, [key('a'), press(KeyCode::Tab), enter()]);

    assert_eq!(picker(&app).title, "Priority of the new task");
    assert_eq!(current(&app).as_deref(), Some("P4"));
    send(&mut app, [key('2')]);
    assert!(matches!(
        app.state().modals.as_slice(),
        [Modal::Form(form)] if form.priority == 3
    ));
}

#[test]
fn choose_theme_applies_the_theme_and_names_the_line_that_keeps_it() {
    let mut app = demo_with(80, 24, THEME_ON_COMMA);
    let before = app.state().theme().name.clone();

    send(&mut app, [key(',')]);
    assert_eq!(picker(&app).title, "Theme for this session");
    assert_eq!(current(&app), Some(before));
    assert!(marks_current(&app));
    send(&mut app, typed("nord"));
    assert_eq!(labels(&app), ["Nord"]);
    send(&mut app, [enter()]);

    assert_eq!(app.state().theme().name, "Nord");
    assert_eq!(
        app.state().notice.as_deref(),
        Some(
            "Nord is the theme for this session; to keep it, add to config.toml: \
             [theme] name = \"nord\""
        )
    );
}

#[test]
fn a_number_chooses_a_theme() {
    let mut app = demo_with(80, 24, THEME_ON_COMMA);
    send(&mut app, [key(',')]);
    let second = labels(&app)[1].clone();
    assert_ne!(app.state().theme().name, second);

    send(&mut app, [key('2')]);

    assert!(app.state().modals.is_empty());
    assert_eq!(app.state().theme().name, second);
}

#[test]
fn the_priority_picker_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('p')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_priority_picker_at_120x36() {
    let mut app = demo_at(120, 36);
    send(&mut app, [key('p')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_sort_picker_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('v'), key('s')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_list_filter_picker_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('f')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_project_picker_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('a')]);
    send(&mut app, typed("Prune the roses"));
    send(&mut app, [press(KeyCode::BackTab), enter()]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_theme_picker_at_80x24() {
    let mut app = demo_with(80, 24, THEME_ON_COMMA);
    send(&mut app, [key(',')]);
    assert_snapshot!(app.backend());
}

#[test]
fn a_filtered_picker_at_80x24() {
    let mut app = demo();
    send(&mut app, [key('f')]);
    send(&mut app, typed("due"));
    assert_snapshot!(app.backend());
}

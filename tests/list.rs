//! Task lists: the list and detail panes, the rows and their columns, the
//! group headers and folds, the list filter header, the empty states and
//! the edit keys, driven through the app on the demo account.

#![cfg(test)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::Zoned;
use jiff::tz::TimeZone;
use ratatoist::app::{
    App, Effect, Event, GroupKind, ListRow, Modal, Pane, PickerKind, PromptField, Screen, State,
};
use ratatoist::config;
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::demo;
use ratatoist::todoist::sync::SyncResponse;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

fn now() -> Zoned {
    demo::NOW.to_zoned(TimeZone::UTC).unwrap()
}

fn demo_sync() -> Box<SyncResponse> {
    Box::new(serde_json::from_value(demo::account(&now()).sync).unwrap())
}

/// The demo account at `columns` x `rows`, told its size as the runtime's
/// first resize would.
fn sized(columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(columns, rows), State::new(now())).unwrap();
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();
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

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
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

fn rows(app: &App<TestBackend>) -> Vec<String> {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn shows(app: &App<TestBackend>, text: &str) -> bool {
    rows(app).iter().any(|row| row.contains(text))
}

fn focused_id(app: &App<TestBackend>) -> String {
    app.state()
        .selected_task()
        .map(|task| task.id.clone())
        .unwrap_or_default()
}

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

/// Opens the list of the project named `name` through the palette.
fn open_project(app: &mut App<TestBackend>, name: &str) {
    send(app, [key(':')]);
    send(app, typed(name));
    send(app, [press(KeyCode::Enter)]);
    assert_eq!(app.state().list_title(), name);
}

/// Focuses the task `id` in the list on screen, moving down from the top.
fn focus(app: &mut App<TestBackend>, id: &str) {
    send(app, [key('g'), key('g')]);
    for _ in 0..40 {
        if focused_id(app) == id {
            return;
        }
        send(app, [key('j')]);
    }
    assert_eq!(focused_id(app), id);
}

/// The column of the priority label (`P1` to `P4`) in row `y`, left of
/// column `end`.
fn priority_column(buffer: &Buffer, y: u16, end: u16) -> Option<u16> {
    (1..end.saturating_sub(1)).rev().find(|&x| {
        buffer[(x - 1, y)].symbol() == " "
            && buffer[(x, y)].symbol() == "P"
            && buffer[(x + 1, y)].symbol().parse::<u8>().is_ok()
    })
}

#[test]
fn at_the_wide_layout_width_the_list_and_detail_show_side_by_side() {
    let mut app = sized(120, 36);
    send(&mut app, [key('1')]);

    let top = &rows(&app)[1];
    assert!(top.starts_with("┏Inbox"));
    assert!(top.contains("┌Inbox"));
    assert!(shows(&app, "State     ○ Open"));
}

#[test]
fn h_and_l_move_the_focus_between_the_panes() {
    let mut app = sized(120, 36);
    send(&mut app, [key('1'), key('l')]);
    assert_eq!(app.state().active_pane, Pane::Detail);
    assert!(rows(&app)[1].starts_with("┌Inbox"));

    send(&mut app, [key('h')]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    assert!(rows(&app)[1].starts_with("┏Inbox"));
}

#[test]
fn below_the_wide_layout_width_only_the_list_shows_and_enter_and_esc_switch() {
    let mut app = sized(119, 36);
    send(&mut app, [key('1')]);
    assert!(!shows(&app, "State     ○ Open"));
    assert!(rows(&app)[1].starts_with("┌Inbox"));

    send(&mut app, [press(KeyCode::Enter)]);
    assert_eq!(app.state().active_pane, Pane::Detail);
    assert!(shows(&app, "State     ○ Open"));
    assert!(!shows(&app, "@phone"));

    send(&mut app, [press(KeyCode::Esc)]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    assert!(!shows(&app, "State     ○ Open"));
}

#[test]
fn h_and_l_do_nothing_below_the_wide_layout_width() {
    let mut app = sized(119, 36);
    send(&mut app, [key('1'), key('l')]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
}

#[test]
fn the_wide_layout_width_comes_from_the_config() {
    let config = config::parse("[ui]\nwide-layout-columns = 100\n", &[]);
    let state = initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    );
    let mut app = App::new(TestBackend::new(100, 30), state).unwrap();
    app.handle(Event::Synced(Ok(demo_sync()))).unwrap();
    send(&mut app, [key('1'), key('l')]);

    assert!(shows(&app, "State     ○ Open"));
    assert_eq!(app.state().active_pane, Pane::Detail);
}

#[test]
fn the_detail_beside_the_list_follows_the_focused_task() {
    let mut app = sized(120, 36);
    send(&mut app, [key('1')]);
    let first = rows(&app)[2].clone();

    send(&mut app, [key('j')]);

    assert_ne!(rows(&app)[2], first);
    let content = app.state().selected_task().unwrap().content.clone();
    let start: String = content.chars().take(10).collect();
    assert!(rows(&app)[2].contains(&start));
}

#[test]
fn every_row_aligns_its_priority_column_with_mixed_widths() {
    for (columns, rows) in [(120, 36), (80, 24)] {
        for screen in ['1', '2', '3'] {
            let mut app = sized(columns, rows);
            send(&mut app, [key(screen)]);
            let buffer = app.backend().buffer();
            let list_end = (1..buffer.area.width)
                .find(|&x| matches!(buffer[(x, 1)].symbol(), "┐" | "┓"))
                .unwrap();
            let columns: Vec<u16> = (2..buffer.area.height - 4)
                .filter_map(|y| priority_column(buffer, y, list_end))
                .collect();
            assert!(columns.len() > 3);
            assert!(columns.windows(2).all(|pair| pair[0] == pair[1]));
        }
    }
}

#[test]
fn long_content_ends_in_an_ellipsis() {
    let mut app = sized(80, 24);
    send(&mut app, [key('1')]);
    assert!(shows(&app, "Write down every idea from Sunday's walk"));
    let row = rows(&app)
        .into_iter()
        .find(|row| row.contains("Write down every idea"))
        .unwrap();
    assert!(row.contains('…'));
}

/// The groups of the list on screen, with the number of task rows under
/// each header.
fn groups(state: &State) -> Vec<(GroupKind, usize, usize)> {
    let mut groups: Vec<(GroupKind, usize, usize)> = Vec::new();
    for row in state.list_rows() {
        match row {
            ListRow::Header(group) => groups.push((group.kind, group.count, 0)),
            ListRow::Task { depth, .. } => {
                if let Some(last) = groups.last_mut()
                    && depth == 0
                {
                    last.2 += 1;
                }
            }
        }
    }
    groups
}

#[test]
fn today_s_header_counts_match_their_rows() {
    let mut app = sized(120, 36);
    send(&mut app, [key('2')]);

    let groups = groups(app.state());
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].0, GroupKind::Overdue);
    assert_eq!(groups[1].0, GroupKind::Today);
    for (kind, count, listed) in &groups {
        assert_eq!(count, listed);
        let name = match kind {
            GroupKind::Overdue => "▲ Overdue",
            _ => "◆ Today",
        };
        assert!(shows(&app, &format!("{name} ({count})")));
    }
}

/// A delta adding an open subtask due on `day` under `parent`.
fn subtask_due(id: &str, parent: &ListRow, day: jiff::civil::Date) -> Box<SyncResponse> {
    let ListRow::Task { task, .. } = parent else {
        panic!("not a task row");
    };
    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "fixture-token-2",
        "items": [{
            "id": id,
            "content": "A subtask with a due date",
            "checked": false,
            "child_order": 1,
            "priority": 1,
            "project_id": task.project_id,
            "parent_id": task.id,
            "due": { "date": day.to_string(), "is_recurring": false, "string": "" },
            "added_at": "2026-11-19T08:30:00Z"
        }]
    });
    Box::new(serde_json::from_value(delta).unwrap())
}

#[test]
fn today_leaves_subtasks_out_of_its_rows_and_its_header_counts() {
    // Below the wide width, so the focused parent's detail (which lists its
    // subtasks) is not on screen.
    let mut app = sized(119, 36);
    send(&mut app, [key('2')]);
    let before = groups(app.state());
    let list = app.state().list_rows();
    let first_of = |kind: GroupKind| {
        let header = list
            .iter()
            .position(|row| matches!(row, ListRow::Header(group) if group.kind == kind))
            .unwrap();
        list[header + 1].clone()
    };
    let overdue_parent = first_of(GroupKind::Overdue);
    let today_parent = first_of(GroupKind::Today);
    let today = demo::NOW.date();
    let overdue = subtask_due(
        "t-overdue-subtask",
        &overdue_parent,
        today.yesterday().unwrap(),
    );
    let due_today = subtask_due("t-today-subtask", &today_parent, today);

    app.handle(Event::Synced(Ok(overdue))).unwrap();
    app.handle(Event::Synced(Ok(due_today))).unwrap();

    let subtasks = app
        .state()
        .store
        .tasks
        .iter()
        .filter(|task| task.id.ends_with("-subtask"))
        .count();
    assert_eq!(subtasks, 2);
    assert_eq!(groups(app.state()), before);
    for (_, count, listed) in &before {
        assert_eq!(count, listed);
    }
    assert!(!shows(&app, "A subtask with a due date"));
}

#[test]
fn section_header_counts_match_their_top_level_rows() {
    let mut app = sized(120, 36);
    open_project(&mut app, "Atlas redesign");

    let groups = groups(app.state());
    let names: Vec<String> = groups
        .iter()
        .map(|(kind, _, _)| match kind {
            GroupKind::Section(Some(name)) => name.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(names, ["Research", "Design", "Build"]);
    for (kind, count, listed) in &groups {
        assert_eq!(count, listed);
        if let GroupKind::Section(Some(name)) = kind {
            assert!(shows(&app, &format!("{name} ({count})")));
        }
    }
}

#[test]
fn a_folded_overdue_group_keeps_its_header() {
    let mut app = sized(120, 36);
    send(&mut app, [key('2'), key('z'), key('a')]);

    assert!(shows(&app, "▲ Overdue (8) ▹"));
    assert!(!shows(&app, "Fix the wobbly chair leg"));
    let state = app.state();
    let tasks = state.visible_tasks();
    assert!(tasks.iter().all(|task| task.id != "t-chair"));

    send(&mut app, [key('z'), key('a')]);
    assert!(shows(&app, "Fix the wobbly chair leg"));
}

#[test]
fn a_task_with_subtasks_leads_with_its_fold_glyph() {
    let mut app = sized(119, 36);
    open_project(&mut app, "Kitchen renovation");

    let row = |app: &App<TestBackend>| {
        rows(app)
            .into_iter()
            .find(|row| row.contains("Pick the tiles"))
            .unwrap()
    };
    assert!(row(&app).contains("▿ Pick the tiles"));
    assert!(shows(&app, "    ○ Measure the worktop"));

    focus(&mut app, "t-tiles");
    send(&mut app, [key('z'), key('a')]);

    assert!(row(&app).contains("▹ Pick the tiles"));
    assert!(!shows(&app, "Measure the worktop"));
}

#[test]
fn folds_on_subtasks_survive_a_sync_delta() {
    let mut app = sized(119, 36);
    open_project(&mut app, "Kitchen renovation");
    focus(&mut app, "t-tiles");
    send(&mut app, [key('z'), key('a')]);
    assert!(!shows(&app, "Measure the worktop"));

    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "delta-1",
        "items": [{
            "id": "t-worktop",
            "content": "Measure the worktop twice",
            "checked": false,
            "child_order": 9,
            "priority": 1,
            "project_id": "p-kitchen",
            "parent_id": "t-tiles"
        }]
    });
    let delta: Box<SyncResponse> = Box::new(serde_json::from_value(delta).unwrap());
    app.handle(Event::Synced(Ok(delta))).unwrap();

    assert!(!shows(&app, "Measure the worktop"));
    assert!(shows(&app, "▹ Pick the tiles"));
    assert_eq!(focused_id(&app), "t-tiles");
}

/// A delta with one task for `project_id`: added with `child_order`, or
/// deleted when `deleted`.
fn task_delta(id: &str, project_id: &str, child_order: i64, deleted: bool) -> Box<SyncResponse> {
    let delta = serde_json::json!({
        "full_sync": false,
        "sync_token": "delta-1",
        "items": [{
            "id": id,
            "content": "A task a delta brings",
            "checked": false,
            "is_deleted": deleted,
            "child_order": child_order,
            "priority": 1,
            "project_id": project_id
        }]
    });
    Box::new(serde_json::from_value(delta).unwrap())
}

fn fetches_comments(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::FetchComments { .. }))
}

#[test]
fn a_task_a_delta_inserts_above_keeps_the_focus_on_the_same_task() {
    let mut app = sized(120, 36);
    send(&mut app, [key('1'), key('j')]);
    let focused = focused_id(&app);
    let row = app.state().selected_task;

    let effects = app
        .handle(Event::Synced(Ok(task_delta("t-new", "p-inbox", -1, false))))
        .unwrap();

    let ids: Vec<String> = app
        .state()
        .visible_tasks()
        .iter()
        .map(|task| task.id.clone())
        .collect();
    assert_eq!(ids.first().map(String::as_str), Some("t-new"));
    assert_eq!(focused_id(&app), focused);
    assert_eq!(app.state().selected_task, row + 1);
    assert!(!fetches_comments(&effects));
}

#[test]
fn when_a_delta_removes_the_focused_task_the_detail_loads_the_next_one_s_comments() {
    let mut app = sized(120, 36);
    send(&mut app, [key('1'), key('j')]);
    let focused = focused_id(&app);

    let effects = app
        .handle(Event::Synced(Ok(task_delta(&focused, "p-inbox", 0, true))))
        .unwrap();

    assert_ne!(focused_id(&app), focused);
    assert!(fetches_comments(&effects));
}

#[test]
fn zr_and_zm_unfold_and_fold_every_row() {
    let mut app = sized(119, 36);
    open_project(&mut app, "Kitchen renovation");

    send(&mut app, [key('z'), key('M')]);
    assert!(!shows(&app, "Measure the worktop"));
    send(&mut app, [key('z'), key('R')]);
    assert!(shows(&app, "Measure the worktop"));
}

#[test]
fn the_list_filter_header_shows_the_filter_and_its_count() {
    let mut app = sized(120, 36);
    send(&mut app, [key('2'), key('f'), key('7')]);

    let p1 = app.state().visible_tasks().len();
    let header = &rows(&app)[1];
    assert!(header.starts_with(&format!("f filter: p1 · {p1} of 19 · Esc clears")));

    send(&mut app, [press(KeyCode::Esc)]);
    assert!(!shows(&app, "filter:"));
}

#[test]
fn no_header_shows_while_the_filter_and_sort_are_the_defaults() {
    let mut app = sized(120, 36);
    send(&mut app, [key('2')]);
    assert!(rows(&app)[1].starts_with('┏'));
}

#[test]
fn an_empty_list_says_so_in_one_muted_sentence() {
    let mut app = sized(120, 36);
    open_project(&mut app, "Reading");
    send(&mut app, [key('f'), key('7')]);

    assert!(shows(&app, "No tasks match p1."));
}

#[test]
fn today_with_nothing_due_says_so() {
    let mut state = sized(120, 36).state().clone();
    state.store.tasks.retain(|task| task.due.is_none());
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    send(&mut app, [key('2')]);

    assert!(shows(&app, "Nothing due today."));
}

#[test]
fn an_empty_project_says_so() {
    let mut state = sized(120, 36).state().clone();
    state
        .store
        .tasks
        .retain(|task| task.project_id != "p-reading");
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();
    open_project(&mut app, "Reading");

    assert!(shows(&app, "No tasks in Reading."));
}

#[test]
fn a_due_date_sent_shows_muted_with_an_ellipsis_until_todoist_answers() {
    let mut app = sized(119, 36);
    send(&mut app, [key('1')]);
    let clear = Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    send(&mut app, [key('d'), clear]);
    send(&mut app, typed("next friday"));
    send(&mut app, [press(KeyCode::Enter)]);

    assert!(shows(&app, "next friday…"));
}

#[test]
fn a_long_due_string_sent_is_cut_with_an_ellipsis_beside_the_content() {
    let mut app = sized(80, 24);
    send(&mut app, [key('1')]);
    let content = app.state().selected_task().unwrap().content.clone();
    let start: String = content.chars().take(10).collect();
    let clear = Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    send(&mut app, [key('d'), clear]);
    send(&mut app, typed(&"every other monday ".repeat(4)));
    send(&mut app, [press(KeyCode::Enter)]);

    let row = rows(&app)
        .into_iter()
        .find(|row| row.contains(&start))
        .unwrap();
    assert!(row.contains("every other monday…"));
    assert!(!row.contains(&"every other monday ".repeat(4)));
}

/// The opener of each edit key, checked against the task the list focuses.
fn assert_edit_keys_act_on(app_at: impl Fn() -> App<TestBackend>) {
    for (edit, field) in [
        ('e', PromptField::Content),
        ('E', PromptField::Description),
        ('d', PromptField::DueDate),
        ('c', PromptField::Comment),
    ] {
        let mut app = app_at();
        let id = focused_id(&app);
        send(&mut app, [key(edit)]);
        match app.state().top_modal() {
            Some(Modal::Prompt(prompt)) => {
                assert_eq!(prompt.field, field);
                assert_eq!(prompt.task_id, id);
            }
            _ => panic!("no prompt"),
        }
    }

    let mut app = app_at();
    send(&mut app, [key('p')]);
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Picker(picker)) if picker.kind == PickerKind::Priority
    ));

    let mut app = app_at();
    let effects = send(&mut app, [key('x')]);
    assert_eq!(command_types(&effects), ["item_close"]);
}

#[test]
fn the_edit_keys_act_on_the_focused_task_from_the_list() {
    assert_edit_keys_act_on(|| {
        let mut app = sized(119, 36);
        send(&mut app, [key('1'), key('j')]);
        assert_eq!(app.state().active_pane, Pane::Tasks);
        app
    });
}

#[test]
fn the_edit_keys_act_on_the_focused_task_from_the_detail() {
    for columns in [119, 120] {
        assert_edit_keys_act_on(|| {
            let mut app = sized(columns, 36);
            send(&mut app, [key('1'), key('j'), press(KeyCode::Enter)]);
            assert_eq!(app.state().active_pane, Pane::Detail);
            app
        });
    }
}

#[test]
fn upcoming_lists_the_project_and_today_does_too() {
    let mut app = sized(120, 36);
    send(&mut app, [key('3')]);
    assert_eq!(app.state().screen, Screen::Upcoming);
    assert!(shows(&app, "P2 Inbox"));
}

#[test]
fn a_project_with_sections_and_subtasks_at_120x36() {
    let mut app = sized(120, 36);
    open_project(&mut app, "Atlas redesign");
    assert_snapshot!(app.backend());
}

#[test]
fn a_project_with_sections_and_subtasks_at_80x24() {
    let mut app = sized(80, 24);
    open_project(&mut app, "Atlas redesign");
    assert_snapshot!(app.backend());
}

#[test]
fn kitchen_with_nested_subtasks_at_80x24() {
    let mut app = sized(80, 24);
    open_project(&mut app, "Kitchen renovation");
    assert_snapshot!(app.backend());
}

#[test]
fn the_detail_focused_beside_the_list_at_120x36() {
    let mut app = sized(120, 36);
    send(&mut app, [key('1'), key('j'), key('l')]);
    assert_snapshot!(app.backend());
}

#[test]
fn the_list_filter_header_at_80x24() {
    let mut app = sized(80, 24);
    send(&mut app, [key('2'), key('f'), key('7')]);
    assert_snapshot!(app.backend());
}

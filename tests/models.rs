//! The serde models against the fixtures: every resource a sync carries
//! parses, and a drifted entity (fields missing, unknown fields, unknown
//! enum values) parses too, so it never fails a whole sync.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use ratatoist::todoist::model::{DurationUnit, ReminderKind};
use ratatoist::todoist::sync::SyncResponse;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/todoist");

fn sync_fixture(name: &str) -> SyncResponse {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_full_sync_parses_every_resource() {
    let sync = sync_fixture("HYPOTHETICAL_full_sync.json");

    assert_eq!(sync.projects.unwrap().len(), 3);
    assert_eq!(sync.sections.unwrap().len(), 1);
    assert_eq!(sync.items.unwrap().len(), 3);
    assert_eq!(sync.labels.unwrap().len(), 1);
    assert_eq!(sync.notes.unwrap().len(), 1);
    assert_eq!(sync.collaborators.unwrap().len(), 1);
    assert_eq!(sync.collaborator_states.unwrap().len(), 1);
    assert_eq!(sync.workspaces.unwrap().len(), 1);
    assert_eq!(sync.folders.unwrap().len(), 1);
    assert_eq!(sync.reminders.unwrap().len(), 3);
    assert!(sync.user.is_some());
}

#[test]
fn a_task_reads_its_deadline_and_duration() {
    let items = sync_fixture("HYPOTHETICAL_full_sync.json").items.unwrap();
    let seedlings = items.iter().find(|t| t.id == "6Jf8VQXxpwv56VT2").unwrap();
    let compost = items.iter().find(|t| t.id == "6Jf8VQXxpwv56VT3").unwrap();

    let duration = seedlings.duration.as_ref().unwrap();
    assert_eq!(duration.amount, 15);
    assert_eq!(duration.unit, DurationUnit::Minute);
    assert!(seedlings.deadline.is_none());
    assert_eq!(compost.deadline.as_ref().unwrap().date, "2026-12-01");
    assert!(compost.duration.is_none());
}

#[test]
fn reminders_read_relative_absolute_and_location_kinds() {
    let reminders = sync_fixture("HYPOTHETICAL_full_sync.json")
        .reminders
        .unwrap();
    let kind = |id: &str| reminders.iter().find(|r| r.id == id).unwrap();

    let relative = kind("6Jf8VQXxpwv56VM1");
    assert_eq!(relative.kind, ReminderKind::Relative);
    assert_eq!(relative.item_id, "6Jf8VQXxpwv56VT2");
    assert_eq!(relative.minute_offset, Some(30));

    let absolute = kind("6Jf8VQXxpwv56VM2");
    assert_eq!(absolute.kind, ReminderKind::Absolute);
    assert_eq!(
        absolute.due.as_ref().unwrap().date,
        "2026-11-21T08:00:00.000000Z"
    );

    let location = kind("6Jf8VQXxpwv56VM3");
    assert_eq!(location.kind, ReminderKind::Location);
    assert_eq!(location.name.as_deref(), Some("Garden centre"));
    assert_eq!(location.loc_trigger.as_deref(), Some("on_enter"));
}

#[test]
fn a_drifted_sync_parses_every_entity() {
    let sync = sync_fixture("HYPOTHETICAL_drifted_sync.json");

    let project = &sync.projects.unwrap()[0];
    assert_eq!(project.id, "6Jf8VQXxpwv56VQ7");
    assert_eq!(project.color, "");
    assert_eq!(project.child_order, 0);
    assert!(!project.is_shared);
    assert!(!project.is_favorite);

    let task = &sync.items.unwrap()[0];
    assert_eq!(task.duration.as_ref().unwrap().unit, DurationUnit::Other);
    assert_eq!(task.deadline.as_ref().unwrap().date, "");
    assert_eq!(task.due.as_ref().unwrap().date, "");

    let reminder = &sync.reminders.unwrap()[0];
    assert_eq!(reminder.kind, ReminderKind::Other);
    assert_eq!(reminder.item_id, "");

    assert_eq!(sync.labels.unwrap()[0].name, "");
    assert_eq!(sync.sections.unwrap()[0].project_id, "");
    assert_eq!(sync.workspaces.unwrap()[0].name, "");
    assert_eq!(sync.folders.unwrap()[0].workspace_id, "");
    assert_eq!(sync.collaborator_states.unwrap()[0].state, "");
}

#[test]
fn a_reminder_with_only_an_id_parses() {
    let reminder: ratatoist::todoist::model::Reminder =
        serde_json::from_str(r#"{"id":"r1"}"#).unwrap();

    assert_eq!(reminder.kind, ReminderKind::Other);
    assert!(reminder.due.is_none());
    assert!(!reminder.is_deleted);
}

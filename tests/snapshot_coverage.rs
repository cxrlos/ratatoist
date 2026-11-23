//! Every screen and every overlay has a snapshot at 120x36 and at 80x24.
//! The matches below have no catch-all arm, so a new `Screen` or `Modal`
//! variant does not compile until it is named here, and naming it means
//! pointing at its two snapshot files.

#![cfg(test)]

use std::path::Path;

use ratatoist::app::{
    Confirm, Confirmed, Modal, Palette, Picker, PickerKind, Prompt, PromptField, Screen, TaskForm,
};

/// What shows each screen and overlay: its name, then the snapshot in
/// `tests/snapshots/` at 120x36 and the one at 80x24.
const SNAPSHOTS: &[(&str, &str, &str)] = &[
    ("Inbox", "frame__inbox_at_120x36", "frame__inbox_at_80x24"),
    ("Today", "frame__today_at_120x36", "frame__today_at_80x24"),
    (
        "Upcoming",
        "frame__upcoming_at_120x36",
        "frame__upcoming_at_80x24",
    ),
    (
        "Projects",
        "frame__projects_at_120x36",
        "frame__projects_at_80x24",
    ),
    (
        "Labels",
        "frame__labels_at_120x36",
        "frame__labels_at_80x24",
    ),
    (
        "Cheatsheet",
        "cheatsheet__cheatsheet_at_120x36",
        "cheatsheet__cheatsheet_at_80x24",
    ),
    (
        "WhichKey",
        "which_key__which_key_after_z_at_120x36",
        "which_key__which_key_after_z_at_80x24",
    ),
    (
        "Palette",
        "palette__palette_typing_prio_at_120x36",
        "palette__palette_typing_prio_at_80x24",
    ),
    (
        "Picker",
        "picker__the_priority_picker_at_120x36",
        "picker__the_priority_picker_at_80x24",
    ),
    (
        "Confirm",
        "confirm__the_quit_confirm_at_120x36",
        "confirm__the_quit_confirm_at_80x24",
    ),
    (
        "Messages",
        "messages__messages_at_120x36",
        "messages__messages_at_80x24",
    ),
    (
        "Form",
        "add_task__the_empty_form_at_120x36",
        "add_task__the_empty_form_at_80x24",
    ),
    (
        "Prompt",
        "prompt__the_due_date_prompt_at_120x36",
        "prompt__the_due_date_prompt_at_80x24",
    ),
];

fn screen_name(screen: Screen) -> &'static str {
    match screen {
        Screen::Inbox => "Inbox",
        Screen::Today => "Today",
        Screen::Upcoming => "Upcoming",
        Screen::Projects => "Projects",
        Screen::Labels => "Labels",
    }
}

fn modal_name(modal: &Modal) -> &'static str {
    match modal {
        Modal::Cheatsheet { .. } => "Cheatsheet",
        Modal::WhichKey { .. } => "WhichKey",
        Modal::Palette(_) => "Palette",
        Modal::Picker(_) => "Picker",
        Modal::Confirm(_) => "Confirm",
        Modal::Messages { .. } => "Messages",
        Modal::Form(_) => "Form",
        Modal::Prompt(_) => "Prompt",
    }
}

/// One of each `Modal` variant, with whatever content: only the variant
/// is read.
fn every_modal() -> Vec<Modal> {
    vec![
        Modal::Cheatsheet { scroll: 0 },
        Modal::WhichKey { prefix: Vec::new() },
        Modal::Palette(Palette::default()),
        Modal::Picker(Picker::new(
            PickerKind::Priority,
            String::new(),
            Vec::new(),
            None,
        )),
        Modal::Confirm(Confirm {
            title: String::new(),
            subject: String::new(),
            consequences: Vec::new(),
            verb: "",
            confirmed: Confirmed::Quit,
        }),
        Modal::Messages { scroll: 0 },
        Modal::Form(TaskForm::new(String::new(), String::new())),
        Modal::Prompt(Prompt {
            field: PromptField::Content,
            task_id: String::new(),
            subject: String::new(),
            error: None,
        }),
    ]
}

fn snapshot_exists(name: &str) -> bool {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.snap"))
        .is_file()
}

#[test]
fn every_screen_and_modal_has_a_snapshot_at_both_sizes() {
    let modals = every_modal();
    let names = Screen::ALL
        .into_iter()
        .map(screen_name)
        .chain(modals.iter().map(modal_name));
    let mut missing = Vec::new();

    for name in names {
        match SNAPSHOTS.iter().find(|(shown, ..)| *shown == name) {
            Some((_, large, small)) => missing.extend(
                [*large, *small]
                    .into_iter()
                    .filter(|file| !snapshot_exists(file))
                    .map(str::to_owned),
            ),
            None => missing.push(name.to_owned()),
        }
    }

    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn every_modal_variant_is_listed_once() {
    let mut names: Vec<_> = every_modal().iter().map(modal_name).collect();
    let count = names.len();
    names.sort_unstable();
    names.dedup();

    assert_eq!(names.len(), count);
}

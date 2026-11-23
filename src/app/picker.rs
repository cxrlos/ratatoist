//! The one picker every choice uses: priority, sort, list filter, project
//! and theme.

use super::{Modal, State};

/// What a picker chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    /// The focused task's priority.
    Priority,
    /// How the task list on screen is sorted.
    Sort,
    /// What the task list on screen shows.
    Filter,
    /// The project of the task in the add-task form.
    Project,
    /// The theme for this session.
    Theme,
}

/// An open picker: what it chooses, the text typed to filter its entries,
/// and the highlighted entry among those the filter leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub kind: PickerKind,
    pub query: String,
    pub selected: usize,
}

impl Picker {
    pub fn new(kind: PickerKind) -> Self {
        Self {
            kind,
            query: String::new(),
            selected: 0,
        }
    }
}

/// Opens the picker that chooses `kind` over the screen.
pub fn open_picker(state: &mut State, kind: PickerKind) {
    state.modals.push(Modal::Picker(Picker::new(kind)));
}

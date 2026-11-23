/// One overlay over the main screen. [`super::State::modals`] keeps them as
/// a stack: the top one takes keys and is the one drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// The `?` cheatsheet.
    Help,
    /// Choosing a priority, with `selection` in the API's numbering.
    PriorityPicker {
        selection: u8,
    },
    /// The new-task form.
    TaskForm(TaskForm),
    /// A one-line prompt; what it writes to depends on `target`.
    Input(InputTarget),
    ThemePicker {
        selection: usize,
    },
}

/// What a submitted [`Modal::Input`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    /// A new comment on the selected task.
    Comment,
    /// The selected detail field of the selected task.
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskForm {
    pub content: String,
    pub priority: u8,
    pub due_string: String,
    pub project_id: String,
    pub active_field: usize,
    /// Typed keys go to the active field's text.
    pub editing: bool,
}

impl TaskForm {
    pub fn new(project_id: String) -> Self {
        Self {
            content: String::new(),
            priority: 1,
            due_string: String::new(),
            project_id,
            active_field: 0,
            editing: true,
        }
    }

    pub fn field_count() -> usize {
        4
    }
}

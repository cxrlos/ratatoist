//! The add-task form: a task's content, priority, due date and project,
//! sent as one `item_add`. The content goes as typed: Todoist does not read
//! a priority, label or project out of it.

/// The due string the form starts with on Today.
pub(crate) const TODAY: &str = "today";

/// A row of the form, in the order Tab visits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Content,
    Priority,
    /// A due string Todoist parses, as `tomorrow` or `every fri`.
    Due,
    Project,
}

impl FormField {
    pub const ALL: [FormField; 4] = [
        FormField::Content,
        FormField::Priority,
        FormField::Due,
        FormField::Project,
    ];

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            FormField::Content => "Content",
            FormField::Priority => "Priority",
            FormField::Due => "Due",
            FormField::Project => "Project",
        }
    }

    /// The row `step` rows away, wrapping at both ends.
    pub fn step(self, step: isize) -> FormField {
        let count = Self::ALL.len().cast_signed();
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0)
            .cast_signed();
        Self::ALL[(index + step).rem_euclid(count).cast_unsigned()]
    }
}

/// The open form. The focused text row's text is [`super::State::input`]
/// while it has focus, and is kept here when focus leaves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskForm {
    pub content: String,
    /// The API's number: 4 is P1, 1 is P4.
    pub priority: u8,
    pub due_string: String,
    pub project_id: String,
    pub field: FormField,
    /// Why the last Enter did not add the task.
    pub error: Option<&'static str>,
}

impl TaskForm {
    /// An empty form adding to `project_id`, due `due_string`.
    pub fn new(project_id: String, due_string: String) -> Self {
        Self {
            content: String::new(),
            priority: 1,
            due_string,
            project_id,
            field: FormField::Content,
            error: None,
        }
    }

    /// Whether the focused row takes typed text, in
    /// [`super::State::input`]: the content and the due date do; the
    /// others open a picker.
    pub fn editing(&self) -> bool {
        matches!(self.field, FormField::Content | FormField::Due)
    }
}

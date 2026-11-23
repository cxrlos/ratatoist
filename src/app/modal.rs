use super::confirm::Confirm;
use super::palette::Palette;
use super::picker::Picker;
use crate::keys::Key;

/// One overlay over the main screen. [`super::State::modals`] keeps them as
/// a stack: the top one takes keys and is the one drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// Every action and its keys for the context below, scrolled down by
    /// `scroll` rows when it is taller than the body.
    Cheatsheet { scroll: u16 },
    /// The keys that can follow `prefix`, the start of a chord.
    WhichKey { prefix: Vec<Key> },
    /// Find an action, a loaded task or a project by typing.
    Palette(Palette),
    /// Choose one value from a list.
    Picker(Picker),
    /// A question to answer before unsent changes are discarded.
    Confirm(Confirm),
    /// The messages log, newest first, scrolled down by `scroll` rows.
    Messages { scroll: u16 },
    /// The add-task form.
    Form(TaskForm),
    /// A one-line prompt; what it writes to depends on its kind.
    Input(InputKind),
    /// Choosing a priority, with `selection` in the API's numbering. The
    /// picker replaces it.
    PriorityPicker { selection: u8 },
    /// The picker replaces it.
    ThemePicker { selection: usize },
}

/// What a submitted [`Modal::Input`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// A new comment on the focused task.
    Comment,
    /// The focused task's content.
    Content,
    /// The focused task's description.
    Description,
    /// The focused task's due date, as a due string.
    DueDate,
    /// The selected detail field of the focused task. The direct edit keys
    /// replace it.
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskForm {
    pub content: String,
    pub priority: u8,
    pub due_string: String,
    pub project_id: String,
    pub active_field: usize,
}

impl TaskForm {
    pub fn new(project_id: String) -> Self {
        Self {
            content: String::new(),
            priority: 1,
            due_string: String::new(),
            project_id,
            active_field: 0,
        }
    }

    pub fn field_count() -> usize {
        4
    }

    /// Whether the active field takes typed text, in
    /// [`super::State::input`]: the content and the due date do.
    pub fn editing(&self) -> bool {
        matches!(self.active_field, 0 | 2)
    }
}

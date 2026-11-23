use super::confirm::Confirm;
use super::palette::Palette;
use super::picker::Picker;
use super::prompt::Prompt;
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
    /// The one-line prompt for a comment or a field of a task.
    Prompt(Prompt),
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

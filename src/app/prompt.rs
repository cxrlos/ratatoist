//! The one prompt every text input uses: a comment, and a task's content,
//! description and due date. The add-task form's text rows draw the same
//! input line.

use super::{Modal, State, TextField};

/// What a prompt writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptField {
    /// A new comment on the task.
    Comment,
    Content,
    Description,
    /// A due string Todoist parses, as `tomorrow` or `every fri`.
    DueDate,
}

impl PromptField {
    /// The words before the task's content in the title, as `Due date of`.
    pub fn title(self) -> &'static str {
        match self {
            PromptField::Comment => "Comment on",
            PromptField::Content => "Content of",
            PromptField::Description => "Description of",
            PromptField::DueDate => "Due date of",
        }
    }

    /// The muted line under the input saying what it takes.
    pub fn hint(self) -> &'static str {
        match self {
            PromptField::Comment => "One line, kept as typed, Markdown included",
            PromptField::Content => "The task's title, Markdown kept as typed",
            PromptField::Description => "One line, kept as typed; empty clears it",
            PromptField::DueDate => "today, tomorrow, fri, Dec 3, every week or no date",
        }
    }

    /// What the empty input line says, muted.
    pub fn placeholder(self) -> &'static str {
        match self {
            PromptField::Comment => "type a comment",
            PromptField::Content => "type the content",
            PromptField::Description => "type a description",
            PromptField::DueDate => "type a date",
        }
    }

    /// The word after the accept key, as `Enter send`.
    pub fn verb(self) -> &'static str {
        match self {
            PromptField::Comment => "send",
            PromptField::Content | PromptField::Description | PromptField::DueDate => "set",
        }
    }

    /// Why `text` (already trimmed) cannot be sent, if it cannot.
    pub fn refusal(self, text: &str) -> Option<&'static str> {
        if !text.is_empty() {
            return None;
        }
        match self {
            PromptField::Comment => Some("A comment needs text"),
            PromptField::Content => Some("A task needs content"),
            PromptField::DueDate => Some("Type a date, or no date to clear it"),
            PromptField::Description => None,
        }
    }
}

/// An open prompt: what it writes, the task it writes to (taken when it
/// opened, so a sync that reorders the list does not move it), that task's
/// content for the title, and why the last Enter was refused. The typed
/// text is [`State::input`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub field: PromptField,
    pub task_id: String,
    pub subject: String,
    pub error: Option<&'static str>,
}

/// Opens the prompt that writes `field` of the focused task. Content and
/// description start with their current text and the cursor at the end; a
/// comment and a due date start empty. Returns whether a task was focused.
pub fn open_prompt(state: &mut State, field: PromptField) -> bool {
    let Some(task) = state.selected_task() else {
        return false;
    };
    let text = match field {
        PromptField::Content => task.content.clone(),
        PromptField::Description => task.description.clone(),
        PromptField::Comment | PromptField::DueDate => String::new(),
    };
    let prompt = Prompt {
        field,
        task_id: task.id.clone(),
        subject: task.content.clone(),
        error: None,
    };
    state.edit_task = Some(prompt.task_id.clone());
    state.input = TextField::new(text);
    state.modals.push(Modal::Prompt(prompt));
    true
}

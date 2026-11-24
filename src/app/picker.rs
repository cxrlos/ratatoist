//! The one picker every choice uses: priority, sort, list filter, project
//! and theme. A filterable list to choose one entry from: typing narrows
//! it, the selection moves through what is left, and the first nine
//! entries left can be picked by number.

use super::modal::Modal;
use super::state::{ListFilter, Sort, State};
use super::task_form::FormField;
use crate::todoist::model::Priority;
use crate::ui::theme;

/// What a picker chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    /// The priority of the task [`State::edit_task`] names, or of the task
    /// in the add-task form below the picker.
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

/// What an entry stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Priority(Priority),
    Sort(Sort),
    Filter(ListFilter),
    /// A project, by id.
    Project(String),
    /// A loaded theme, by its index in [`State::themes`].
    Theme(usize),
}

/// One choice: what the list shows and filters on, and what it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub value: Choice,
}

/// An open picker: what it chooses, its title naming the subject, its
/// entries, the text typed to filter them, and which entry is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub kind: PickerKind,
    /// Names what is being chosen, as `Priority of Renew passport photos`.
    pub title: String,
    entries: Vec<Entry>,
    query: String,
    /// An index into `entries`. When the query hides it, the first entry
    /// left is selected instead.
    selected: usize,
    /// The entry that holds the value in force now, marked in the list.
    current: Option<usize>,
}

impl Picker {
    /// A picker over `entries` with nothing typed. The `current` entry is
    /// marked and selected first.
    pub fn new(
        kind: PickerKind,
        title: String,
        entries: Vec<Entry>,
        current: Option<usize>,
    ) -> Self {
        Self {
            kind,
            title,
            entries,
            query: String::new(),
            selected: current.unwrap_or(0),
            current,
        }
    }

    /// What the user typed. The text-entry hook edits it in place.
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn query_mut(&mut self) -> &mut String {
        &mut self.query
    }

    /// The entries whose label contains every word typed, ignoring case, in
    /// their order, with their index among all entries.
    pub fn visible(&self) -> Vec<(usize, &Entry)> {
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let label = entry.label.to_lowercase();
                words.iter().all(|word| label.contains(word))
            })
            .collect()
    }

    /// Where the selection is among the visible entries, `None` when none
    /// are visible.
    pub fn selected_position(&self) -> Option<usize> {
        let visible = self.visible();
        if visible.is_empty() {
            return None;
        }
        Some(
            visible
                .iter()
                .position(|(index, _)| *index == self.selected)
                .unwrap_or(0),
        )
    }

    /// Whether `index` among all entries holds the value in force now.
    pub fn is_current(&self, index: usize) -> bool {
        self.current == Some(index)
    }

    /// Moves the selection one visible entry down (`step` 1) or up (`step`
    /// -1), stopping at either end. Returns whether it moved.
    pub fn select_by(&mut self, step: isize) -> bool {
        let Some(position) = self.selected_position() else {
            return false;
        };
        let visible = self.visible();
        let last = visible.len() - 1;
        let target = position.saturating_add_signed(step).min(last);
        let before = visible[position].0;
        self.selected = visible[target].0;
        self.selected != before
    }

    /// The selected entry's value.
    pub fn selected(&self) -> Option<&Choice> {
        let position = self.selected_position()?;
        self.visible().get(position).map(|(_, entry)| &entry.value)
    }

    /// The value of the `n`th visible entry, counting from 1.
    pub fn nth(&self, n: usize) -> Option<&Choice> {
        let index = n.checked_sub(1)?;
        self.visible().get(index).map(|(_, entry)| &entry.value)
    }
}

/// Opens the picker that chooses `kind` over the screen. A priority picker
/// writes to the add-task form when one is open below it, otherwise to the
/// task [`State::edit_task`] names, so the caller sets that first.
pub fn open_picker(state: &mut State, kind: PickerKind) {
    let (title, entries, current) = match kind {
        PickerKind::Priority => priority_entries(state),
        PickerKind::Sort => {
            let entries: Vec<Entry> = Sort::ALL
                .into_iter()
                .map(|sort| entry(sort.label(), Choice::Sort(sort)))
                .collect();
            let current = Sort::ALL.iter().position(|sort| *sort == state.sort);
            (format!("Sort {} by", state.list_title()), entries, current)
        }
        PickerKind::Filter => {
            let entries: Vec<Entry> = ListFilter::ALL
                .into_iter()
                .map(|filter| entry(filter.label(), Choice::Filter(filter)))
                .collect();
            let current = ListFilter::ALL
                .iter()
                .position(|filter| *filter == state.list_filter);
            (format!("Show in {}", state.list_title()), entries, current)
        }
        PickerKind::Project => {
            let current_id = match state.top_modal() {
                Some(Modal::Form(form)) => form.project_id.clone(),
                _ => String::new(),
            };
            let entries: Vec<Entry> = state
                .store
                .projects
                .iter()
                .filter(|project| {
                    project.is_deleted != Some(true) && project.is_archived != Some(true)
                })
                .map(|project| entry(&project.name, Choice::Project(project.id.clone())))
                .collect();
            let current = entries
                .iter()
                .position(|e| e.value == Choice::Project(current_id.clone()));
            (
                format!("Project of {}", form_subject(state)),
                entries,
                current,
            )
        }
        PickerKind::Theme => {
            let entries: Vec<Entry> = state
                .themes
                .iter()
                .enumerate()
                .map(|(index, theme)| entry(&theme.name, Choice::Theme(index)))
                .collect();
            let current = (state.theme_idx < entries.len()).then_some(state.theme_idx);
            ("Theme for this session".to_owned(), entries, current)
        }
    };
    state
        .modals
        .push(Modal::Picker(Picker::new(kind, title, entries, current)));
}

fn entry(label: &str, value: Choice) -> Entry {
    Entry {
        label: label.to_owned(),
        value,
    }
}

/// The priority picker's title, entries and current entry: of the task in
/// the add-task form when it is on top, otherwise of the task being edited.
fn priority_entries(state: &State) -> (String, Vec<Entry>, Option<usize>) {
    let entries: Vec<Entry> = Priority::ALL
        .into_iter()
        .map(|priority| entry(priority.label(), Choice::Priority(priority)))
        .collect();
    let (subject, api) = match state.top_modal() {
        Some(Modal::Form(form)) => (form_subject(state), form.priority),
        _ => {
            let task = state
                .edit_task
                .as_deref()
                .and_then(|id| state.store.tasks.iter().find(|task| task.id == id));
            (
                task.map_or_else(String::new, |task| task.content.clone()),
                task.map_or(1, |task| task.priority),
            )
        }
    };
    let current = Priority::ALL
        .iter()
        .position(|priority| *priority == Priority::from_api(api));
    (format!("Priority of {subject}"), entries, current)
}

/// The add-task form's task, by its content once it has some.
fn form_subject(state: &State) -> String {
    let content = match state.top_modal() {
        Some(Modal::Form(form)) if form.field == FormField::Content => {
            state.input.text().trim().to_owned()
        }
        Some(Modal::Form(form)) => form.content.trim().to_owned(),
        _ => String::new(),
    };
    if content.is_empty() {
        "the new task".to_owned()
    } else {
        content
    }
}

/// The notice after choosing the theme `name`: the `config.toml` line that
/// keeps it past this session.
pub(super) fn theme_notice(name: &str) -> String {
    format!(
        "{name} is the theme for this session; to keep it, add to config.toml: [theme] name = \"{}\"",
        theme::slug(name)
    )
}

#[cfg(test)]
mod tests {
    use super::{Choice, Entry, Picker, PickerKind};

    fn picker(labels: &[&str], current: Option<usize>) -> Picker {
        let entries = labels
            .iter()
            .enumerate()
            .map(|(value, label)| Entry {
                label: (*label).to_owned(),
                value: Choice::Theme(value),
            })
            .collect();
        Picker::new(PickerKind::Theme, String::new(), entries, current)
    }

    #[test]
    fn every_typed_word_must_appear_ignoring_case() {
        let mut list = picker(&["Due today", "Due this week", "Overdue"], None);

        list.query_mut().push_str("WEEK due");

        let labels: Vec<&str> = list
            .visible()
            .iter()
            .map(|(_, e)| e.label.as_str())
            .collect();
        assert_eq!(labels, ["Due this week"]);
    }

    #[test]
    fn numbers_count_the_entries_the_filter_leaves() {
        let mut list = picker(&["Open", "Completed", "All"], None);

        list.query_mut().push('l');

        assert_eq!(list.nth(1), Some(&Choice::Theme(1)));
        assert_eq!(list.nth(2), Some(&Choice::Theme(2)));
        assert_eq!(list.nth(3), None);
        assert_eq!(list.nth(0), None);
    }

    #[test]
    fn the_current_entry_starts_selected_and_a_hidden_selection_falls_to_the_first_left() {
        let mut list = picker(&["P1", "P2", "P3"], Some(2));
        assert_eq!(list.selected(), Some(&Choice::Theme(2)));

        list.query_mut().push('2');

        assert_eq!(list.selected(), Some(&Choice::Theme(1)));
        list.query_mut().push('x');
        assert_eq!(list.selected(), None);
        assert_eq!(list.selected_position(), None);
    }

    #[test]
    fn the_selection_stops_at_either_end() {
        let mut list = picker(&["a", "b"], None);

        assert!(!list.select_by(-1));
        assert!(list.select_by(1));
        assert!(!list.select_by(1));
        assert_eq!(list.selected(), Some(&Choice::Theme(1)));
    }
}

//! The Projects and Labels screens: the tree of projects (the personal
//! space, then each workspace with its folders) and the list of labels,
//! each row with its open-task count. Enter lists the focused project's or
//! label's tasks, so every task action works on them, and Esc comes back
//! here with the focus where it was.

use super::effect::Effect;
use super::state::{Pane, Screen, State};
use super::sync::OptimisticOp;
use super::{Action, ListFilter};
use crate::todoist::model::{Project, Task};
use crate::todoist::sync::SyncCommand;

/// A row of the Projects tree the focus can rest on, by id so it stays
/// put while projects change around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeItem {
    Folder(String),
    Project(String),
}

/// One row of the Projects tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    /// The head of a space: the personal one (`None`) or a workspace, by
    /// its name.
    Space(Option<String>),
    /// A folder of a workspace and whether its projects show.
    Folder {
        id: String,
        name: String,
        open: bool,
    },
    /// The project at this index of `State::projects`, `depth` levels
    /// under its top-level project, inside a folder or not.
    Project {
        index: usize,
        depth: usize,
        in_folder: bool,
    },
}

impl TreeRow {
    /// The focusable item this row is, if any.
    pub fn item(&self, state: &State) -> Option<TreeItem> {
        match self {
            TreeRow::Space(_) => None,
            TreeRow::Folder { id, .. } => Some(TreeItem::Folder(id.clone())),
            TreeRow::Project { index, .. } => state
                .projects
                .get(*index)
                .map(|project| TreeItem::Project(project.id.clone())),
        }
    }
}

/// One row of the Labels screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRow {
    pub name: String,
    pub favourite: bool,
    /// How many open tasks carry it: the tasks its list shows.
    pub count: usize,
}

impl State {
    /// The Projects tree, in Todoist's order: the personal space, then
    /// each workspace with its projects outside a folder and then each
    /// folder's. The Inbox has its own screen and is left out; a folded
    /// folder keeps its row and hides its projects.
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let mut space: Option<Option<&str>> = None;
        let mut folder: Option<&str> = None;
        for (index, project) in self.projects.iter().enumerate() {
            if project.is_inbox() {
                continue;
            }
            let workspace = project.workspace_id.as_deref();
            if space != Some(workspace) {
                space = Some(workspace);
                folder = None;
                let name = workspace.map(|id| {
                    self.workspaces
                        .iter()
                        .find(|w| w.id == id)
                        .map_or_else(|| id.to_owned(), |w| w.name.clone())
                });
                rows.push(TreeRow::Space(name));
            }
            let folder_id = project.folder_id.as_deref();
            if folder_id.is_some() && folder != folder_id {
                folder = folder_id;
                if let Some(id) = folder_id {
                    let name = self
                        .folders
                        .iter()
                        .find(|f| f.id == id)
                        .map_or_else(|| id.to_owned(), |f| f.name.clone());
                    rows.push(TreeRow::Folder {
                        id: id.to_owned(),
                        name,
                        open: !self.collapsed_folders.contains(id),
                    });
                }
            }
            if folder_id.is_some_and(|id| self.collapsed_folders.contains(id)) {
                continue;
            }
            rows.push(TreeRow::Project {
                index,
                depth: self.project_depth(&project.id),
                in_folder: folder_id.is_some(),
            });
        }
        rows
    }

    /// Every label: the user's own, favourites first and then in their
    /// order, then those shared with the user that only tasks carry, by
    /// name.
    pub fn label_rows(&self) -> Vec<LabelRow> {
        let mut own: Vec<_> = self
            .labels
            .iter()
            .filter(|label| label.is_deleted != Some(true))
            .collect();
        own.sort_by_key(|label| (!label.is_favorite, label.item_order));
        let mut rows: Vec<LabelRow> = own
            .into_iter()
            .map(|label| LabelRow {
                name: label.name.clone(),
                favourite: label.is_favorite,
                count: 0,
            })
            .collect();
        let mut shared: Vec<&str> = self
            .tasks
            .iter()
            .flat_map(|task| task.labels.iter().map(String::as_str))
            .filter(|name| rows.iter().all(|row| row.name != *name))
            .collect();
        shared.sort_unstable();
        shared.dedup();
        rows.extend(shared.into_iter().map(|name| LabelRow {
            name: name.to_owned(),
            favourite: false,
            count: 0,
        }));
        for row in &mut rows {
            row.count = self
                .tasks
                .iter()
                .filter(|task| is_open(task) && task.labels.contains(&row.name))
                .count();
        }
        rows
    }

    /// How many open tasks project `id` holds, subtasks included: the
    /// tasks its list shows with nothing folded.
    pub fn open_task_count(&self, project_id: &str) -> usize {
        self.tasks
            .iter()
            .filter(|task| is_open(task) && task.project_id == project_id)
            .count()
    }

    /// The open tasks of the opened label that `filter` keeps, subtasks
    /// included unless it narrows to a due date or priority, grouped by
    /// project in the tree's order.
    pub(crate) fn label_tasks(&self, filter: ListFilter) -> Vec<(&Project, Vec<&Task>)> {
        let Some(label) = self.opened_label.as_deref() else {
            return Vec::new();
        };
        let mut groups = Vec::new();
        for project in &self.projects {
            let mut tasks: Vec<&Task> = self
                .tasks
                .iter()
                .filter(|task| {
                    is_open(task)
                        && task.project_id == project.id
                        && task.labels.iter().any(|name| name == label)
                        && match filter {
                            ListFilter::Open | ListFilter::All => true,
                            ListFilter::Completed => false,
                            _ => self.in_cross_project_list(task, filter),
                        }
                })
                .collect();
            tasks.sort_by_key(|task| (task.section_id.is_some(), task.child_order));
            if !tasks.is_empty() {
                groups.push((project, tasks));
            }
        }
        groups
    }

    /// The row the tree's focus is on: the one it rests on, or the first
    /// while it rests on none that shows.
    pub fn tree_focused(&self) -> Option<TreeItem> {
        let items = self.tree_items();
        self.tree_focus
            .clone()
            .filter(|focus| items.contains(focus))
            .or_else(|| items.into_iter().next())
    }

    /// The project the tree's focus is on.
    pub fn tree_project(&self) -> Option<&Project> {
        match self.tree_focused() {
            Some(TreeItem::Project(id)) => self.projects.iter().find(|p| p.id == id),
            _ => None,
        }
    }

    /// Where the focus is among the focusable rows of the tree or the
    /// labels: the first one when it rests on none.
    pub fn browse_focus_index(&self) -> usize {
        match self.screen {
            Screen::Labels => self
                .label_focus
                .as_ref()
                .and_then(|focus| self.label_rows().iter().position(|r| &r.name == focus))
                .unwrap_or(0),
            _ => {
                let items = self.tree_items();
                self.tree_focus
                    .as_ref()
                    .and_then(|focus| items.iter().position(|item| item == focus))
                    .unwrap_or(0)
            }
        }
    }

    fn tree_items(&self) -> Vec<TreeItem> {
        self.tree_rows()
            .iter()
            .filter_map(|row| row.item(self))
            .collect()
    }

    /// Moves the focus of the Projects tree or the labels by `action`.
    pub(crate) fn move_in_browse(&mut self, action: Action) -> bool {
        let len = match self.screen {
            Screen::Labels => self.label_rows().len(),
            _ => self.tree_items().len(),
        };
        let Some(last) = len.checked_sub(1) else {
            return false;
        };
        let current = self.browse_focus_index();
        let next = match action {
            Action::MoveDown => (current + 1).min(last),
            Action::MoveUp => current.saturating_sub(1),
            Action::MoveToTop => 0,
            Action::MoveToBottom => last,
            Action::HalfPageDown => current.saturating_add(self.half_page()).min(last),
            _ => current.saturating_sub(self.half_page()),
        };
        self.focus_browse_row(next);
        true
    }

    fn focus_browse_row(&mut self, index: usize) {
        match self.screen {
            Screen::Labels => {
                self.label_focus = self.label_rows().get(index).map(|row| row.name.clone());
            }
            _ => self.tree_focus = self.tree_items().get(index).cloned(),
        }
    }

    /// Opens the task list of the focused project or label.
    pub(crate) fn open_list(&mut self) -> bool {
        if self.active_pane != Pane::Browse {
            return false;
        }
        match self.screen {
            Screen::Projects => {
                let Some(id) = self.tree_project().map(|p| p.id.clone()) else {
                    return false;
                };
                let Some(index) = self.projects.iter().position(|p| p.id == id) else {
                    return false;
                };
                self.selected_project = index;
            }
            Screen::Labels => {
                let index = self.browse_focus_index();
                let Some(row) = self.label_rows().into_iter().nth(index) else {
                    return false;
                };
                self.label_focus = Some(row.name.clone());
                self.opened_label = Some(row.name);
            }
            Screen::Inbox | Screen::Today | Screen::Upcoming => return false,
        }
        self.active_pane = Pane::Tasks;
        self.list_filter = ListFilter::default();
        self.selected_task = 0;
        self.detail_scroll = 0;
        true
    }

    /// From a list opened on Projects or Labels back to its tree, focused
    /// on the project or label it listed.
    pub(crate) fn back_to_browse(&mut self) {
        if self.screen == Screen::Projects
            && let Some(project) = self.projects.get(self.selected_project)
        {
            self.tree_focus = Some(TreeItem::Project(project.id.clone()));
        }
        self.active_pane = Pane::Browse;
        self.list_filter = ListFilter::default();
        self.selected_task = 0;
        self.detail_scroll = 0;
    }

    /// Folds or unfolds the focused folder, or the folder of the focused
    /// project, which then takes the focus.
    pub(crate) fn toggle_folder(&mut self) -> bool {
        let folder = match self.tree_focused() {
            Some(TreeItem::Folder(id)) => Some(id),
            Some(TreeItem::Project(_)) => self.tree_project().and_then(|p| p.folder_id.clone()),
            None => None,
        };
        let Some(id) = folder else {
            return false;
        };
        if !self.collapsed_folders.remove(&id) {
            self.collapsed_folders.insert(id.clone());
        }
        self.tree_focus = Some(TreeItem::Folder(id));
        true
    }

    /// Marks the focused project as a favourite, or unmarks it, at once;
    /// a rejection restores it and marks its row.
    pub(crate) fn toggle_favourite(&mut self, effects: &mut Vec<Effect>) -> bool {
        let Some(project) = self.tree_project() else {
            return false;
        };
        let before = project.clone();
        let id = before.id.clone();
        let favourite = !before.is_favorite;
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == id) {
            project.is_favorite = favourite;
        }
        self.rolled_back_projects.remove(&id);
        self.sort_projects();
        let command = SyncCommand {
            r#type: "project_update".to_owned(),
            temp_id: None,
            uuid: super::sync::new_uuid(),
            args: serde_json::json!({ "id": id, "is_favorite": favourite }),
        };
        let op = OptimisticOp::ProjectUpdated {
            project_id: id,
            before: Box::new(before),
        };
        self.send_command(command, Some(op), effects);
        true
    }
}

/// Whether `task` is open: neither completed nor deleted.
fn is_open(task: &Task) -> bool {
    !task.checked && !task.is_deleted
}

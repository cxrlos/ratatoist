//! The palette: one query that fuzzy-finds every available action in the
//! registry, every loaded open task and every project, and Enter to run the
//! action, jump to the task or open the project's list.

use std::cmp::Reverse;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use super::update::{Update, perform};
use super::{Action, ListFilter, Modal, Pane, Screen, State};
use crate::keys::{self, ACTIONS, ActionSpec};
use crate::todoist::model::{Project, Task};

/// The open palette: what has been typed, and the palette_highlight row among the
/// matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    pub query: String,
    /// The palette_highlight match, counted from the best one.
    pub selected: usize,
}

/// One thing the palette can find.
#[derive(Debug, Clone, Copy)]
pub enum PaletteEntry<'a> {
    Action(&'static ActionSpec),
    Task(&'a Task),
    Project(&'a Project),
}

impl PaletteEntry<'_> {
    /// Where it sorts among entries with the same score: actions, then
    /// tasks, then projects.
    fn rank(&self) -> u8 {
        match self {
            PaletteEntry::Action(_) => 0,
            PaletteEntry::Task(_) => 1,
            PaletteEntry::Project(_) => 2,
        }
    }
}

/// Every action that applies to the screen under the palette and can run
/// now, every loaded open task and every project matching `query`, best
/// first. Each word of the query must match fuzzily, ignoring case, in an
/// action's description or id, a task's content or a project's name. Ties
/// keep actions in registry order, then tasks, then projects.
pub fn palette_matches<'a>(state: &'a State, query: &str) -> Vec<PaletteEntry<'a>> {
    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buffer = Vec::new();
    let mut score = |text: &str| pattern.score(Utf32Str::new(text, &mut buffer), &mut matcher);

    // Overlay actions never match these contexts, so they are left out
    // before availability is asked: the availability of the palette's own
    // keys asks for these matches. Closing and reopening the palette are
    // what Esc and its key already do.
    let contexts = keys::screen_contexts(state);
    let actions = ACTIONS
        .iter()
        .filter(|spec| spec.contexts.iter().any(|c| contexts.contains(c)))
        .filter(|spec| !matches!(spec.action, Action::Close | Action::ShowPalette))
        .filter(|spec| spec.availability(state).can_run())
        .map(PaletteEntry::Action);
    let tasks = state
        .store
        .tasks
        .iter()
        .filter(|task| !task.checked && !task.is_deleted)
        .map(PaletteEntry::Task);
    let projects = state.store.projects.iter().map(PaletteEntry::Project);

    let mut found: Vec<(u32, PaletteEntry)> = actions
        .chain(tasks)
        .chain(projects)
        .filter_map(|entry| {
            let text = match entry {
                PaletteEntry::Action(spec) => format!("{} {}", spec.description_in(state), spec.id),
                PaletteEntry::Task(task) => task.content.clone(),
                PaletteEntry::Project(project) => project.name.clone(),
            };
            Some((score(&text)?, entry))
        })
        .collect();
    found.sort_by_key(|(score, entry)| (Reverse(*score), entry.rank()));
    found.into_iter().map(|(_, entry)| entry).collect()
}

/// `selected` as an index into `matches` matches: a highlight left past
/// the end, as when a sync drops matches while the palette is open, is on
/// the last one.
pub fn palette_highlight(selected: usize, matches: usize) -> usize {
    selected.min(matches.saturating_sub(1))
}

/// Moves the highlight one match down or up, within the matches.
pub(super) fn step(state: &mut State, action: Action) -> bool {
    let Some(Modal::Palette(palette)) = state.top_modal() else {
        return false;
    };
    let matches = palette_matches(state, &palette.query).len();
    let Some(Modal::Palette(palette)) = state.top_modal_mut() else {
        return false;
    };
    let before = palette.selected;
    let current = palette_highlight(palette.selected, matches);
    palette.selected = match action {
        Action::ScrollDown => palette_highlight(current.saturating_add(1), matches),
        _ => current.saturating_sub(1),
    };
    palette.selected != before
}

/// Whether the highlight can move one match down.
pub fn palette_can_step_down(state: &State) -> bool {
    match state.top_modal() {
        Some(Modal::Palette(palette)) => {
            let matches = palette_matches(state, &palette.query).len();
            palette_highlight(palette.selected, matches) + 1 < matches
        }
        _ => false,
    }
}

/// Whether a match is palette_highlight for Enter to take.
pub fn palette_has_selection(state: &State) -> bool {
    match state.top_modal() {
        Some(Modal::Palette(palette)) => !palette_matches(state, &palette.query).is_empty(),
        _ => false,
    }
}

/// The palette_highlight match, as what Enter does with it.
enum Chosen {
    Action(Action),
    Task(String),
    Project(usize),
}

/// Runs the palette_highlight action on what is under the palette, jumps to the
/// palette_highlight task, or opens the palette_highlight project's list. With nothing
/// palette_highlight the palette stays open.
pub(super) fn accept(state: &mut State) -> Update {
    let Some(Modal::Palette(palette)) = state.top_modal() else {
        return Update::default();
    };
    let matches = palette_matches(state, &palette.query);
    let chosen = match matches.get(palette_highlight(palette.selected, matches.len())) {
        Some(PaletteEntry::Action(spec)) => Chosen::Action(spec.action),
        Some(PaletteEntry::Task(task)) => Chosen::Task(task.id.clone()),
        Some(PaletteEntry::Project(project)) => {
            match state.store.projects.iter().position(|p| p.id == project.id) {
                Some(index) => Chosen::Project(index),
                None => return Update::default(),
            }
        }
        None => return Update::default(),
    };
    state.modals.pop();
    let mut update = match chosen {
        Chosen::Action(action) => perform(state, action),
        Chosen::Task(id) => {
            jump_to(state, &id);
            Update::default()
        }
        Chosen::Project(index) => {
            open_project(state, index);
            Update::default()
        }
    };
    update.redraw = true;
    update
}

/// Opens the task list of the project at `index`, from its first row: on
/// the Inbox screen for the Inbox, on the Projects screen otherwise, with
/// no list filter so every open task shows.
fn open_project(state: &mut State, index: usize) {
    let inbox = state
        .store
        .projects
        .get(index)
        .is_some_and(|p| p.is_inbox());
    state.screen = if inbox {
        Screen::Inbox
    } else {
        Screen::Projects
    };
    state.selected_project = index;
    state.list_filter = ListFilter::default();
    state.selected_task = 0;
    state.detail_scroll = 0;
    state.active_pane = Pane::Tasks;
}

/// Focuses the task `id` in its project's list, unfolding the tasks above
/// it so it shows.
fn jump_to(state: &mut State, id: &str) {
    let Some(task) = state.store.tasks.iter().find(|task| task.id == id) else {
        return;
    };
    let project_id = task.project_id.clone();
    let mut parent = task.parent_id.clone();
    while let Some(parent_id) = parent {
        state.collapsed.remove(&parent_id);
        parent = state
            .store
            .tasks
            .iter()
            .find(|task| task.id == parent_id)
            .and_then(|task| task.parent_id.clone());
    }
    let index = state
        .store
        .projects
        .iter()
        .position(|project| project.id == project_id)
        .unwrap_or(state.selected_project);
    open_project(state, index);
    if let Some(row) = state.visible_tasks().iter().position(|task| task.id == id) {
        state.selected_task = row;
    }
}

//! What the screen derives from [`State`]: the rows of the task lists, the
//! counts and the stats. They are built once into [`Lists`] from the store
//! and the list state, and rebuilt on the first read after either changes,
//! so a frame or a key reads them instead of walking every task again.

use std::collections::{HashMap, HashSet};

use jiff::civil::Date;
use jiff::tz::TimeZone;

use super::browse::LabelRow;
use super::state::{COUNTED_FILTERS, ListFilter, OverviewStats, Pane, Screen, Sort, State};
use super::store::{Keyed, Table};
use crate::todoist::model::{Priority, Project, Task};
use crate::ui::dates::{self, DueBucket};

/// What the lists are built from; when any of it changes they are rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListKey {
    tasks: u64,
    projects: u64,
    sections: u64,
    labels: u64,
    completed: u64,
    collapsed: u64,
    user: Option<String>,
    today: Date,
    zone: TimeZone,
    screen: Screen,
    browsing: bool,
    filter: ListFilter,
    sort: Sort,
    selected_project: usize,
    opened_label: Option<String>,
    overdue_folded: bool,
}

/// Where a row's task is: at a position of the store's tasks, or of the
/// completed tasks loaded for the listed project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum At {
    Store(usize),
    Completed(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    Header(Group),
    Task { at: At, depth: usize },
}

/// The lists derived from the store and the list state.
#[derive(Debug, Clone)]
pub(crate) struct Lists {
    key: ListKey,
    tree: Tree,
    rows: Vec<Row>,
    /// The task rows of `rows`, in order.
    tasks: Vec<At>,
    /// How many tasks the list shows with the default list filter.
    default_count: usize,
    /// The project whose completed tasks the rows list.
    completed_of: Option<String>,
    /// The tasks above a loaded completed task, under the Completed list
    /// filter: the open parents shown dimmed as its context.
    completed_ancestors: HashSet<String>,
    open_by_project: HashMap<String, usize>,
    label_rows: Vec<LabelRow>,
}

/// A task's parent, as the lists nest it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Parent {
    None,
    At(usize),
    /// The parent is not loaded: the task is a subtask, under nothing that
    /// shows.
    Missing,
}

/// How the store's tasks nest, by their position in it. A task whose
/// parents loop back to it is top level here.
#[derive(Debug, Clone, Default)]
struct Tree {
    parent: Vec<Parent>,
    /// Each task's subtasks, in Todoist's order.
    children: Vec<Vec<usize>>,
    depth: Vec<usize>,
    /// Whether a completed task sits somewhere under each task.
    completed_below: Vec<bool>,
}

impl Tree {
    fn of(tasks: &Table<Task>) -> Self {
        let mut parent = parents(tasks, |task| task.parent_id.as_deref());
        for member in cycles(&parent).into_iter().flatten() {
            if let Some(slot) = parent.get_mut(member) {
                *slot = Parent::None;
            }
        }
        let mut children = vec![Vec::new(); tasks.len()];
        for (at, up) in parent.iter().enumerate() {
            if let Parent::At(up) = up
                && let Some(list) = children.get_mut(*up)
            {
                list.push(at);
            }
        }
        for list in &mut children {
            list.sort_by_key(|&at| tasks.get(at).map(|task| task.child_order));
        }
        let mut completed_below = vec![false; tasks.len()];
        for (at, task) in tasks.iter().enumerate() {
            if !task.checked || task.is_deleted {
                continue;
            }
            let mut up = parent.get(at).copied();
            while let Some(Parent::At(above)) = up {
                match completed_below.get_mut(above) {
                    Some(seen) if !*seen => *seen = true,
                    _ => break,
                }
                up = parent.get(above).copied();
            }
        }
        Self {
            depth: depths(&parent),
            parent,
            children,
            completed_below,
        }
    }

    fn is_top_level(&self, at: usize) -> bool {
        self.parent.get(at) == Some(&Parent::None)
    }

    fn children(&self, at: usize) -> &[usize] {
        self.children.get(at).map_or(&[], Vec::as_slice)
    }

    fn depth(&self, at: usize) -> usize {
        self.depth.get(at).copied().unwrap_or(0)
    }

    fn completed_below(&self, at: usize) -> bool {
        self.completed_below.get(at).copied().unwrap_or(false)
    }
}

/// Each entity's parent in `table`, by position.
pub(crate) fn parents<T: Keyed>(
    table: &Table<T>,
    parent_of: impl Fn(&T) -> Option<&str>,
) -> Vec<Parent> {
    table
        .iter()
        .map(|item| match parent_of(item) {
            None => Parent::None,
            Some(id) => table.position(id).map_or(Parent::Missing, Parent::At),
        })
        .collect()
}

/// The loops in `parent`, each as the positions on it. Every walk stops at
/// a position it has seen, so it ends whatever the parents are.
pub(crate) fn cycles(parent: &[Parent]) -> Vec<Vec<usize>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Seen {
        No,
        OnPath,
        Done,
    }
    let mut seen = vec![Seen::No; parent.len()];
    let mut found = Vec::new();
    for start in 0..parent.len() {
        let mut path = Vec::new();
        let mut at = start;
        loop {
            match seen.get(at) {
                Some(Seen::No) => {
                    seen[at] = Seen::OnPath;
                    path.push(at);
                    match parent.get(at) {
                        Some(Parent::At(up)) => at = *up,
                        _ => break,
                    }
                }
                Some(Seen::OnPath) => {
                    if let Some(from) = path.iter().position(|&p| p == at) {
                        found.push(path[from..].to_vec());
                    }
                    break;
                }
                _ => break,
            }
        }
        for at in path {
            seen[at] = Seen::Done;
        }
    }
    found
}

/// How many parents up each task's top-level task is; a missing parent
/// counts as one. `parent` has no loops.
fn depths(parent: &[Parent]) -> Vec<usize> {
    let mut depth: Vec<Option<usize>> = vec![None; parent.len()];
    for start in 0..parent.len() {
        let mut path = Vec::new();
        let mut at = Some(start);
        let mut base = 0;
        while let Some(i) = at {
            if let Some(Some(known)) = depth.get(i) {
                base = known + 1;
                break;
            }
            if path.len() > parent.len() {
                break;
            }
            path.push(i);
            at = match parent.get(i) {
                Some(Parent::At(up)) => Some(*up),
                Some(Parent::Missing) => {
                    base = 1;
                    None
                }
                _ => {
                    base = 0;
                    None
                }
            };
        }
        for i in path.into_iter().rev() {
            depth[i] = Some(base);
            base += 1;
        }
    }
    depth.into_iter().map(Option::unwrap_or_default).collect()
}

impl State {
    /// Runs `read` on the lists, building them first when the store or the
    /// list state changed since they were built.
    pub(crate) fn with_lists<R>(&self, read: impl FnOnce(&Lists) -> R) -> R {
        let key = self.list_key();
        if let Ok(slot) = self.lists.try_borrow()
            && let Some(lists) = slot.as_ref().filter(|lists| lists.key == key)
        {
            return read(lists);
        }
        let lists = self.build_lists(key);
        let value = read(&lists);
        if let Ok(mut slot) = self.lists.try_borrow_mut() {
            *slot = Some(lists);
        }
        value
    }

    fn list_key(&self) -> ListKey {
        ListKey {
            tasks: self.store.tasks.stamp(),
            projects: self.store.projects.stamp(),
            sections: self.store.sections.stamp(),
            labels: self.store.labels.stamp(),
            completed: self.completed_cache.stamp(),
            collapsed: self.collapsed.stamp(),
            user: self.store.current_user_id.clone(),
            today: self.today,
            zone: self.now.time_zone().clone(),
            screen: self.screen,
            browsing: self.active_pane == Pane::Browse,
            filter: self.list_filter,
            sort: self.sort,
            selected_project: self.selected_project,
            opened_label: self.opened_label.clone(),
            overdue_folded: self.overdue_section_collapsed,
        }
    }

    fn build_lists(&self, key: ListKey) -> Lists {
        let tree = Tree::of(&self.store.tasks);
        let rows = self.rows_with(&tree, key.filter);
        let tasks: Vec<At> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Task { at, .. } => Some(*at),
                Row::Header(_) => None,
            })
            .collect();
        let default_count = if key.filter == ListFilter::default() {
            tasks.len()
        } else {
            self.rows_with(&tree, ListFilter::default())
                .iter()
                .filter(|row| matches!(row, Row::Task { .. }))
                .count()
        };
        let completed_of = self.list_project().map(|p| p.id.clone());
        let completed_ancestors = match (&completed_of, key.filter) {
            (Some(project_id), ListFilter::Completed) => self
                .completed_cache
                .get(project_id)
                .map(|cached| ancestors_within(cached))
                .unwrap_or_default(),
            _ => HashSet::new(),
        };
        let mut open_by_project: HashMap<String, usize> = HashMap::new();
        for task in self.store.tasks.iter().filter(|task| is_open(task)) {
            *open_by_project.entry(task.project_id.clone()).or_default() += 1;
        }
        Lists {
            label_rows: self.build_label_rows(),
            key,
            tree,
            rows,
            tasks,
            default_count,
            completed_of,
            completed_ancestors,
            open_by_project,
        }
    }

    /// The task at `at`, as the lists placed it.
    fn task_at<'s>(&'s self, lists: &Lists, at: At) -> Option<&'s Task> {
        match at {
            At::Store(index) => self.store.tasks.get(index),
            At::Completed(index) => lists
                .completed_of
                .as_deref()
                .and_then(|project_id| self.completed_cache.get(project_id))
                .and_then(|cached| cached.get(index)),
        }
    }

    /// Where `task`'s due date falls today.
    pub fn due_bucket(&self, task: &Task) -> DueBucket {
        dates::due_bucket(task.due.as_ref(), self.today, self.now.time_zone())
    }

    /// Unchecked, not deleted, and unassigned or assigned to the user: the
    /// tasks every count and cross-project list is made of.
    pub fn is_open_and_mine(&self, task: &Task) -> bool {
        !task.checked
            && !task.is_deleted
            && task
                .responsible_uid
                .as_deref()
                .is_none_or(|uid| self.store.current_user_id.as_deref() == Some(uid))
    }

    /// Whether the task at `at` is a row of a list narrowed to `filter`, a
    /// due bucket or a priority. The stats count these tasks across every
    /// project, while a list applies the filter within its own project, so a
    /// count can exceed the rows a filtered list shows. Subtasks are left
    /// out of both, as the Today list leaves them out. Open, completed and
    /// all narrow nothing here.
    fn narrowed_to(&self, tree: &Tree, at: usize, filter: ListFilter) -> bool {
        let Some(task) = self.store.tasks.get(at) else {
            return false;
        };
        if !tree.is_top_level(at) || !self.is_open_and_mine(task) {
            return false;
        }
        match filter {
            ListFilter::Overdue => self.due_bucket(task) == DueBucket::Overdue,
            ListFilter::DueToday => self.due_bucket(task) == DueBucket::Today,
            ListFilter::DueThisWeek => matches!(
                self.due_bucket(task),
                DueBucket::Today | DueBucket::ThisWeek
            ),
            ListFilter::Priority(priority) => Priority::from_api(task.priority) == priority,
            ListFilter::Open | ListFilter::Completed | ListFilter::All => true,
        }
    }

    /// How many projects up `project_id`'s top-level project is. A project
    /// whose parents loop back shows at top level.
    pub fn project_depth(&self, project_id: &str) -> usize {
        let mut depth = 0;
        let mut seen = HashSet::new();
        let mut current = project_id;
        while let Some(parent_id) = self
            .store
            .projects
            .by_id(current)
            .and_then(|p| p.parent_id.as_deref())
        {
            if !seen.insert(current) {
                return 0;
            }
            depth += 1;
            current = parent_id;
        }
        depth
    }

    /// The project whose tasks the list shows: the Inbox on the Inbox
    /// screen and the opened project on the Projects screen. The other
    /// screens list tasks across projects.
    pub fn list_project(&self) -> Option<&Project> {
        match self.screen {
            Screen::Inbox => self.store.projects.iter().find(|p| p.is_inbox()),
            Screen::Projects => self.store.projects.get(self.selected_project),
            Screen::Today | Screen::Upcoming | Screen::Labels => None,
        }
    }

    /// The title of the task list's pane: the screen's name, or the
    /// project's where it lists one project.
    pub fn list_title(&self) -> String {
        match (self.list_project(), &self.opened_label) {
            (Some(project), _) => project.name.clone(),
            (None, Some(label)) if self.screen == Screen::Labels => format!("@{label}"),
            (None, _) => self.screen.title().to_owned(),
        }
    }

    /// The one muted sentence an empty task list shows.
    pub fn empty_list_sentence(&self) -> String {
        if self.list_filter != ListFilter::default() {
            return format!(
                "No tasks match {}.",
                self.list_filter.label().to_lowercase()
            );
        }
        match self.screen {
            Screen::Today => "Nothing due today.".to_owned(),
            Screen::Upcoming => "Nothing due in the next seven days.".to_owned(),
            _ => format!("No tasks in {}.", self.list_title()),
        }
    }

    pub fn selected_project_name(&self) -> &str {
        self.store
            .projects
            .get(self.selected_project)
            .map(|p| p.name.as_str())
            .unwrap_or("Tasks")
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.with_lists(|lists| {
            lists
                .tasks
                .get(self.selected_task)
                .and_then(|at| self.task_at(lists, *at))
        })
    }

    pub fn overview_stats(&self) -> OverviewStats {
        let mut stats = OverviewStats {
            due_today: 0,
            due_week: 0,
            overdue: 0,
            by_priority: [0; 5],
        };
        self.with_lists(|lists| {
            for filter in COUNTED_FILTERS {
                let count = (0..self.store.tasks.len())
                    .filter(|&at| self.narrowed_to(&lists.tree, at, filter))
                    .count();
                let count = u32::try_from(count).unwrap_or(u32::MAX);
                match filter {
                    ListFilter::Overdue => stats.overdue = count,
                    ListFilter::DueToday => stats.due_today = count,
                    ListFilter::DueThisWeek => stats.due_week = count,
                    ListFilter::Priority(p) => {
                        if let Some(slot) = stats.by_priority.get_mut(usize::from(p.api())) {
                            *slot = count;
                        }
                    }
                    ListFilter::Open | ListFilter::Completed | ListFilter::All => {}
                }
            }
        });
        stats
    }

    /// Whether task `task_id` has subtasks that nest under it.
    pub fn has_children(&self, task_id: &str) -> bool {
        let Some(at) = self.store.tasks.position(task_id) else {
            return false;
        };
        self.with_lists(|lists| !lists.tree.children(at).is_empty())
    }

    pub fn is_collapsed(&self, task_id: &str) -> bool {
        self.collapsed.contains(task_id)
    }

    /// The tasks of the list on screen, in order: the task rows of
    /// [`State::list_rows`]. Empty while a tree has the focus.
    pub fn visible_tasks(&self) -> Vec<&Task> {
        self.with_lists(|lists| {
            lists
                .tasks
                .iter()
                .filter_map(|at| self.task_at(lists, *at))
                .collect()
        })
    }

    /// The rows of the task list on the current screen, group headers and
    /// tasks, in order. Empty while a tree has the focus.
    pub fn list_rows(&self) -> Vec<ListRow<'_>> {
        self.with_lists(|lists| {
            lists
                .rows
                .iter()
                .filter_map(|row| match row {
                    Row::Header(group) => Some(ListRow::Header(group.clone())),
                    Row::Task { at, depth } => self.task_at(lists, *at).map(|task| ListRow::Task {
                        task,
                        depth: *depth,
                    }),
                })
                .collect()
        })
    }

    /// How many tasks the list on screen shows with `filter`: the `of 42`
    /// of the list filter header.
    pub fn task_count_with(&self, filter: ListFilter) -> usize {
        self.with_lists(|lists| {
            if filter == lists.key.filter {
                lists.tasks.len()
            } else if filter == ListFilter::default() {
                lists.default_count
            } else {
                self.rows_with(&lists.tree, filter)
                    .iter()
                    .filter(|row| matches!(row, Row::Task { .. }))
                    .count()
            }
        })
    }

    /// The labels of the Labels screen; see [`State::build_label_rows`].
    pub fn label_rows(&self) -> Vec<LabelRow> {
        self.with_lists(|lists| lists.label_rows.clone())
    }

    /// How many open tasks project `id` holds, subtasks included: the
    /// tasks its list shows with nothing folded.
    pub fn open_task_count(&self, project_id: &str) -> usize {
        self.with_lists(|lists| lists.open_by_project.get(project_id).copied().unwrap_or(0))
    }

    fn rows_with(&self, tree: &Tree, filter: ListFilter) -> Vec<Row> {
        if self.active_pane == Pane::Browse {
            return Vec::new();
        }
        match self.screen {
            Screen::Today => self.today_rows(tree, filter),
            Screen::Upcoming => self.upcoming_rows(tree, filter),
            Screen::Labels => self.label_list_rows(tree, filter),
            Screen::Inbox | Screen::Projects => self.project_rows(tree, filter),
        }
    }

    /// Open tasks due today or in the six days after, by day, then by
    /// their order in the project. Subtasks are left out, as on Today.
    fn upcoming_with(&self, tree: &Tree, filter: ListFilter) -> Vec<usize> {
        let tasks = &self.store.tasks;
        let mut upcoming: Vec<usize> = (0..tasks.len())
            .filter(|&at| {
                self.narrowed_to(tree, at, ListFilter::DueThisWeek)
                    && self.in_cross_project_list(tree, at, filter)
            })
            .collect();
        upcoming.sort_by(|&a, &b| {
            let (a, b) = (&tasks[a], &tasks[b]);
            let day = |t: &Task| (self.due_day(t), t.due.as_ref().map(|d| d.date.clone()));
            day(a).cmp(&day(b)).then(a.child_order.cmp(&b.child_order))
        });
        upcoming
    }

    /// The Upcoming tasks under a header for each day.
    fn upcoming_rows(&self, tree: &Tree, filter: ListFilter) -> Vec<Row> {
        let tasks = self.upcoming_with(tree, filter);
        let day_of = |at: usize| self.store.tasks.get(at).and_then(|task| self.due_day(task));
        let mut rows = Vec::with_capacity(tasks.len() + 7);
        let mut index = 0;
        while let Some(&first) = tasks.get(index) {
            let day = day_of(first);
            let group: Vec<usize> = tasks[index..]
                .iter()
                .take_while(|&&at| day_of(at) == day)
                .copied()
                .collect();
            index += group.len();
            push_group(&mut rows, GroupKind::Day(day), false, group);
        }
        rows
    }

    /// The opened label's tasks under a header for each project.
    fn label_list_rows(&self, tree: &Tree, filter: ListFilter) -> Vec<Row> {
        let mut rows = Vec::new();
        for (project, tasks) in self.label_tasks(tree, filter) {
            push_group(
                &mut rows,
                GroupKind::Project(project.name.clone()),
                false,
                tasks,
            );
        }
        rows
    }

    /// The open tasks of the opened label that `filter` keeps, subtasks
    /// included unless it narrows to a due date or priority, grouped by
    /// project in the tree's order.
    fn label_tasks(&self, tree: &Tree, filter: ListFilter) -> Vec<(&Project, Vec<usize>)> {
        let Some(label) = self.opened_label.as_deref() else {
            return Vec::new();
        };
        let mut by_project: HashMap<&str, Vec<usize>> = HashMap::new();
        for (at, task) in self.store.tasks.iter().enumerate() {
            let kept = is_open(task)
                && task.labels.iter().any(|name| name == label)
                && match filter {
                    ListFilter::Open | ListFilter::All => true,
                    ListFilter::Completed => false,
                    _ => self.in_cross_project_list(tree, at, filter),
                };
            if kept {
                by_project.entry(&task.project_id).or_default().push(at);
            }
        }
        let mut groups = Vec::new();
        for project in self.store.projects.iter() {
            let Some(mut tasks) = by_project.remove(project.id.as_str()) else {
                continue;
            };
            tasks.sort_by_key(|&at| {
                let task = &self.store.tasks[at];
                (task.section_id.is_some(), task.child_order)
            });
            groups.push((project, tasks));
        }
        groups
    }

    /// Whether `filter` keeps the task at `at` in a list across projects
    /// (Today, Upcoming, a label's), which holds open tasks only: completed
    /// shows none of them, a due bucket or priority narrows them.
    fn in_cross_project_list(&self, tree: &Tree, at: usize, filter: ListFilter) -> bool {
        filter != ListFilter::Completed && self.narrowed_to(tree, at, filter)
    }

    /// The day `task` is due in the user's zone, when it has a due date
    /// that parses.
    pub fn due_day(&self, task: &Task) -> Option<Date> {
        dates::due_day(task.due.as_ref()?, self.now.time_zone())
    }

    /// Today: the Overdue group, which folds, then the tasks due today.
    /// A group with no tasks is left out; a folded one keeps its header.
    fn today_rows(&self, tree: &Tree, filter: ListFilter) -> Vec<Row> {
        let tasks = &self.store.tasks;
        let mut due: Vec<usize> = (0..tasks.len())
            .filter(|&at| {
                (self.narrowed_to(tree, at, ListFilter::Overdue)
                    || self.narrowed_to(tree, at, ListFilter::DueToday))
                    && self.in_cross_project_list(tree, at, filter)
            })
            .collect();
        due.sort_by(|&a, &b| {
            self.due_day(&tasks[a])
                .cmp(&self.due_day(&tasks[b]))
                .then(tasks[a].child_order.cmp(&tasks[b].child_order))
        });
        let (overdue, today): (Vec<usize>, Vec<usize>) = due
            .into_iter()
            .partition(|&at| self.due_bucket(&tasks[at]) == DueBucket::Overdue);
        let mut rows = Vec::with_capacity(overdue.len() + today.len() + 2);
        push_group(
            &mut rows,
            GroupKind::Overdue,
            self.overdue_section_collapsed,
            overdue,
        );
        push_group(&mut rows, GroupKind::Today, false, today);
        rows
    }

    /// The tasks of the listed project: each top-level task the list
    /// filter keeps, with its subtasks under it unless it is folded. In
    /// Todoist's order, open tasks and those narrowed to a due bucket or
    /// priority go under a header for each section when the project has
    /// sections; any other sort or filter lists them without headers.
    fn project_rows(&self, tree: &Tree, filter: ListFilter) -> Vec<Row> {
        let tasks = &self.store.tasks;
        let current_project_id = self.list_project().map(|p| p.id.as_str());
        let narrows = narrows(filter);

        let mut top_level: Vec<usize> = (0..tasks.len())
            .filter(|&at| {
                let t = &tasks[at];
                if t.is_deleted || !tree.is_top_level(at) {
                    return false;
                }
                Some(t.project_id.as_str()) == current_project_id
                    && match filter {
                        ListFilter::Open => !t.checked,
                        ListFilter::Completed => t.checked || tree.completed_below(at),
                        ListFilter::All => true,
                        filter => self.narrowed_to(tree, at, filter),
                    }
            })
            .collect();

        match self.sort {
            Sort::Order => top_level.sort_by(|&a, &b| {
                let (a, b) = (&tasks[a], &tasks[b]);
                self.section_order(a)
                    .cmp(&self.section_order(b))
                    .then(a.child_order.cmp(&b.child_order))
            }),
            Sort::Priority => top_level.sort_by_key(|&at| std::cmp::Reverse(tasks[at].priority)),
            Sort::DueDate => top_level.sort_by(|&a, &b| {
                let due = |at: usize| {
                    tasks[at]
                        .due
                        .as_ref()
                        .map(|d| d.date.as_str())
                        .unwrap_or("9999")
                };
                due(a).cmp(due(b))
            }),
            Sort::Created => top_level.sort_by(|&a, &b| {
                let added = |at: usize| tasks[at].added_at.as_deref().unwrap_or("");
                added(b).cmp(added(a))
            }),
        }

        let by_section = self.sort == Sort::Order
            && (filter == ListFilter::Open || narrows)
            && top_level.iter().any(|&at| tasks[at].section_id.is_some());
        let mut rows = Vec::with_capacity(tasks.len());
        let mut index = 0;
        while let Some(&first) = top_level.get(index) {
            let section = &tasks[first].section_id;
            let group: Vec<usize> = if by_section {
                top_level[index..]
                    .iter()
                    .take_while(|&&at| &tasks[at].section_id == section)
                    .copied()
                    .collect()
            } else {
                top_level[index..].to_vec()
            };
            index += group.len();
            if by_section {
                rows.push(Row::Header(Group {
                    kind: GroupKind::Section(self.section_name(section.as_deref())),
                    count: group.len(),
                    folded: false,
                }));
            }
            for at in group {
                rows.push(Row::Task {
                    at: At::Store(at),
                    depth: 0,
                });
                if narrows || self.collapsed.contains(&tasks[at].id) {
                    continue;
                }
                let mut children = Vec::new();
                match filter {
                    ListFilter::Completed => self.collect_done_children(tree, at, &mut children),
                    ListFilter::All => {
                        self.collect_visible_children(tree, at, true, &mut children);
                    }
                    _ => self.collect_visible_children(tree, at, false, &mut children),
                }
                rows.extend(children.into_iter().map(|child| Row::Task {
                    at: At::Store(child),
                    depth: tree.depth(child),
                }));
            }
        }

        if matches!(filter, ListFilter::Completed | ListFilter::All)
            && let Some(project_id) = current_project_id
            && let Some(cached) = self.completed_cache.get(project_id)
        {
            self.append_cached_completed(tree, cached, &mut rows);
        }

        rows
    }

    /// Where `task`'s section sorts in its project; tasks in no section
    /// come first.
    fn section_order(&self, task: &Task) -> i32 {
        task.section_id
            .as_deref()
            .and_then(|id| self.store.sections.by_id(id))
            .and_then(|s| s.section_order)
            .unwrap_or(i32::MIN)
    }

    /// The name of section `id`, `None` for no section.
    fn section_name(&self, id: Option<&str>) -> Option<String> {
        let id = id?;
        Some(
            self.store
                .sections
                .by_id(id)
                .map_or_else(|| id.to_owned(), |s| s.name.clone()),
        )
    }

    /// The subtasks of `parent` that are completed or hold a completed
    /// task, depth first, skipping those under a folded task.
    fn collect_done_children(&self, tree: &Tree, parent: usize, result: &mut Vec<usize>) {
        for &child in tree.children(parent) {
            let task = &self.store.tasks[child];
            if task.is_deleted || !(task.checked || tree.completed_below(child)) {
                continue;
            }
            result.push(child);
            if !self.collapsed.contains(&task.id) {
                self.collect_done_children(tree, child, result);
            }
        }
    }

    /// Whether `task` is an open parent shown dimmed under the Completed
    /// list filter, for the completed subtasks under it.
    pub fn is_context_task(&self, task: &Task) -> bool {
        if !(self.list_filter == ListFilter::Completed && !task.checked) {
            return false;
        }
        let at = self.store.tasks.position(&task.id);
        self.with_lists(|lists| {
            at.is_some_and(|at| lists.tree.completed_below(at))
                || lists.completed_ancestors.contains(&task.id)
        })
    }

    /// The subtasks of `parent` under it, depth first, skipping those
    /// under a folded task, and completed ones unless `include_completed`.
    fn collect_visible_children(
        &self,
        tree: &Tree,
        parent: usize,
        include_completed: bool,
        result: &mut Vec<usize>,
    ) {
        for &child in tree.children(parent) {
            let task = &self.store.tasks[child];
            if task.is_deleted || !(include_completed || !task.checked) {
                continue;
            }
            result.push(child);
            if !self.collapsed.contains(&task.id) {
                self.collect_visible_children(tree, child, include_completed, result);
            }
        }
    }

    /// Appends the project's loaded completed tasks to `rows`, each one
    /// whose parent is not among them after that open parent as a dimmed
    /// context row, and its completed subtasks under it.
    fn append_cached_completed(&self, tree: &Tree, cached: &[Task], rows: &mut Vec<Row>) {
        if cached.is_empty() {
            return;
        }
        let cached_ids: HashSet<&str> = cached.iter().map(|t| t.id.as_str()).collect();
        let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut roots = Vec::new();
        for (index, task) in cached.iter().enumerate() {
            match task.parent_id.as_deref() {
                Some(parent) if cached_ids.contains(parent) => {
                    children.entry(parent).or_default().push(index);
                }
                _ => roots.push(index),
            }
        }
        roots.sort_by_key(|&index| cached[index].child_order);
        for list in children.values_mut() {
            list.sort_by_key(|&index| cached[index].child_order);
        }
        let depth = |task: &Task| match task.parent_id.as_deref() {
            None => 0,
            Some(parent) => self
                .store
                .tasks
                .position(parent)
                .map_or(1, |at| tree.depth(at) + 1),
        };
        let mut visited = HashSet::new();
        for root in roots {
            let task = &cached[root];
            if let Some(parent) = task.parent_id.as_deref()
                && let Some(at) = self.store.tasks.position(parent)
                && self.store.tasks.get(at).is_some_and(|t| !t.is_deleted)
            {
                rows.push(Row::Task {
                    at: At::Store(at),
                    depth: tree.depth(at),
                });
            }
            let mut stack = vec![root];
            while let Some(index) = stack.pop() {
                if !visited.insert(index) {
                    continue;
                }
                rows.push(Row::Task {
                    at: At::Completed(index),
                    depth: depth(&cached[index]),
                });
                if let Some(below) = children.get(cached[index].id.as_str()) {
                    stack.extend(below.iter().rev());
                }
            }
        }
    }
}

/// Every task id above a task of `cached` through tasks of `cached`: the
/// parent of each, and that parent's parent while it is in `cached` too.
fn ancestors_within(cached: &[Task]) -> HashSet<String> {
    let by_id: HashMap<&str, &Task> = cached.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut ancestors = HashSet::new();
    for task in cached {
        let mut current = task.parent_id.as_deref();
        while let Some(parent) = current {
            if !ancestors.insert(parent.to_owned()) {
                break;
            }
            current = by_id.get(parent).and_then(|t| t.parent_id.as_deref());
        }
    }
    ancestors
}

/// Whether `task` is open: neither completed nor deleted.
pub(crate) fn is_open(task: &Task) -> bool {
    !task.checked && !task.is_deleted
}

/// One row of a task list: a group header, or a task `depth` subtasks down
/// from its top-level task.
#[derive(Debug, Clone)]
pub enum ListRow<'a> {
    Header(Group),
    Task { task: &'a Task, depth: usize },
}

/// A group of a task list as its header shows it: what it groups, how many
/// tasks it holds and whether they are folded away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub kind: GroupKind,
    /// The tasks in the group, folded or not; in a project, its top-level
    /// tasks.
    pub count: usize,
    pub folded: bool,
}

/// What a group of a task list holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupKind {
    /// A section of a project, by its name; `None` for the tasks in no
    /// section.
    Section(Option<String>),
    Overdue,
    Today,
    /// One day of Upcoming.
    Day(Option<Date>),
    /// The tasks of one project, by its name.
    Project(String),
}

/// Adds the group of the store's tasks at `tasks` under its header, leaving
/// out a group with no tasks and the tasks of a folded one.
fn push_group(rows: &mut Vec<Row>, kind: GroupKind, folded: bool, tasks: Vec<usize>) {
    if tasks.is_empty() {
        return;
    }
    rows.push(Row::Header(Group {
        kind,
        count: tasks.len(),
        folded,
    }));
    if !folded {
        rows.extend(tasks.into_iter().map(|at| Row::Task {
            at: At::Store(at),
            depth: 0,
        }));
    }
}

/// Whether `filter` is a due bucket or a priority, which shows matching
/// top-level tasks without their subtasks.
fn narrows(filter: ListFilter) -> bool {
    !matches!(
        filter,
        ListFilter::Open | ListFilter::Completed | ListFilter::All
    )
}

#[cfg(test)]
mod tests {
    use jiff::{ToSpan, Zoned};
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use super::*;
    use crate::todoist::model::{Due, Label, Section};

    fn now() -> Zoned {
        jiff::civil::date(2026, 11, 24)
            .at(9, 0, 0, 0)
            .to_zoned(TimeZone::UTC)
            .unwrap()
    }

    /// One generated task: its parent by position, its project (the Inbox
    /// or `p`), completed or not, its order, its due day from today, its
    /// priority, and whether it is in section `s` and carries label `l`.
    #[derive(Debug, Clone)]
    struct Spec {
        parent: Option<usize>,
        inbox: bool,
        checked: bool,
        order: i32,
        due: Option<i64>,
        priority: u8,
        sectioned: bool,
        labelled: bool,
    }

    fn spec(parent: impl Strategy<Value = Option<usize>>) -> impl Strategy<Value = Spec> {
        (
            parent,
            any::<bool>(),
            prop::bool::weighted(0.3),
            0i32..4,
            prop::option::of(-3i64..9),
            1u8..=4,
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(
                |(parent, inbox, checked, order, due, priority, sectioned, labelled)| Spec {
                    parent,
                    inbox,
                    checked,
                    order,
                    due,
                    priority,
                    sectioned,
                    labelled,
                },
            )
    }

    /// Tasks whose parents come before them, so they never loop.
    fn forest() -> impl Strategy<Value = Vec<Spec>> {
        (0usize..24).prop_flat_map(|n| {
            (0..n)
                .map(|at| match at {
                    0 => spec(Just(None)).boxed(),
                    _ => spec(prop::option::weighted(0.6, 0..at)).boxed(),
                })
                .collect::<Vec<_>>()
        })
    }

    /// Tasks whose parents can be any task, themselves included.
    fn tangle() -> impl Strategy<Value = Vec<Spec>> {
        (1usize..16).prop_flat_map(|n| prop::collection::vec(spec(prop::option::of(0..n)), n))
    }

    fn day(offset: i64) -> String {
        let date = now().date().checked_add(offset.days()).unwrap();
        date.strftime("%Y-%m-%d").to_string()
    }

    fn task(at: usize, spec: &Spec) -> Task {
        Task {
            id: format!("t{at}"),
            content: format!("task {at}"),
            checked: spec.checked,
            child_order: spec.order,
            priority: spec.priority,
            project_id: if spec.inbox { "inbox" } else { "p" }.to_owned(),
            section_id: spec.sectioned.then(|| "s".to_owned()),
            parent_id: spec.parent.map(|parent| format!("t{parent}")),
            labels: if spec.labelled {
                vec!["l".to_owned()]
            } else {
                Vec::new()
            },
            due: spec.due.map(|offset| Due {
                date: day(offset),
                ..Due::default()
            }),
            ..Task::default()
        }
    }

    fn state(specs: &[Spec]) -> State {
        let mut state = State::new(now());
        state.loading = false;
        state.store.projects.replace_all([
            Project {
                id: "inbox".to_owned(),
                name: "Inbox".to_owned(),
                inbox_project: Some(true),
                ..Project::default()
            },
            Project {
                id: "p".to_owned(),
                name: "P".to_owned(),
                ..Project::default()
            },
        ]);
        state.store.sections.replace_all([Section {
            id: "s".to_owned(),
            project_id: "p".to_owned(),
            name: "S".to_owned(),
            section_order: Some(1),
            ..Section::default()
        }]);
        state.store.labels.replace_all([Label {
            id: "l".to_owned(),
            name: "l".to_owned(),
            ..Label::default()
        }]);
        state
            .store
            .tasks
            .replace_all(specs.iter().enumerate().map(|(at, spec)| task(at, spec)));
        state
    }

    /// Everything the lists answer, as one comparable string.
    fn answers(state: &State) -> String {
        let rows: Vec<String> = state
            .list_rows()
            .iter()
            .map(|row| match row {
                ListRow::Header(group) => format!("{group:?}"),
                ListRow::Task { task, depth } => format!("{}@{depth}", task.id),
            })
            .collect();
        let contexts: Vec<bool> = state
            .visible_tasks()
            .iter()
            .map(|task| state.is_context_task(task))
            .collect();
        let children: Vec<bool> = state
            .store
            .tasks
            .iter()
            .map(|task| state.has_children(&task.id))
            .collect();
        let stats = state.overview_stats();
        format!(
            "{rows:?} {:?} {} {} {contexts:?} {children:?} {:?} {} {} {} {} {} {:?}",
            state.selected_task().map(|task| &task.id),
            state.task_count_with(state.list_filter),
            state.task_count_with(ListFilter::default()),
            state.label_rows(),
            state.open_task_count("inbox"),
            state.open_task_count("p"),
            stats.overdue,
            stats.due_today,
            stats.due_week,
            stats.by_priority,
        )
    }

    /// The same answers from lists built from scratch.
    fn answers_from_scratch(state: &State) -> String {
        let fresh = state.clone();
        *fresh.lists.borrow_mut() = None;
        answers(&fresh)
    }

    /// A change to the store or to the list state.
    #[derive(Debug, Clone)]
    enum Change {
        Complete(usize),
        Fold(usize),
        Remove(usize),
        Add(Spec),
        Filter(usize),
        Sort(usize),
        Screen(usize),
        Browse(bool),
        Label(bool),
        Project(usize),
        FoldOverdue,
        LoadCompleted(Vec<Option<usize>>),
        Focus(usize),
        NextDay,
    }

    fn change() -> impl Strategy<Value = Change> {
        prop_oneof![
            (0usize..24).prop_map(Change::Complete),
            (0usize..24).prop_map(Change::Fold),
            (0usize..24).prop_map(Change::Remove),
            spec(prop::option::of(0usize..24)).prop_map(Change::Add),
            (0..ListFilter::ALL.len()).prop_map(Change::Filter),
            (0..Sort::ALL.len()).prop_map(Change::Sort),
            (0..Screen::ALL.len()).prop_map(Change::Screen),
            any::<bool>().prop_map(Change::Browse),
            any::<bool>().prop_map(Change::Label),
            (0usize..3).prop_map(Change::Project),
            Just(Change::FoldOverdue),
            prop::collection::vec(prop::option::of(0usize..30), 0..6)
                .prop_map(Change::LoadCompleted),
            (0usize..30).prop_map(Change::Focus),
            Just(Change::NextDay),
        ]
    }

    fn apply(state: &mut State, change: Change) {
        let id = |at: usize| format!("t{at}");
        match change {
            Change::Complete(at) => {
                if let Some(task) = state.store.tasks.by_id_mut(&id(at)) {
                    task.checked = !task.checked;
                }
            }
            Change::Fold(at) => {
                if !state.collapsed.remove(&id(at)) {
                    state.collapsed.insert(id(at));
                }
            }
            Change::Remove(at) => {
                state.store.tasks.remove(&id(at));
            }
            Change::Add(spec) => {
                let at = state.store.tasks.len() + 100;
                state.store.tasks.upsert(task(at, &spec));
            }
            Change::Filter(at) => state.list_filter = ListFilter::ALL[at],
            Change::Sort(at) => state.sort = Sort::ALL[at],
            Change::Screen(at) => state.screen = Screen::ALL[at],
            Change::Browse(browse) => {
                state.active_pane = if browse { Pane::Browse } else { Pane::Tasks };
            }
            Change::Label(open) => state.opened_label = open.then(|| "l".to_owned()),
            Change::Project(at) => state.selected_project = at,
            Change::FoldOverdue => {
                state.overdue_section_collapsed = !state.overdue_section_collapsed;
            }
            Change::LoadCompleted(parents) => {
                let cached = parents
                    .iter()
                    .enumerate()
                    .map(|(at, parent)| Task {
                        id: format!("c{at}"),
                        checked: true,
                        child_order: i32::try_from(at % 3).unwrap_or(0),
                        project_id: "inbox".to_owned(),
                        parent_id: parent.map(|parent| {
                            if parent % 2 == 0 {
                                format!("c{}", parent / 6)
                            } else {
                                id(parent)
                            }
                        }),
                        ..Task::default()
                    })
                    .collect();
                state.completed_cache.insert("inbox".to_owned(), cached);
            }
            Change::Focus(at) => state.selected_task = at,
            Change::NextDay => state.today = state.today.tomorrow().unwrap(),
        }
    }

    /// How many parents up `at`'s top-level task is, walking ids.
    fn naive_depth(tasks: &[Task], at: usize) -> usize {
        let mut depth = 0;
        let mut parent = tasks[at].parent_id.as_deref();
        while let Some(id) = parent {
            depth += 1;
            parent = tasks
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| t.parent_id.as_deref());
        }
        depth
    }

    fn naive_is_below(tasks: &[Task], at: usize, ancestor: &str) -> bool {
        let mut parent = tasks[at].parent_id.as_deref();
        while let Some(id) = parent {
            if id == ancestor {
                return true;
            }
            parent = tasks
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| t.parent_id.as_deref());
        }
        false
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        })]

        #[test]
        fn the_tree_matches_walking_every_parent(specs in forest()) {
            let state = state(&specs);
            let tasks = &state.store.tasks;
            let tree = Tree::of(tasks);

            for (at, task) in tasks.iter().enumerate() {
                prop_assert_eq!(tree.depth(at), naive_depth(tasks, at));
                prop_assert_eq!(tree.is_top_level(at), task.parent_id.is_none());
                let below = tasks.iter().enumerate().any(|(other, t)| {
                    t.checked && !t.is_deleted && naive_is_below(tasks, other, &task.id)
                });
                prop_assert_eq!(tree.completed_below(at), below);
                let mut children: Vec<usize> = (0..tasks.len())
                    .filter(|&other| tasks[other].parent_id.as_deref() == Some(task.id.as_str()))
                    .collect();
                children.sort_by_key(|&other| tasks[other].child_order);
                prop_assert_eq!(tree.children(at), children.as_slice());
            }
        }

        #[test]
        fn tasks_whose_parents_loop_show_once_at_top_level(specs in tangle()) {
            let mut state = state(&specs);
            let tasks = &state.store.tasks;
            let tree = Tree::of(tasks);
            for (at, task) in tasks.iter().enumerate() {
                let mut seen = HashSet::new();
                let mut parent = task.parent_id.as_deref();
                let mut on_loop = false;
                while let Some(id) = parent {
                    if id == task.id {
                        on_loop = true;
                        break;
                    }
                    if !seen.insert(id) {
                        break;
                    }
                    parent = tasks.by_id(id).and_then(|t| t.parent_id.as_deref());
                }
                if on_loop {
                    prop_assert!(tree.is_top_level(at));
                }
            }

            state.screen = Screen::Projects;
            for project in 0..2 {
                state.selected_project = project;
                state.list_filter = ListFilter::All;
                let shown: Vec<String> = state.visible_tasks().iter().map(|t| t.id.clone()).collect();
                let unique: HashSet<&String> = shown.iter().collect();
                prop_assert_eq!(unique.len(), shown.len());
            }
        }

        #[test]
        fn rebuilt_lists_equal_lists_built_from_scratch(
            specs in forest(),
            changes in prop::collection::vec(change(), 0..24),
        ) {
            let mut state = state(&specs);
            prop_assert_eq!(answers(&state), answers_from_scratch(&state));
            for change in changes {
                apply(&mut state, change);
                prop_assert_eq!(answers(&state), answers_from_scratch(&state));
            }
        }
    }
}

//! What the screen derives from [`State`]: the rows of the task lists, the
//! project tree and the stats.

use std::collections::HashSet;

use chrono::NaiveDate;

use super::state::{COUNTED_FILTERS, ListFilter, OverviewStats, Pane, Screen, Sort, State};
use crate::todoist::model::{Priority, Project, Task};
use crate::ui::dates::{self, DueBucket};

impl State {
    /// Where `task`'s due date falls today.
    pub fn due_bucket(&self, task: &Task) -> DueBucket {
        dates::due_bucket(task.due.as_ref(), self.today)
    }

    /// Unchecked, not deleted, and unassigned or assigned to the user: the
    /// tasks every count and cross-project list is made of.
    pub fn is_open_and_mine(&self, task: &Task) -> bool {
        !task.checked
            && !task.is_deleted
            && task
                .responsible_uid
                .as_deref()
                .is_none_or(|uid| self.current_user_id.as_deref() == Some(uid))
    }

    /// Whether `task` is a row of a list narrowed to `filter`, a due bucket
    /// or a priority. The stats count these tasks across every project,
    /// while a list applies the filter within its own project, so a count
    /// can exceed the rows a filtered list shows. Subtasks are left out of
    /// both, as the Today list leaves them out. Open, completed and all
    /// narrow nothing here.
    fn narrowed_to(&self, task: &Task, filter: ListFilter) -> bool {
        if task.parent_id.is_some() || !self.is_open_and_mine(task) {
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

    pub fn project_depth(&self, project_id: &str) -> usize {
        let mut depth = 0;
        let mut current = project_id;
        while let Some(parent_id) = self
            .projects
            .iter()
            .find(|p| p.id == current)
            .and_then(|p| p.parent_id.as_deref())
        {
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
            Screen::Inbox => self.projects.iter().find(|p| p.is_inbox()),
            Screen::Projects => self.projects.get(self.selected_project),
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
        self.projects
            .get(self.selected_project)
            .map(|p| p.name.as_str())
            .unwrap_or("Tasks")
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let visible = self.visible_tasks();
        visible.get(self.selected_task).copied()
    }

    pub fn overview_stats(&self) -> OverviewStats {
        let mut stats = OverviewStats {
            due_today: 0,
            due_week: 0,
            overdue: 0,
            by_priority: [0; 5],
        };
        for filter in COUNTED_FILTERS {
            let count = self
                .tasks
                .iter()
                .filter(|t| self.narrowed_to(t, filter))
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
        stats
    }

    pub fn has_children(&self, task_id: &str) -> bool {
        self.tasks
            .iter()
            .any(|t| t.parent_id.as_deref() == Some(task_id))
    }

    pub fn is_collapsed(&self, task_id: &str) -> bool {
        self.collapsed.contains(task_id)
    }

    /// The tasks of the list on screen, in order: the task rows of
    /// [`State::list_rows`]. Empty while a tree has the focus.
    pub fn visible_tasks(&self) -> Vec<&Task> {
        self.list_rows()
            .into_iter()
            .filter_map(|row| match row {
                ListRow::Task { task, .. } => Some(task),
                ListRow::Header(_) => None,
            })
            .collect()
    }

    /// The rows of the task list on the current screen, group headers and
    /// tasks, in order. Empty while a tree has the focus.
    pub fn list_rows(&self) -> Vec<ListRow<'_>> {
        self.rows_with(self.list_filter)
    }

    /// How many tasks the list on screen shows with `filter`: the `of 42`
    /// of the list filter header.
    pub fn task_count_with(&self, filter: ListFilter) -> usize {
        self.rows_with(filter)
            .iter()
            .filter(|row| matches!(row, ListRow::Task { .. }))
            .count()
    }

    fn rows_with(&self, filter: ListFilter) -> Vec<ListRow<'_>> {
        if self.active_pane == Pane::Browse {
            return Vec::new();
        }
        match self.screen {
            Screen::Today => self.today_rows(filter),
            Screen::Upcoming => self.upcoming_rows(filter),
            Screen::Labels => self.label_list_rows(filter),
            Screen::Inbox | Screen::Projects => self.project_rows(filter),
        }
    }

    /// Open tasks due today or in the six days after, by day, then by
    /// their order in the project. Subtasks are left out, as on Today.
    pub fn upcoming_tasks(&self) -> Vec<&Task> {
        self.upcoming_with(self.list_filter)
    }

    fn upcoming_with(&self, filter: ListFilter) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                self.narrowed_to(t, ListFilter::DueThisWeek)
                    && self.in_cross_project_list(t, filter)
            })
            .collect();
        tasks.sort_by(|a, b| {
            let day = |t: &Task| t.due.as_ref().map(|d| d.date.clone()).unwrap_or_default();
            day(a).cmp(&day(b)).then(a.child_order.cmp(&b.child_order))
        });
        tasks
    }

    /// The Upcoming tasks under a header for each day.
    fn upcoming_rows(&self, filter: ListFilter) -> Vec<ListRow<'_>> {
        let tasks = self.upcoming_with(filter);
        let mut rows = Vec::with_capacity(tasks.len() + 7);
        let mut index = 0;
        while let Some(first) = tasks.get(index) {
            let day = Self::due_day(first);
            let group: Vec<&Task> = tasks[index..]
                .iter()
                .take_while(|t| Self::due_day(t) == day)
                .copied()
                .collect();
            index += group.len();
            push_group(&mut rows, GroupKind::Day(day), false, group);
        }
        rows
    }

    /// The opened label's tasks under a header for each project.
    fn label_list_rows(&self, filter: ListFilter) -> Vec<ListRow<'_>> {
        let mut rows = Vec::new();
        for (project, tasks) in self.label_tasks(filter) {
            push_group(
                &mut rows,
                GroupKind::Project(project.name.clone()),
                false,
                tasks,
            );
        }
        rows
    }

    /// Whether `filter` keeps `task` in a list across projects (Today,
    /// Upcoming), which holds open tasks only: completed shows none of
    /// them, a due bucket or priority narrows them.
    pub(crate) fn in_cross_project_list(&self, task: &Task, filter: ListFilter) -> bool {
        filter != ListFilter::Completed && self.narrowed_to(task, filter)
    }

    /// The day `task` is due, when it has a due date that parses.
    pub fn due_day(task: &Task) -> Option<NaiveDate> {
        let due = task.due.as_ref()?;
        NaiveDate::parse_from_str(dates::date_part(&due.date), "%Y-%m-%d").ok()
    }

    /// Today: the Overdue group, which folds, then the tasks due today.
    /// A group with no tasks is left out; a folded one keeps its header.
    fn today_rows(&self, filter: ListFilter) -> Vec<ListRow<'_>> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                (self.narrowed_to(t, ListFilter::Overdue)
                    || self.narrowed_to(t, ListFilter::DueToday))
                    && self.in_cross_project_list(t, filter)
            })
            .collect();
        tasks.sort_by(|a, b| {
            let a_date = a
                .due
                .as_ref()
                .map(|d| dates::date_part(&d.date))
                .unwrap_or("");
            let b_date = b
                .due
                .as_ref()
                .map(|d| dates::date_part(&d.date))
                .unwrap_or("");
            a_date.cmp(b_date).then(a.child_order.cmp(&b.child_order))
        });
        let (overdue, today): (Vec<&Task>, Vec<&Task>) = tasks
            .into_iter()
            .partition(|t| self.due_bucket(t) == DueBucket::Overdue);
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
    fn project_rows(&self, filter: ListFilter) -> Vec<ListRow<'_>> {
        let current_project_id = self.list_project().map(|p| p.id.as_str());
        let narrows = narrows(filter);

        let mut top_level: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                if t.is_deleted || t.parent_id.is_some() {
                    return false;
                }
                Some(t.project_id.as_str()) == current_project_id
                    && match filter {
                        ListFilter::Open => !t.checked,
                        ListFilter::Completed => t.checked || self.has_completed_descendant(&t.id),
                        ListFilter::All => true,
                        filter => self.narrowed_to(t, filter),
                    }
            })
            .collect();

        match self.sort {
            Sort::Order => {
                top_level.sort_by(|a, b| {
                    self.section_order(a)
                        .cmp(&self.section_order(b))
                        .then(a.child_order.cmp(&b.child_order))
                });
            }
            Sort::Priority => top_level.sort_by_key(|b| std::cmp::Reverse(b.priority)),
            Sort::DueDate => top_level.sort_by(|a, b| {
                let a_due = a.due.as_ref().map(|d| d.date.as_str()).unwrap_or("9999");
                let b_due = b.due.as_ref().map(|d| d.date.as_str()).unwrap_or("9999");
                a_due.cmp(b_due)
            }),
            Sort::Created => top_level.sort_by(|a, b| {
                let a_at = a.added_at.as_deref().unwrap_or("");
                let b_at = b.added_at.as_deref().unwrap_or("");
                b_at.cmp(a_at)
            }),
        }

        let by_section = self.sort == Sort::Order
            && (filter == ListFilter::Open || narrows)
            && top_level.iter().any(|t| t.section_id.is_some());
        let mut rows = Vec::with_capacity(self.tasks.len());
        let mut index = 0;
        while let Some(first) = top_level.get(index) {
            let group: Vec<&Task> = if by_section {
                top_level[index..]
                    .iter()
                    .take_while(|t| t.section_id == first.section_id)
                    .copied()
                    .collect()
            } else {
                top_level[index..].to_vec()
            };
            index += group.len();
            if by_section {
                rows.push(ListRow::Header(Group {
                    kind: GroupKind::Section(self.section_name(first.section_id.as_deref())),
                    count: group.len(),
                    folded: false,
                }));
            }
            for task in group {
                rows.push(ListRow::Task { task, depth: 0 });
                if narrows || self.collapsed.contains(&task.id) {
                    continue;
                }
                let mut children = Vec::new();
                match filter {
                    ListFilter::Completed => self.collect_done_children(&task.id, &mut children),
                    ListFilter::All => self.collect_visible_children(&task.id, true, &mut children),
                    _ => self.collect_visible_children(&task.id, false, &mut children),
                }
                rows.extend(children.into_iter().map(|task| ListRow::Task {
                    task,
                    depth: self.task_depth(task),
                }));
            }
        }

        if matches!(filter, ListFilter::Completed | ListFilter::All)
            && let Some(pid) = self.list_project().map(|p| p.id.clone())
        {
            let mut cached = Vec::new();
            self.append_cached_completed(&pid, &mut cached);
            rows.extend(cached.into_iter().map(|task| ListRow::Task {
                task,
                depth: self.task_depth(task),
            }));
        }

        rows
    }

    /// Where `task`'s section sorts in its project; tasks in no section
    /// come first.
    fn section_order(&self, task: &Task) -> i32 {
        task.section_id
            .as_deref()
            .and_then(|id| self.sections.iter().find(|s| s.id == id))
            .and_then(|s| s.section_order)
            .unwrap_or(i32::MIN)
    }

    /// The name of section `id`, `None` for no section.
    fn section_name(&self, id: Option<&str>) -> Option<String> {
        let id = id?;
        Some(
            self.sections
                .iter()
                .find(|s| s.id == id)
                .map_or_else(|| id.to_owned(), |s| s.name.clone()),
        )
    }

    fn collect_done_children<'a>(&'a self, parent_id: &str, result: &mut Vec<&'a Task>) {
        let mut children: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                !t.is_deleted
                    && t.parent_id.as_deref() == Some(parent_id)
                    && (t.checked || self.has_completed_descendant(&t.id))
            })
            .collect();
        children.sort_by_key(|t| t.child_order);
        for child in children {
            result.push(child);
            if !self.collapsed.contains(&child.id) {
                self.collect_done_children(&child.id, result);
            }
        }
    }

    fn has_completed_descendant(&self, task_id: &str) -> bool {
        self.tasks
            .iter()
            .any(|t| !t.is_deleted && t.checked && self.is_descendant_of(&t.id, task_id))
    }

    fn is_descendant_of(&self, task_id: &str, ancestor_id: &str) -> bool {
        let mut current = task_id.to_string();
        loop {
            let parent = self
                .tasks
                .iter()
                .find(|t| t.id == current)
                .and_then(|t| t.parent_id.clone());
            match parent {
                None => return false,
                Some(pid) if pid == ancestor_id => return true,
                Some(pid) => current = pid,
            }
        }
    }

    pub fn is_context_task(&self, task: &Task) -> bool {
        if !(self.list_filter == ListFilter::Completed && !task.checked) {
            return false;
        }
        if self.has_completed_descendant(&task.id) {
            return true;
        }
        if let Some(pid) = self.list_project().map(|p| p.id.as_str())
            && let Some(cached) = self.completed_cache.get(pid)
        {
            return cached
                .iter()
                .any(|t| self.is_cached_descendant_of(t, &task.id, cached));
        }
        false
    }

    /// The subtasks of `parent_id` under it, depth first, skipping those
    /// under a folded task, and completed ones unless `include_completed`.
    fn collect_visible_children<'a>(
        &'a self,
        parent_id: &str,
        include_completed: bool,
        result: &mut Vec<&'a Task>,
    ) {
        let mut children: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                !t.is_deleted
                    && t.parent_id.as_deref() == Some(parent_id)
                    && (include_completed || !t.checked)
            })
            .collect();
        children.sort_by_key(|t| t.child_order);

        for child in children {
            result.push(child);
            if !self.collapsed.contains(&child.id) {
                self.collect_visible_children(&child.id, include_completed, result);
            }
        }
    }

    pub fn task_depth(&self, task: &Task) -> usize {
        let mut depth = 0;
        let mut current_parent = task.parent_id.as_deref();
        while let Some(pid) = current_parent {
            depth += 1;
            current_parent = self
                .tasks
                .iter()
                .find(|t| t.id == pid)
                .and_then(|t| t.parent_id.as_deref());
        }
        depth
    }

    /// Appends cached completed tasks for `project_id` into `result`, inserting active parent
    /// tasks as dimmed context rows where needed. Works for both Done and Both filters:
    /// in Both mode, active parents are already in `result` so they're skipped via `already_shown`.
    fn append_cached_completed<'a>(&'a self, project_id: &str, result: &mut Vec<&'a Task>) {
        let cached = match self.completed_cache.get(project_id) {
            Some(c) if !c.is_empty() => c,
            _ => return,
        };

        let already_shown: HashSet<&str> = result.iter().map(|t| t.id.as_str()).collect();
        let cached_ids: HashSet<&str> = cached.iter().map(|t| t.id.as_str()).collect();

        // Roots: cached tasks whose parent is absent from the cached set.
        let mut roots: Vec<&Task> = cached
            .iter()
            .filter(|t| {
                t.parent_id
                    .as_ref()
                    .is_none_or(|pid| !cached_ids.contains(pid.as_str()))
            })
            .collect();
        roots.sort_by_key(|t| t.child_order);

        for root in roots {
            // If this cached root has an active parent not yet shown, add it as a context row.
            if let Some(ref pid) = root.parent_id
                && !already_shown.contains(pid.as_str())
                && let Some(parent) = self.tasks.iter().find(|t| t.id == *pid && !t.is_deleted)
            {
                result.push(parent);
            }
            result.push(root);
            Self::collect_cached_children(&root.id, cached, &mut *result);
        }
    }

    fn collect_cached_children<'a>(
        parent_id: &str,
        cached: &'a [Task],
        result: &mut Vec<&'a Task>,
    ) {
        let mut children: Vec<&Task> = cached
            .iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent_id))
            .collect();
        children.sort_by_key(|t| t.child_order);
        for child in children {
            result.push(child);
            Self::collect_cached_children(&child.id, cached, result);
        }
    }

    /// Returns true if `task` is a descendant of `ancestor_id` within `cached`.
    fn is_cached_descendant_of(&self, task: &Task, ancestor_id: &str, cached: &[Task]) -> bool {
        let mut current_parent = task.parent_id.as_deref();
        while let Some(pid) = current_parent {
            if pid == ancestor_id {
                return true;
            }
            current_parent = cached
                .iter()
                .find(|t| t.id == pid)
                .and_then(|t| t.parent_id.as_deref());
        }
        false
    }
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
    Day(Option<NaiveDate>),
    /// The tasks of one project, by its name.
    Project(String),
}

/// Adds the group of `tasks` under its header, leaving out a group with no
/// tasks and the tasks of a folded one.
fn push_group<'a>(
    rows: &mut Vec<ListRow<'a>>,
    kind: GroupKind,
    folded: bool,
    tasks: Vec<&'a Task>,
) {
    if tasks.is_empty() {
        return;
    }
    rows.push(ListRow::Header(Group {
        kind,
        count: tasks.len(),
        folded,
    }));
    if !folded {
        rows.extend(
            tasks
                .into_iter()
                .map(|task| ListRow::Task { task, depth: 0 }),
        );
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

//! What the screen derives from [`State`]: the visible tasks, the project
//! tree and the stats.

use std::collections::HashSet;

use super::state::{
    COUNTED_FILTERS, ListFilter, OverviewStats, ProjectEntry, ProjectNavItem, Screen, Sort, State,
};
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
    /// or a priority. The stats count the same tasks, so a count always
    /// equals the rows it opens. Subtasks are left out of both, as the Today
    /// list leaves them out. Open, completed and all narrow nothing here.
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

    /// Whether the list filter is a due bucket or a priority, which shows
    /// matching top-level tasks without their subtasks.
    fn narrows(&self) -> bool {
        !matches!(
            self.list_filter,
            ListFilter::Open | ListFilter::Completed | ListFilter::All
        )
    }

    pub fn project_list_entries(&self) -> Vec<ProjectEntry> {
        let mut entries = Vec::new();
        let mut in_personal = false;
        let mut last_ws_id: Option<&str> = None;
        let mut last_folder_id: Option<&str> = None;

        for (i, p) in self.projects.iter().enumerate() {
            let ws_id = p.workspace_id.as_deref();
            let folder_id = p.folder_id.as_deref();

            let folder_collapsed = folder_id
                .map(|fid| self.collapsed_folders.contains(fid))
                .unwrap_or(false);

            if ws_id.is_none() {
                if !in_personal {
                    in_personal = true;
                    entries.push(ProjectEntry::PersonalHeader);
                }
            } else {
                if last_ws_id != ws_id {
                    last_ws_id = ws_id;
                    last_folder_id = None;
                    entries.push(ProjectEntry::Separator);
                    if let Some(wi) = self
                        .workspaces
                        .iter()
                        .position(|w| Some(w.id.as_str()) == ws_id)
                    {
                        entries.push(ProjectEntry::WorkspaceHeader(wi));
                    }
                }
                if last_folder_id != folder_id {
                    last_folder_id = folder_id;
                    if let Some(fid) = folder_id
                        && let Some(fi) = self.folders.iter().position(|f| f.id.as_str() == fid)
                    {
                        entries.push(ProjectEntry::FolderHeader(fi));
                    }
                }
            }

            if !folder_collapsed {
                entries.push(ProjectEntry::Project(i));
            }
        }

        entries
    }

    pub fn project_indent(&self, project: &Project) -> usize {
        let base = if project.folder_id.is_some() { 3 } else { 1 };
        base + self.project_depth(&project.id)
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

    pub fn visible_nav_items(&self) -> Vec<ProjectNavItem> {
        self.project_list_entries()
            .into_iter()
            .filter_map(|e| match e {
                ProjectEntry::FolderHeader(fi) => Some(ProjectNavItem::Folder(fi)),
                ProjectEntry::Project(i) => Some(ProjectNavItem::Project(i)),
                _ => None,
            })
            .collect()
    }
    /// The project whose tasks the list shows: the Inbox on the Inbox
    /// screen and the focused project on the Projects screen. The other
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
    pub fn list_title(&self) -> &str {
        match self.list_project() {
            Some(project) => &project.name,
            None => self.screen.title(),
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

    /// The rows of the task list on the current screen, in order.
    pub fn visible_tasks(&self) -> Vec<&Task> {
        match self.screen {
            Screen::Today => self.today_tasks(),
            Screen::Upcoming => self.upcoming_tasks(),
            Screen::Labels => Vec::new(),
            Screen::Inbox | Screen::Projects => self.project_tasks(),
        }
    }

    /// Open tasks due today or in the six days after, by day, then by
    /// their order in the project. Subtasks are left out, as on Today.
    pub fn upcoming_tasks(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                self.narrowed_to(t, ListFilter::DueThisWeek) && self.in_cross_project_list(t)
            })
            .collect();
        tasks.sort_by(|a, b| {
            let day = |t: &Task| t.due.as_ref().map(|d| d.date.clone()).unwrap_or_default();
            day(a).cmp(&day(b)).then(a.child_order.cmp(&b.child_order))
        });
        tasks
    }

    /// Whether the list filter keeps `task` in a list across projects
    /// (Today, Upcoming), which holds open tasks only: completed shows none
    /// of them, a due bucket or priority narrows them.
    fn in_cross_project_list(&self, task: &Task) -> bool {
        self.list_filter != ListFilter::Completed && self.narrowed_to(task, self.list_filter)
    }

    /// The day `task` is due, when it has a due date that parses.
    pub fn due_day(task: &Task) -> Option<chrono::NaiveDate> {
        let due = task.due.as_ref()?;
        chrono::NaiveDate::parse_from_str(dates::date_part(&due.date), "%Y-%m-%d").ok()
    }

    fn today_tasks(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                (self.narrowed_to(t, ListFilter::Overdue)
                    || self.narrowed_to(t, ListFilter::DueToday))
                    && self.in_cross_project_list(t)
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
        if self.overdue_section_collapsed {
            tasks.retain(|t| self.due_bucket(t) == DueBucket::Today);
        }
        tasks
    }

    fn project_tasks(&self) -> Vec<&Task> {
        let current_project_id = self.list_project().map(|p| p.id.as_str());

        let mut top_level: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                if t.is_deleted || t.parent_id.is_some() {
                    return false;
                }
                Some(t.project_id.as_str()) == current_project_id
                    && match self.list_filter {
                        ListFilter::Open => !t.checked,
                        ListFilter::Completed => t.checked || self.has_completed_descendant(&t.id),
                        ListFilter::All => true,
                        filter => self.narrowed_to(t, filter),
                    }
            })
            .collect();

        match self.sort {
            Sort::Order => {
                let so = |sid: Option<&str>| {
                    sid.and_then(|id| self.sections.iter().find(|s| s.id == id))
                        .and_then(|s| s.section_order)
                        .unwrap_or(i32::MIN)
                };
                top_level.sort_by(|a, b| {
                    so(a.section_id.as_deref())
                        .cmp(&so(b.section_id.as_deref()))
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

        if self.narrows() {
            return top_level;
        }

        let mut result = Vec::with_capacity(self.tasks.len());
        for task in top_level {
            result.push(task);
            if !self.collapsed.contains(&task.id) {
                if self.list_filter == ListFilter::Completed {
                    self.collect_done_children(&task.id, &mut result);
                } else {
                    self.collect_visible_children(&task.id, &mut result);
                }
            }
        }

        if matches!(self.list_filter, ListFilter::Completed | ListFilter::All)
            && let Some(pid) = self.list_project().map(|p| p.id.clone())
        {
            self.append_cached_completed(&pid, &mut result);
        }

        result
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

    fn collect_visible_children<'a>(&'a self, parent_id: &str, result: &mut Vec<&'a Task>) {
        let mut children: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| !t.is_deleted && t.parent_id.as_deref() == Some(parent_id))
            .collect();
        children.sort_by_key(|t| t.child_order);

        for child in children {
            result.push(child);
            if !self.collapsed.contains(&child.id) {
                self.collect_visible_children(&child.id, result);
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

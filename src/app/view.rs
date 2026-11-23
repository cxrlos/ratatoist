//! What the screen derives from [`State`]: the visible tasks, the project
//! tree and the stats.

use std::collections::HashSet;

use super::state::{
    DOCK_ITEMS, DockItem, OverviewStats, ProjectEntry, ProjectNavItem, SortMode, State, TaskFilter,
};
use crate::todoist::model::{Project, Task};
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

    /// Whether `task` is a row of `item`'s dock filter. The stats count the
    /// same tasks, so a count always equals the rows it opens. Subtasks are
    /// left out of both, as the Today list leaves them out.
    fn in_dock_item(&self, task: &Task, item: DockItem) -> bool {
        if task.parent_id.is_some() || !self.is_open_and_mine(task) {
            return false;
        }
        match item {
            DockItem::DueOverdue => self.due_bucket(task) == DueBucket::Overdue,
            DockItem::DueToday => self.due_bucket(task) == DueBucket::Today,
            DockItem::DueWeek => matches!(
                self.due_bucket(task),
                DueBucket::Today | DueBucket::ThisWeek
            ),
            DockItem::Priority(p) => task.priority == p,
        }
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
                let is_inbox = self.projects[i].is_inbox();
                entries.push(ProjectEntry::Project(i));
                if is_inbox {
                    entries.push(ProjectEntry::TodayView);
                }
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
                ProjectEntry::TodayView => Some(ProjectNavItem::TodayView),
                _ => None,
            })
            .collect()
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
        for item in DOCK_ITEMS {
            let count = self
                .tasks
                .iter()
                .filter(|t| self.in_dock_item(t, item))
                .count();
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            match item {
                DockItem::DueOverdue => stats.overdue = count,
                DockItem::DueToday => stats.due_today = count,
                DockItem::DueWeek => stats.due_week = count,
                DockItem::Priority(p) => {
                    if let Some(slot) = stats.by_priority.get_mut(usize::from(p)) {
                        *slot = count;
                    }
                }
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

    pub fn visible_tasks(&self) -> Vec<&Task> {
        if self.today_view_active {
            let mut tasks: Vec<&Task> = self
                .tasks
                .iter()
                .filter(|t| {
                    self.in_dock_item(t, DockItem::DueOverdue)
                        || self.in_dock_item(t, DockItem::DueToday)
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
            return tasks;
        }

        let current_project_id = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.as_str());

        let mut top_level: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| {
                if t.is_deleted || t.parent_id.is_some() {
                    return false;
                }
                if let Some(dock) = self.dock_filter {
                    return self.in_dock_item(t, dock);
                }
                Some(t.project_id.as_str()) == current_project_id
                    && match self.task_filter {
                        TaskFilter::Active => !t.checked,
                        TaskFilter::Done => t.checked || self.has_completed_descendant(&t.id),
                        TaskFilter::Both => true,
                    }
            })
            .collect();

        match self.sort_mode {
            SortMode::Default => {
                if self.dock_filter.is_none() {
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
                } else {
                    top_level.sort_by_key(|t| t.child_order);
                }
            }
            SortMode::Priority => top_level.sort_by_key(|b| std::cmp::Reverse(b.priority)),
            SortMode::DueDate => top_level.sort_by(|a, b| {
                let a_due = a.due.as_ref().map(|d| d.date.as_str()).unwrap_or("9999");
                let b_due = b.due.as_ref().map(|d| d.date.as_str()).unwrap_or("9999");
                a_due.cmp(b_due)
            }),
            SortMode::Created => top_level.sort_by(|a, b| {
                let a_at = a.added_at.as_deref().unwrap_or("");
                let b_at = b.added_at.as_deref().unwrap_or("");
                b_at.cmp(a_at)
            }),
        }

        if self.dock_filter.is_some() {
            return top_level;
        }

        let mut result = Vec::with_capacity(self.tasks.len());
        for task in top_level {
            result.push(task);
            if !self.collapsed.contains(&task.id) {
                if self.task_filter == TaskFilter::Done {
                    self.collect_done_children(&task.id, &mut result);
                } else {
                    self.collect_visible_children(&task.id, &mut result);
                }
            }
        }

        if matches!(self.task_filter, TaskFilter::Done | TaskFilter::Both)
            && let Some(pid) = self
                .projects
                .get(self.selected_project)
                .map(|p| p.id.clone())
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
        if !(self.task_filter == TaskFilter::Done && self.dock_filter.is_none() && !task.checked) {
            return false;
        }
        if self.has_completed_descendant(&task.id) {
            return true;
        }
        if let Some(pid) = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.as_str())
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

# Today View

A virtual sidebar entry below Inbox showing all overdue and due-today tasks across all projects in one place.

## Behavior

- Appears in the sidebar immediately below Inbox with a count badge (overdue + due today).
- Navigating to it shows tasks from all projects, not just the selected one.
- Overdue tasks appear in a collapsible "▼ Overdue (N)" section at the top, sorted oldest first.
- Today's tasks appear below, sorted by project then creation order.
- Each task row shows its source project name.
- Task completion (`x`) and detail pane (`Enter`) work the same as in any project.
- Empty state: "All caught up for today" when no tasks qualify.
- Updates in real time as sync events arrive.

## Filters

- Only includes tasks where `responsible_uid` is `None` (personal) or matches the current user (assigned to me in shared projects).
- Excludes completed and deleted tasks.
- Excludes subtasks (parent tasks only).

## Out of Scope (v1)

- Quick-add from the Today view.
- Bulk reschedule of overdue tasks.

# ratatoist

A terminal client for Todoist. These are the words I use for the things ratatoist shows and acts on.

## People

**User**:
The person running ratatoist, identified by the API token in use; Todoist's `user` resource.
_Avoid_: viewer, me, account holder

**Collaborator**:
Another person who shares a project with the user.
_Avoid_: teammate, member (Todoist's word for someone in a workspace)

## Tasks

**Task**:
One thing to do, with content, an optional description and the fields below.
_Avoid_: item (the Sync API's name for it), todo, issue

**Subtask**:
A task with a parent task (`parent_id`); it can have subtasks of its own.
_Avoid_: child task, sub-item

**Priority**:
How urgent a task is, shown as p1 (most urgent) to p4 (the default). The API numbers it the other way round: `priority: 4` is p1 and `priority: 1` is p4.
_Avoid_: a bare number such as "priority 4" (ambiguous between the API and the screen), urgency

**Due date**:
When a task is planned for, a date with an optional time, possibly recurring.
_Avoid_: deadline (a separate field), schedule

**Deadline**:
The date a task must be finished by, separate from its due date; it has no time and never recurs.
_Avoid_: due date, cutoff

**Recurring task**:
A task whose due date repeats (`is_recurring`). Completing it moves the due date to the next occurrence instead of closing the task.
_Avoid_: repeating task, routine

**Completed task**:
A task the user has checked off. ratatoist loads them from the completed-tasks route, not from the sync.
_Avoid_: done, closed, checked (the API's field, not a word for the UI), archived

**Comment**:
A note attached to a task, with optional attachment, written by the user or a collaborator.
_Avoid_: note (the Sync API's name for it), reply, annotation

## Organizing

**Project**:
A named list of tasks, with sections, subprojects and optional sharing.
_Avoid_: list, board

**Inbox**:
The project every Todoist account has, where tasks without a project land (`inbox_project`).
_Avoid_: default project, unsorted

**Section**:
A named group of tasks inside one project.
_Avoid_: group, header, column

**Label**:
A tag a task can carry any number of, shared across projects.
_Avoid_: tag, context

**Filter**:
A saved Todoist query (for example `today | overdue`) that selects tasks across projects.
_Avoid_: view, smart list, search

**List filter**:
What `f` narrows the task list on screen to: open, completed or all tasks, a due bucket (overdue, today, this week) or a priority. It exists only in ratatoist; a saved Filter is Todoist's.
_Avoid_: filter on its own where a saved Filter could be meant, view, done (say completed)

**Workspace**:
A Todoist team space whose projects belong to the team rather than to one user. Projects outside any workspace are the user's personal projects.
_Avoid_: team, organization, org

**Folder**:
A named group of projects inside a workspace.
_Avoid_: directory, project group

**Favourite**:
A project, label or filter the user has marked to keep at the top (`is_favorite`).
_Avoid_: star, pin, bookmark

## Progress

**Karma**:
Todoist's score for the user's productivity, with daily and weekly goals and streaks.
_Avoid_: points, score

## Sync

**Sync token**:
The opaque string Todoist returns with each sync; sending it back asks only for what changed since. `*` asks for everything (a full sync).
_Avoid_: cursor (the paging token of list endpoints), version, etag

**Command**:
One write sent to the Sync API (for example `item_close`), with its own uuid and, for a new object, a temp id.
_Avoid_: mutation, request, action (an entry in the key registry)

**Optimistic write**:
A change ratatoist shows at once, before Todoist confirms it, remembering what it replaced.
_Avoid_: optimistic update, pending edit

**Rollback**:
Restoring what an optimistic write replaced, after Todoist rejects the command or it never arrives.
_Avoid_: revert, undo (the user's own action)

## Failures

**Failure**:
Something ratatoist tried that did not work, with the action it was doing and the full message.
_Avoid_: exception, crash

**Notice**:
The one line that reports the latest failure or result; it never takes a keypress.
_Avoid_: toast, alert, popup

**Messages log**:
The failures of the session, newest first, opened with `!`.
_Avoid_: error log, history, console

## ratatoist itself

**Demo account**:
The fictional, deterministic Todoist account `--demo` runs against, with no network and no token. Snapshot tests use the same data.
_Avoid_: mock account, sample data, test account

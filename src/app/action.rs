use super::Screen;

/// Something the user can ask ratatoist to do. Keys, the palette and the
/// cheatsheet all name actions; `perform` carries them out. Every action is
/// listed once in [`crate::keys::ACTIONS`] with its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    /// Hide the release notice until a newer release ships.
    SkipRelease,
    /// Open the GitHub page of the release the notice shows.
    ShowReleaseNotes,
    ShowScreen(Screen),
    MoveDown,
    MoveUp,
    MoveToTop,
    MoveToBottom,
    HalfPageDown,
    HalfPageUp,
    /// Scroll the task detail one row.
    LineDown,
    LineUp,
    FocusLeft,
    FocusRight,
    /// Open the focused task's detail.
    Open,
    /// Open the task list of the focused project or label.
    OpenList,
    /// Close the top overlay, or step back: from the detail to its list,
    /// from a filtered list to the whole list, from an opened list to the
    /// screen it was opened from.
    Close,
    ShowCheatsheet,
    /// Open the palette, to find an action, a loaded task or a project.
    ShowPalette,
    /// Open the messages log.
    ShowMessages,
    /// Sync everything again from scratch (sync token `*`).
    Resync,
    /// Move an overlay's highlight, or scroll it, one row.
    ScrollDown,
    ScrollUp,
    /// Run or choose what an overlay has highlighted, or submit its text.
    Accept,
    /// Choose the entry with this number, from 1 to 9, counted among the
    /// entries the picker's filter leaves.
    PickNumber(u8),
    /// Run what the confirmation on top asks about.
    Confirm,
    /// Close the confirmation on top without running anything.
    Decline,
    /// Move to the next or previous field of the add-task form.
    NextField,
    PreviousField,
    /// Complete the focused task, or reopen it when it is completed.
    ToggleComplete,
    /// Open the add-task form.
    AddTask,
    /// Change a field of the focused task.
    Edit(Editable),
    /// Write a comment on the focused task.
    Comment,
    /// Fold or unfold the focused task's subtasks, the focused group header
    /// or the focused folder.
    ToggleFold,
    OpenAllFolds,
    CloseAllFolds,
    /// Pick what the task list on screen shows.
    FilterList,
    /// Pick how the task list on screen is sorted.
    SortList,
    /// Mark the focused project as a favourite, or unmark it.
    ToggleFavourite,
    /// Pick the theme, kept in `config.toml`.
    ChooseTheme,
    /// Load the listed project's tasks completed in the window before the
    /// oldest one loaded.
    LoadOlderCompleted,
}

/// A field of a task that [`Action::Edit`] changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Editable {
    Content,
    Description,
    DueDate,
    Priority,
}

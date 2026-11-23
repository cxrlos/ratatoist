use std::borrow::Cow;

use crossterm::event::KeyCode;

use super::{Context, Key};
use crate::app::{self, Action, Editable, ListFilter, Modal, Pane, Screen, State};
use crate::ui;

/// The narrowest terminal, in columns, that shows the task list and the
/// detail side by side.
const WIDE_COLUMNS: u16 = 120;

/// One action as the user sees it: what it is called, where it applies,
/// when it can run and which keys run it by default (the vim preset).
#[derive(Debug)]
pub struct ActionSpec {
    pub action: Action,
    /// The name used in `config.toml`, for example `move-down`.
    pub id: &'static str,
    /// What the palette and the keymap reference say it does.
    pub description: &'static str,
    /// What the cheatsheet says it does, in a few words. Actions of one
    /// pane with the same label share a line, as `j/k  down / up`.
    pub label: &'static str,
    /// The cheatsheet pane it is listed in.
    pub group: Group,
    /// Where the action shows in the bottom bar, if it does.
    pub hint: Option<Hint>,
    pub contexts: &'static [Context],
    /// The kinds of overlay it applies to, when it binds in
    /// [`Context::Overlay`]. The bottom bar leaves it out over any other
    /// kind, where it can never run; over these it is muted while it cannot
    /// run now.
    pub overlays: &'static [OverlayKind],
    /// Its keys in the vim preset. [`STANDARD`] lists the ones the
    /// standard preset changes.
    pub default_keys: &'static [&'static [Key]],
    /// Whether it can run now. Unavailable actions are still listed, muted.
    pub available: fn(&State) -> bool,
}

impl ActionSpec {
    /// What the palette says it does on `state`: its description, but with
    /// the version named where it is about a release.
    pub fn description_in(&self, state: &State) -> Cow<'static, str> {
        match (self.action, state.release.notice()) {
            (Action::ShowReleaseNotes, Some(version)) => {
                Cow::Owned(format!("What's new in v{version}"))
            }
            _ => Cow::Borrowed(self.description),
        }
    }

    /// Where the bottom bar shows it on `state`: its hint, but with the word
    /// the open prompt uses for Enter, as `send` for a comment.
    pub fn hint_in(&self, state: &State) -> Option<Hint> {
        match (self.action, state.top_modal()) {
            (Action::Accept, Some(Modal::Prompt(prompt))) => self.hint.map(|hint| Hint {
                word: prompt.field.verb(),
                ..hint
            }),
            _ => self.hint,
        }
    }

    /// Whether it can run now.
    pub fn availability(&self, state: &State) -> Availability {
        if (self.available)(state) {
            Availability::Available
        } else {
            Availability::Unavailable
        }
    }
}

/// Whether an action can run now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Available,
    /// It does not apply to what is on screen now.
    Unavailable,
}

impl Availability {
    pub fn can_run(self) -> bool {
        self == Availability::Available
    }
}

/// The cheatsheet's panes by purpose, in the order they are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Move,
    Edit,
    App,
    /// Keys of one screen or of the overlays, as the list's folds and
    /// filter. Its pane is named after their context.
    Screen,
}

impl Group {
    /// The pane's title, or `None` for [`Group::Screen`], whose title is
    /// its context's.
    pub fn title(self) -> Option<&'static str> {
        match self {
            Group::Move => Some("Move"),
            Group::Edit => Some("Edit the task"),
            Group::App => Some("App"),
            Group::Screen => None,
        }
    }
}

/// An action's place in the bottom bar: its row and the word after its key.
/// Actions in one row with the same word share an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint {
    pub row: HintRow,
    pub word: &'static str,
}

/// The rows of the bottom bar. [`HintRow::LINES`] places them on its three
/// lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintRow {
    Move,
    Do,
    /// The task edits.
    Edit,
    /// A screen's own keys, which change what it shows.
    View,
}

impl HintRow {
    /// The bar's lines, top first, each with the rows it holds in order.
    /// A screen's own keys lead the third line, so the task edits after
    /// them are the first to go when it runs out of room.
    pub const LINES: [&[HintRow]; 3] = [
        &[HintRow::Move],
        &[HintRow::Do],
        &[HintRow::View, HintRow::Edit],
    ];

    /// The label at the start of the row.
    pub fn label(self) -> &'static str {
        match self {
            HintRow::Move => "Move",
            HintRow::Do => "Do",
            HintRow::Edit => "Edit",
            HintRow::View => "View",
        }
    }
}

const fn hint(row: HintRow, word: &'static str) -> Option<Hint> {
    Some(Hint { row, word })
}

const fn moves(word: &'static str) -> Option<Hint> {
    hint(HintRow::Move, word)
}

const fn does(word: &'static str) -> Option<Hint> {
    hint(HintRow::Do, word)
}

const fn edits(word: &'static str) -> Option<Hint> {
    hint(HintRow::Edit, word)
}

const fn views(word: &'static str) -> Option<Hint> {
    hint(HintRow::View, word)
}

const GLOBAL: &[Context] = &[Context::Global];
const LIST: &[Context] = &[Context::List];
const DETAIL: &[Context] = &[Context::Detail];
const PANES: &[Context] = &[Context::List, Context::Detail];
/// Moves go through every list: tasks, the detail's rows, and the Projects
/// and Labels screens.
const MOVES: &[Context] = &[Context::List, Context::Detail, Context::Browse];
/// The screens whose rows open a task list.
const OPENS: &[Context] = &[Context::Browse];
/// The rows that fold: subtasks and group headers, and folders.
const FOLDS: &[Context] = &[Context::List, Context::Browse];
const PROJECTS: &[Context] = &[Context::Projects];
const OVERLAY: &[Context] = &[Context::Overlay];
/// Esc steps back from everything but the top level of a screen.
const CLOSES: &[Context] = &[
    Context::List,
    Context::Detail,
    Context::Browse,
    Context::Overlay,
];

/// What an overlay is, as far as which keys can ever apply to it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    /// The cheatsheet, which scrolls.
    Cheatsheet,
    /// A list to choose one entry from: the picker.
    List,
    Palette,
    /// A one-line prompt.
    Prompt,
    /// The add-task form.
    Form,
    /// A question answered with yes or no.
    Confirm,
    /// The messages log, which scrolls.
    Messages,
}

impl OverlayKind {
    pub const ALL: [OverlayKind; 7] = [
        OverlayKind::Cheatsheet,
        OverlayKind::List,
        OverlayKind::Palette,
        OverlayKind::Prompt,
        OverlayKind::Form,
        OverlayKind::Confirm,
        OverlayKind::Messages,
    ];

    /// The kind of `modal`, or `None` for the which-key popup, which only
    /// waits for the rest of a chord.
    pub fn of(modal: &Modal) -> Option<Self> {
        Some(match modal {
            Modal::Cheatsheet { .. } => OverlayKind::Cheatsheet,
            Modal::WhichKey { .. } => return None,
            Modal::Picker(_) => OverlayKind::List,
            Modal::Palette(_) => OverlayKind::Palette,
            Modal::Prompt(_) => OverlayKind::Prompt,
            Modal::Form(_) => OverlayKind::Form,
            Modal::Confirm(_) => OverlayKind::Confirm,
            Modal::Messages { .. } => OverlayKind::Messages,
        })
    }
}

/// The lists, which numbers pick from.
const LISTS: &[OverlayKind] = &[OverlayKind::List];
/// The overlays with something to move through.
const SCROLLING: &[OverlayKind] = &[
    OverlayKind::Cheatsheet,
    OverlayKind::List,
    OverlayKind::Palette,
    OverlayKind::Messages,
];
/// The overlays Enter chooses or submits in.
const ACCEPTING: &[OverlayKind] = &[
    OverlayKind::List,
    OverlayKind::Palette,
    OverlayKind::Prompt,
    OverlayKind::Form,
    OverlayKind::Confirm,
];

/// Every action ratatoist has, in the order hints and the cheatsheet list
/// them. This is the only place a key binding is written down.
pub static ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        action: Action::MoveDown,
        id: "move-down",
        description: "Move down",
        label: "down / up",
        group: Group::Move,
        hint: moves("down"),
        contexts: MOVES,
        overlays: &[],
        default_keys: &[&[Key::char('j')], &[Key::named(KeyCode::Down)]],
        available: can_move_down,
    },
    ActionSpec {
        action: Action::MoveUp,
        id: "move-up",
        description: "Move up",
        label: "down / up",
        group: Group::Move,
        hint: moves("up"),
        contexts: MOVES,
        overlays: &[],
        default_keys: &[&[Key::char('k')], &[Key::named(KeyCode::Up)]],
        available: can_move_up,
    },
    ActionSpec {
        action: Action::MoveToTop,
        id: "move-to-top",
        description: "Move to the top",
        label: "top / bottom",
        group: Group::Move,
        hint: moves("top"),
        contexts: MOVES,
        overlays: &[],
        default_keys: &[&[Key::char('g'), Key::char('g')]],
        available: can_move_up,
    },
    ActionSpec {
        action: Action::MoveToBottom,
        id: "move-to-bottom",
        description: "Move to the bottom",
        label: "top / bottom",
        group: Group::Move,
        hint: moves("bottom"),
        contexts: MOVES,
        overlays: &[],
        default_keys: &[&[Key::char('G')]],
        available: can_move_down,
    },
    ActionSpec {
        action: Action::HalfPageDown,
        id: "half-page-down",
        description: "Move down half a page",
        label: "half page",
        group: Group::Move,
        hint: moves("page"),
        contexts: MOVES,
        overlays: &[],
        default_keys: &[&[Key::ctrl('d')]],
        available: can_move_down,
    },
    ActionSpec {
        action: Action::HalfPageUp,
        id: "half-page-up",
        description: "Move up half a page",
        label: "half page",
        group: Group::Move,
        hint: moves("page"),
        contexts: MOVES,
        overlays: &[],
        default_keys: &[&[Key::ctrl('u')]],
        available: can_move_up,
    },
    ActionSpec {
        action: Action::LineDown,
        id: "line-down",
        description: "Scroll the detail down one row",
        label: "scroll a row",
        group: Group::Move,
        hint: None,
        contexts: DETAIL,
        overlays: &[],
        default_keys: &[&[Key::ctrl('e')]],
        available: can_move_down,
    },
    ActionSpec {
        action: Action::LineUp,
        id: "line-up",
        description: "Scroll the detail up one row",
        label: "scroll a row",
        group: Group::Move,
        hint: None,
        contexts: DETAIL,
        overlays: &[],
        default_keys: &[&[Key::ctrl('y')]],
        available: can_move_up,
    },
    ActionSpec {
        action: Action::FocusLeft,
        id: "focus-left",
        description: "Focus the pane on the left",
        label: "pane left / right",
        group: Group::Move,
        hint: moves("pane"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('h')]],
        available: is_wide,
    },
    ActionSpec {
        action: Action::FocusRight,
        id: "focus-right",
        description: "Focus the pane on the right",
        label: "pane left / right",
        group: Group::Move,
        hint: moves("pane"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('l')]],
        available: is_wide,
    },
    ActionSpec {
        action: Action::Open,
        id: "open",
        description: "Open the focused task",
        label: "open the task",
        group: Group::Move,
        hint: does("open"),
        contexts: LIST,
        overlays: &[],
        default_keys: &[&[Key::named(KeyCode::Enter)]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::OpenList,
        id: "open-list",
        description: "Open the focused project's or label's tasks",
        label: "open its tasks",
        group: Group::Move,
        hint: does("open"),
        contexts: OPENS,
        overlays: &[],
        default_keys: &[&[Key::named(KeyCode::Enter)]],
        available: always,
    },
    ActionSpec {
        action: Action::ScrollDown,
        id: "scroll-down",
        description: "Scroll down or move the highlight down",
        label: "down / up",
        group: Group::Move,
        hint: moves("move"),
        contexts: OVERLAY,
        overlays: SCROLLING,
        default_keys: &[
            &[Key::char('j')],
            &[Key::named(KeyCode::Down)],
            &[Key::ctrl('n')],
        ],
        available: can_scroll_down,
    },
    ActionSpec {
        action: Action::ScrollUp,
        id: "scroll-up",
        description: "Scroll up or move the highlight up",
        label: "down / up",
        group: Group::Move,
        hint: moves("move"),
        contexts: OVERLAY,
        overlays: SCROLLING,
        default_keys: &[
            &[Key::char('k')],
            &[Key::named(KeyCode::Up)],
            &[Key::ctrl('p')],
        ],
        available: can_scroll_up,
    },
    ActionSpec {
        action: Action::Accept,
        id: "accept",
        description: "Choose the highlighted entry or submit",
        label: "choose",
        group: Group::Screen,
        hint: does("choose"),
        contexts: OVERLAY,
        overlays: ACCEPTING,
        default_keys: &[&[Key::named(KeyCode::Enter)]],
        available: can_accept,
    },
    ActionSpec {
        action: Action::PickNumber(1),
        id: "pick-1",
        description: "Pick entry 1 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('1')]],
        available: |state| picker_has(1, state),
    },
    ActionSpec {
        action: Action::PickNumber(2),
        id: "pick-2",
        description: "Pick entry 2 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('2')]],
        available: |state| picker_has(2, state),
    },
    ActionSpec {
        action: Action::PickNumber(3),
        id: "pick-3",
        description: "Pick entry 3 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('3')]],
        available: |state| picker_has(3, state),
    },
    ActionSpec {
        action: Action::PickNumber(4),
        id: "pick-4",
        description: "Pick entry 4 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('4')]],
        available: |state| picker_has(4, state),
    },
    ActionSpec {
        action: Action::PickNumber(5),
        id: "pick-5",
        description: "Pick entry 5 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('5')]],
        available: |state| picker_has(5, state),
    },
    ActionSpec {
        action: Action::PickNumber(6),
        id: "pick-6",
        description: "Pick entry 6 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('6')]],
        available: |state| picker_has(6, state),
    },
    ActionSpec {
        action: Action::PickNumber(7),
        id: "pick-7",
        description: "Pick entry 7 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('7')]],
        available: |state| picker_has(7, state),
    },
    ActionSpec {
        action: Action::PickNumber(8),
        id: "pick-8",
        description: "Pick entry 8 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('8')]],
        available: |state| picker_has(8, state),
    },
    ActionSpec {
        action: Action::PickNumber(9),
        id: "pick-9",
        description: "Pick entry 9 of the list",
        label: "pick by number",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: LISTS,
        default_keys: &[&[Key::char('9')]],
        available: |state| picker_has(9, state),
    },
    ActionSpec {
        action: Action::Confirm,
        id: "confirm",
        description: "Confirm the question asked",
        label: "confirm / cancel",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: &[OverlayKind::Confirm],
        default_keys: &[&[Key::char('y')]],
        available: is_confirming,
    },
    ActionSpec {
        action: Action::Decline,
        id: "decline",
        description: "Cancel the question asked",
        label: "confirm / cancel",
        group: Group::Screen,
        hint: None,
        contexts: OVERLAY,
        overlays: &[OverlayKind::Confirm],
        default_keys: &[&[Key::char('n')]],
        available: is_confirming,
    },
    ActionSpec {
        action: Action::NextField,
        id: "next-field",
        description: "Move to the next field",
        label: "next / previous field",
        group: Group::Screen,
        hint: moves("field"),
        contexts: OVERLAY,
        overlays: &[OverlayKind::Form],
        default_keys: &[&[Key::named(KeyCode::Tab)]],
        available: in_form,
    },
    ActionSpec {
        action: Action::PreviousField,
        id: "previous-field",
        description: "Move to the previous field",
        label: "next / previous field",
        group: Group::Screen,
        hint: moves("field"),
        contexts: OVERLAY,
        overlays: &[OverlayKind::Form],
        default_keys: &[&[Key::shift(KeyCode::Tab)]],
        available: in_form,
    },
    ActionSpec {
        action: Action::Close,
        id: "close",
        description: "Close the top overlay or step back",
        label: "close or step back",
        group: Group::Move,
        hint: does("close"),
        contexts: CLOSES,
        overlays: &OverlayKind::ALL,
        default_keys: &[&[Key::named(KeyCode::Esc)]],
        available: can_close,
    },
    ActionSpec {
        action: Action::ToggleComplete,
        id: "toggle-complete",
        description: "Complete the task, or reopen it",
        label: "complete / reopen",
        group: Group::Edit,
        hint: edits("complete"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('x')]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::AddTask,
        id: "add-task",
        description: "Add a task",
        label: "add a task",
        group: Group::Edit,
        hint: edits("add"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('a')]],
        available: always,
    },
    ActionSpec {
        action: Action::Edit(Editable::Priority),
        id: "edit-priority",
        description: "Change the priority",
        label: "priority",
        group: Group::Edit,
        hint: edits("priority"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('p')]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::Edit(Editable::DueDate),
        id: "edit-due-date",
        description: "Set or clear the due date",
        label: "due date",
        group: Group::Edit,
        hint: edits("due"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('d')]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::Edit(Editable::Content),
        id: "edit-content",
        description: "Edit the content",
        label: "content / description",
        group: Group::Edit,
        hint: edits("edit"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('e')]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::Edit(Editable::Description),
        id: "edit-description",
        description: "Edit the description",
        label: "content / description",
        group: Group::Edit,
        hint: edits("edit"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('E')]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::Comment,
        id: "comment",
        description: "Comment on the task",
        label: "comment",
        group: Group::Edit,
        hint: edits("comment"),
        contexts: PANES,
        overlays: &[],
        default_keys: &[&[Key::char('c')]],
        available: has_focused_task,
    },
    ActionSpec {
        action: Action::ToggleFold,
        id: "toggle-fold",
        description: "Fold or unfold the focused row",
        label: "fold / unfold",
        group: Group::Screen,
        hint: views("fold"),
        contexts: FOLDS,
        overlays: &[],
        default_keys: &[&[Key::char('z'), Key::char('a')], &[Key::char(' ')]],
        available: always,
    },
    ActionSpec {
        action: Action::OpenAllFolds,
        id: "open-all-folds",
        description: "Unfold every row",
        label: "unfold / fold all",
        group: Group::Screen,
        hint: views("fold"),
        contexts: LIST,
        overlays: &[],
        default_keys: &[&[Key::char('z'), Key::char('R')]],
        available: always,
    },
    ActionSpec {
        action: Action::CloseAllFolds,
        id: "close-all-folds",
        description: "Fold every row",
        label: "unfold / fold all",
        group: Group::Screen,
        hint: views("fold"),
        contexts: LIST,
        overlays: &[],
        default_keys: &[&[Key::char('z'), Key::char('M')]],
        available: always,
    },
    ActionSpec {
        action: Action::FilterList,
        id: "filter",
        description: "Pick what the list shows",
        label: "filter / sort",
        group: Group::Screen,
        hint: views("filter"),
        contexts: LIST,
        overlays: &[],
        default_keys: &[&[Key::char('f')]],
        available: always,
    },
    ActionSpec {
        action: Action::SortList,
        id: "sort",
        description: "Pick how the list is sorted",
        label: "filter / sort",
        group: Group::Screen,
        hint: views("sort"),
        contexts: LIST,
        overlays: &[],
        default_keys: &[&[Key::char('v'), Key::char('s')]],
        available: always,
    },
    ActionSpec {
        action: Action::ToggleFavourite,
        id: "toggle-favourite",
        description: "Mark or unmark the project as a favourite",
        label: "favourite",
        group: Group::Screen,
        hint: views("favourite"),
        contexts: PROJECTS,
        overlays: &[],
        default_keys: &[&[Key::char('s')]],
        available: has_focused_project,
    },
    ActionSpec {
        action: Action::ShowScreen(Screen::Inbox),
        id: "show-inbox",
        description: "Show the Inbox",
        label: "screens",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('1')]],
        available: |state| state.screen != Screen::Inbox,
    },
    ActionSpec {
        action: Action::ShowScreen(Screen::Today),
        id: "show-today",
        description: "Show Today",
        label: "screens",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('2')]],
        available: |state| state.screen != Screen::Today,
    },
    ActionSpec {
        action: Action::ShowScreen(Screen::Upcoming),
        id: "show-upcoming",
        description: "Show Upcoming",
        label: "screens",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('3')]],
        available: |state| state.screen != Screen::Upcoming,
    },
    ActionSpec {
        action: Action::ShowScreen(Screen::Projects),
        id: "show-projects",
        description: "Show Projects",
        label: "screens",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('4')]],
        available: |state| state.screen != Screen::Projects,
    },
    ActionSpec {
        action: Action::ShowScreen(Screen::Labels),
        id: "show-labels",
        description: "Show Labels",
        label: "screens",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('5')]],
        available: |state| state.screen != Screen::Labels,
    },
    ActionSpec {
        action: Action::Resync,
        id: "resync",
        description: "Sync everything again",
        label: "re-sync",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('R')]],
        available: always,
    },
    ActionSpec {
        action: Action::ShowCheatsheet,
        id: "show-cheatsheet",
        description: "Show every key for this context",
        label: "keys",
        group: Group::App,
        hint: does("keys"),
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('?')]],
        available: always,
    },
    ActionSpec {
        action: Action::ShowPalette,
        id: "show-palette",
        description: "Find an action, task or project",
        label: "palette",
        group: Group::App,
        hint: does("palette"),
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char(':')], &[Key::ctrl('k')]],
        available: always,
    },
    ActionSpec {
        action: Action::ShowMessages,
        id: "show-messages",
        description: "Show the messages log",
        label: "messages",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('!')]],
        available: always,
    },
    ActionSpec {
        action: Action::Quit,
        id: "quit",
        description: "Quit ratatoist",
        label: "quit",
        group: Group::App,
        hint: does("quit"),
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[&[Key::char('q')], &[Key::ctrl('c')]],
        available: always,
    },
    // The palette runs these; they have no keys.
    ActionSpec {
        action: Action::ChooseTheme,
        id: "choose-theme",
        description: "Choose a theme for this session",
        label: "theme",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[],
        available: always,
    },
    ActionSpec {
        action: Action::SkipRelease,
        id: "skip-release",
        description: "Skip this release",
        label: "skip release",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[],
        available: knows_newer_release,
    },
    ActionSpec {
        action: Action::ShowReleaseNotes,
        id: "release-notes",
        description: "What's new in the latest release",
        label: "release notes",
        group: Group::App,
        hint: None,
        contexts: GLOBAL,
        overlays: &[],
        default_keys: &[],
        available: knows_newer_release,
    },
];

/// A set of default keys, chosen with `[keys] preset` in `config.toml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Preset {
    /// The keys in [`ACTIONS`].
    #[default]
    Vim,
    /// Arrows, Home/End, PgUp/PgDn and Tab, with Ctrl on the keys that
    /// would otherwise be letters. Only what [`STANDARD`] lists changes.
    Standard,
}

impl Preset {
    pub const ALL: [Preset; 2] = [Preset::Vim, Preset::Standard];

    /// Its value for `[keys] preset`.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Vim => "vim",
            Preset::Standard => "standard",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    /// The keys `spec`'s action has in this preset.
    pub fn default_keys(self, spec: &ActionSpec) -> &'static [&'static [Key]] {
        match self {
            Preset::Vim => spec.default_keys,
            Preset::Standard => STANDARD
                .iter()
                .find(|(action, _)| *action == spec.action)
                .map_or(spec.default_keys, |(_, keys)| *keys),
        }
    }
}

/// The actions whose keys the standard preset changes, with their keys
/// there. Every other action keeps its key from [`ACTIONS`].
pub static STANDARD: &[(Action, &[&[Key]])] = &[
    (Action::MoveDown, &[&[Key::named(KeyCode::Down)]]),
    (Action::MoveUp, &[&[Key::named(KeyCode::Up)]]),
    (Action::MoveToTop, &[&[Key::named(KeyCode::Home)]]),
    (Action::MoveToBottom, &[&[Key::named(KeyCode::End)]]),
    (Action::HalfPageDown, &[&[Key::named(KeyCode::PageDown)]]),
    (Action::HalfPageUp, &[&[Key::named(KeyCode::PageUp)]]),
    (Action::FocusLeft, &[&[Key::shift(KeyCode::Tab)]]),
    (Action::FocusRight, &[&[Key::named(KeyCode::Tab)]]),
    (
        Action::ScrollDown,
        &[&[Key::named(KeyCode::Down)], &[Key::ctrl('n')]],
    ),
    (
        Action::ScrollUp,
        &[&[Key::named(KeyCode::Up)], &[Key::ctrl('p')]],
    ),
    (Action::ToggleComplete, &[&[Key::ctrl('x')]]),
    (Action::AddTask, &[&[Key::ctrl('a')]]),
    (Action::ToggleFold, &[&[Key::char(' ')]]),
    (Action::ShowPalette, &[&[Key::ctrl('k')]]),
];

fn always(_: &State) -> bool {
    true
}

fn knows_newer_release(state: &State) -> bool {
    state.release.notice().is_some()
}

fn has_focused_task(state: &State) -> bool {
    state.selected_task().is_some()
}

fn has_focused_project(state: &State) -> bool {
    state.projects.get(state.selected_project).is_some()
}

fn is_wide(state: &State) -> bool {
    state.screen_size().width >= WIDE_COLUMNS
}

fn can_move_down(_: &State) -> bool {
    true
}

fn can_move_up(_: &State) -> bool {
    true
}

/// Whether the overlay on top has somewhere further down to go: the
/// cheatsheet a hidden row below, the palette a match below the highlight.
fn can_scroll_down(state: &State) -> bool {
    match state.top_modal() {
        Some(Modal::Cheatsheet { scroll }) => {
            *scroll < ui::cheatsheet_max_scroll(state, state.screen_size())
        }
        Some(Modal::Palette(_)) => app::palette_can_step_down(state),
        other => other.is_some(),
    }
}

fn can_scroll_up(state: &State) -> bool {
    match state.top_modal() {
        Some(Modal::Cheatsheet { scroll }) => *scroll > 0,
        Some(Modal::Palette(palette)) => palette.selected > 0,
        other => other.is_some(),
    }
}

fn can_accept(state: &State) -> bool {
    match state.top_modal() {
        Some(Modal::Palette(_)) => app::palette_has_selection(state),
        other => matches!(
            other,
            Some(Modal::Picker(_) | Modal::Prompt(_) | Modal::Form(_) | Modal::Confirm(_))
        ),
    }
}

fn can_close(state: &State) -> bool {
    state.top_modal().is_some()
        || state.active_pane == Pane::Detail
        || state.list_filter != ListFilter::default()
}

fn is_confirming(state: &State) -> bool {
    matches!(state.top_modal(), Some(Modal::Confirm(_)))
}

fn in_form(state: &State) -> bool {
    matches!(state.top_modal(), Some(Modal::Form(_)))
}

/// Whether a picker is open with at least `count` entries left by its
/// filter.
fn picker_has(count: usize, state: &State) -> bool {
    match state.top_modal() {
        Some(Modal::Picker(picker)) => picker.visible().len() >= count,
        _ => false,
    }
}

/// The registry entry for `action`.
pub fn spec(action: Action) -> Option<&'static ActionSpec> {
    ACTIONS.iter().find(|spec| spec.action == action)
}

/// The registry entry whose config name is `id`.
pub fn by_id(id: &str) -> Option<&'static ActionSpec> {
    ACTIONS.iter().find(|spec| spec.id == id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn ids_and_actions_are_unique() {
        let ids: HashSet<_> = ACTIONS.iter().map(|spec| spec.id).collect();
        assert_eq!(ids.len(), ACTIONS.len());
        for spec in ACTIONS {
            assert!(std::ptr::eq(super::spec(spec.action).unwrap(), spec));
        }
    }

    #[test]
    fn standard_lists_each_action_once_and_only_known_ones() {
        let actions: HashSet<_> = STANDARD.iter().map(|(action, _)| *action).collect();
        assert_eq!(actions.len(), STANDARD.len());
        for (action, _) in STANDARD {
            assert!(spec(*action).is_some());
        }
    }

    /// Within one context of one preset, no key sequence runs two actions,
    /// and no binding is the prefix of another (a chord would shadow it).
    #[test]
    fn no_two_bindings_clash_within_a_context() {
        for preset in Preset::ALL {
            for context in Context::ALL {
                let mut bound: Vec<(&[Key], &str)> = Vec::new();
                for spec in ACTIONS
                    .iter()
                    .filter(|spec| spec.contexts.contains(&context))
                {
                    for keys in preset.default_keys(spec) {
                        for (other, id) in &bound {
                            let clash = keys.starts_with(other) || other.starts_with(keys);
                            assert!(
                                !clash || overlays_differ(spec.id, id),
                                "{} and {id} clash in {} ({})",
                                spec.id,
                                context.name(),
                                preset.name(),
                            );
                        }
                        bound.push((keys, spec.id));
                    }
                }
            }
        }
    }

    /// Two overlay actions on one key are fine when no overlay kind takes
    /// both, as Enter choosing in a picker and accepting a confirmation.
    fn overlays_differ(a: &str, b: &str) -> bool {
        let (Some(a), Some(b)) = (by_id(a), by_id(b)) else {
            return false;
        };
        a.overlays.iter().all(|kind| !b.overlays.contains(kind))
            && !a.overlays.is_empty()
            && !b.overlays.is_empty()
    }

    /// Keys later milestones claim stay unbound outside overlays.
    #[test]
    fn reserved_keys_stay_unbound() {
        let reserved = ['/', 'u', 't', 'L', 'm', 'D', 'F', 'V', 'o', 'y', '6'];
        for preset in Preset::ALL {
            for spec in ACTIONS.iter().filter(|spec| spec.contexts != OVERLAY) {
                for keys in preset.default_keys(spec) {
                    let first = keys.first().and_then(|key| key.typed());
                    assert!(
                        first.is_none_or(|c| !reserved.contains(&c)),
                        "{} takes a reserved key",
                        spec.id
                    );
                }
            }
        }
    }

    #[test]
    fn every_screen_has_a_number_key() {
        for screen in Screen::ALL {
            assert!(
                spec(Action::ShowScreen(screen)).is_some_and(|spec| !spec.default_keys.is_empty())
            );
        }
    }

    #[test]
    fn edits_are_listed() {
        for field in [
            Editable::Content,
            Editable::Description,
            Editable::DueDate,
            Editable::Priority,
        ] {
            assert!(spec(Action::Edit(field)).is_some());
        }
    }

    #[test]
    fn the_standard_preset_drops_vim_letters_for_moves() {
        let down = spec(Action::MoveDown).unwrap();
        assert_eq!(
            Preset::Standard.default_keys(down),
            &[&[Key::named(KeyCode::Down)] as &[Key]]
        );
    }
}

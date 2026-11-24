use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use jiff::civil::Date;
use jiff::{SignedDuration, Zoned};
use ratatui::layout::Size;

use super::browse::TreeItem;
use super::failure::{ErrorLog, Failure, LoadError};
use super::modal::Modal;
use super::release::ReleaseState;
use super::store::{Stamped, Store};
use super::sync::OptimisticOp;
use super::text_field::TextField;
use super::view::Lists;
use crate::auth::Source;
use crate::keys::Keymap;
use crate::todoist::CompletedWindow;
use crate::todoist::model::{Comment, Priority, Task};
use crate::ui::icons::{IconSet, Icons};
use crate::ui::theme::{self, Theme};

/// How long the session waits without a key before it counts as idle.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;

/// The narrowest terminal, in columns, that shows a task list and the
/// focused task's detail side by side, unless `[ui] wide-layout-columns`
/// says otherwise.
pub const DEFAULT_WIDE_LAYOUT_COLUMNS: u16 = 120;

/// Everything the screen shows, and nothing else: no client, channel,
/// terminal or file handle. Rendering reads only this.
#[derive(Debug, Clone)]
pub struct State {
    /// Set by the runtime from the clock; fixed in tests.
    pub today: Date,
    pub now: Zoned,

    /// True until the first sync answers; the body says it is loading
    /// meanwhile.
    pub loading: bool,
    /// Why the task lists cannot load, when they cannot; the body shows an
    /// error screen instead.
    pub load_error: Option<LoadError>,
    /// Where the token in use came from, to say how to replace it when
    /// Todoist rejects it.
    pub token_source: Option<Source>,
    /// Every synced entity, by id.
    pub store: Store,
    pub selected_project: usize,
    pub selected_task: usize,
    /// The pane that takes keys: a task list, the focused task's detail, or
    /// the tree of the Projects or Labels screen.
    pub active_pane: Pane,
    /// From this many columns on, a task list and the detail of its focused
    /// task show side by side; below it the focused pane fills the body.
    pub wide_layout_columns: u16,
    /// The keys in force: the preset and the overrides from `config.toml`.
    pub keymap: Keymap,
    /// Overlays, drawn bottom to top; the top one takes keys. A
    /// [`Modal::WhichKey`] on top holds the chord typed so far.
    pub modals: Vec<Modal>,
    /// The text of the open prompt, or of the add-task form's active text
    /// field.
    pub input: TextField,
    /// The task the open prompt or priority picker writes to, taken when it
    /// opened, so a sync that reorders the list cannot retarget it.
    pub edit_task: Option<String>,
    /// The tasks whose subtasks are folded away.
    pub collapsed: Stamped<HashSet<String>>,
    /// Rows the task detail is scrolled down by. Scroll keys keep it within
    /// the content at [`State::screen_size`].
    pub detail_scroll: u16,
    /// The terminal size: read when the app starts, then from each resize.
    /// `None` only for a `State` no terminal has shown.
    pub viewport: Option<Size>,
    /// The screen the tab row has selected.
    pub screen: Screen,
    /// A result or a hint, shown above the bottom bar until the next key.
    pub notice: Option<String>,
    /// The newest failure is the notice until the next key, in place of
    /// [`State::notice`].
    pub show_failure: bool,
    /// The failures of this session, for the messages log.
    pub failures: ErrorLog,
    /// Tasks whose last write was rolled back, with why. Their rows show the
    /// failed mark until they change again.
    pub rolled_back: HashMap<String, Failure>,
    /// Due strings sent for tasks whose parsed due date has not come back
    /// yet, by task id. The task shows the string until Todoist answers
    /// with the parsed date.
    pub pending_due: HashMap<String, String>,
    /// What is known about newer releases, for the tab row.
    pub release: ReleaseState,
    /// How the task list on screen is sorted.
    pub sort: Sort,
    /// The comments the detail shows, of the task it shows.
    pub comments: Vec<Comment>,
    /// What the task list on screen narrows to.
    pub list_filter: ListFilter,
    /// The glyphs every view draws with, from `[ui] icons`.
    pub icon_set: IconSet,
    /// The themes the theme picker offers; `theme_idx` is the one drawn.
    pub themes: Vec<Theme>,
    pub theme_idx: usize,
    /// `NO_COLOR` is set: the one theme is [`Theme::no_color`], and a theme
    /// picked is not written to `config.toml`.
    pub no_color: bool,
    /// How reading from Todoist went the last time, for the sync indicator.
    pub sync_health: SyncHealth,
    /// The live connection dropped and has not come back yet.
    pub live_dropped: bool,
    pub sync_token: String,
    /// The completed tasks loaded for each project, by project id.
    pub completed_cache: Stamped<HashMap<String, Vec<Task>>>,
    /// The oldest window loaded into `completed_cache`, by project.
    pub oldest_completed: HashMap<String, CompletedWindow>,
    pub idle_timeout_secs: u64,
    pub last_sync_at: Option<Zoned>,
    pub collapsed_folders: HashSet<String>,
    /// The row the Projects tree focuses, when it is on one.
    pub tree_focus: Option<TreeItem>,
    /// The label the Labels screen focuses, by name.
    pub label_focus: Option<String>,
    /// The label whose tasks the list on the Labels screen shows.
    pub opened_label: Option<String>,
    /// Projects whose last write was rolled back, with why, until they
    /// change again.
    pub rolled_back_projects: HashMap<String, Failure>,
    pub overdue_section_collapsed: bool,
    pub last_activity: Zoned,
    pub(crate) pending_ws_sync: bool,
    pub(crate) comments_fetch_seq: u64,
    pub(crate) websocket_url: Option<String>,
    pub(crate) live_opened: bool,
    /// The user the cache painted from belongs to, until the first sync
    /// answers and confirms it.
    pub(crate) cache_user_id: Option<String>,
    /// The cached user to check against the user asked for, when the first
    /// sync from the cache came back without one.
    pub(crate) user_to_confirm: Option<String>,
    /// When a merge first changed the store since the cache was last
    /// written.
    pub(crate) cache_dirty_since: Option<jiff::Timestamp>,
    /// Revert snapshots of optimistic writes, keyed by command uuid.
    pub(crate) temp_id_pending: HashMap<String, OptimisticOp>,
    /// What each command still waiting for Todoist does, as `complete "Buy
    /// oat milk"`, keyed by its uuid, to name it if it fails.
    pub(crate) pending_actions: HashMap<String, String>,
    pub(crate) next_temp_id: u64,
    /// The lists derived from the store and the list state, rebuilt on the
    /// first read after either changes.
    pub(crate) lists: RefCell<Option<Lists>>,
    /// The tasks and projects last found in a parent cycle, so each cycle
    /// is logged once.
    pub(crate) reported_cycles: HashSet<String>,
}

impl State {
    pub fn new(now: Zoned) -> Self {
        Self {
            today: now.date(),
            now: now.clone(),
            loading: true,
            load_error: None,
            token_source: None,
            store: Store::default(),
            selected_project: 0,
            selected_task: 0,
            active_pane: Pane::Tasks,
            wide_layout_columns: DEFAULT_WIDE_LAYOUT_COLUMNS,
            keymap: Keymap::default(),
            modals: Vec::new(),
            input: TextField::default(),
            edit_task: None,
            collapsed: Stamped::default(),
            detail_scroll: 0,
            viewport: None,
            screen: Screen::Today,
            notice: None,
            show_failure: false,
            failures: ErrorLog::default(),
            rolled_back: HashMap::new(),
            pending_due: HashMap::new(),
            release: ReleaseState::default(),
            sort: Sort::default(),
            comments: Vec::new(),
            list_filter: ListFilter::default(),
            icon_set: IconSet::default(),
            themes: Theme::built_ins(),
            theme_idx: 0,
            no_color: false,
            sync_health: SyncHealth::Fine,
            live_dropped: false,
            sync_token: "*".to_owned(),
            completed_cache: Stamped::default(),
            oldest_completed: HashMap::new(),
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            last_sync_at: None,
            collapsed_folders: HashSet::new(),
            tree_focus: None,
            label_focus: None,
            opened_label: None,
            rolled_back_projects: HashMap::new(),
            overdue_section_collapsed: false,
            last_activity: now,
            pending_ws_sync: false,
            comments_fetch_seq: 0,
            websocket_url: None,
            live_opened: false,
            cache_user_id: None,
            user_to_confirm: None,
            cache_dirty_since: None,
            temp_id_pending: HashMap::new(),
            pending_actions: HashMap::new(),
            next_temp_id: 0,
            lists: RefCell::new(None),
            reported_cycles: HashSet::new(),
        }
    }

    pub fn theme(&self) -> &Theme {
        self.themes
            .get(self.theme_idx)
            .or_else(|| self.themes.first())
            .unwrap_or_else(|| fallback_theme())
    }

    /// The glyphs of the icon set in use.
    pub fn icons(&self) -> &'static Icons {
        self.icon_set.icons()
    }

    /// The due string sent for `task_id` whose parsed date has not come back
    /// yet, if any.
    pub fn pending_due(&self, task_id: &str) -> Option<&str> {
        self.pending_due.get(task_id).map(String::as_str)
    }

    pub fn top_modal(&self) -> Option<&Modal> {
        self.modals.last()
    }

    pub fn top_modal_mut(&mut self) -> Option<&mut Modal> {
        self.modals.last_mut()
    }

    /// The terminal size, assuming 80x24 when no terminal has reported one.
    pub fn screen_size(&self) -> Size {
        self.viewport.unwrap_or(Size::new(80, 24))
    }

    /// Whether the terminal is wide enough to show a task list and the
    /// detail side by side.
    pub fn is_wide(&self) -> bool {
        self.screen_size().width >= self.wide_layout_columns
    }

    /// Shows the error screen for `error` in place of the task lists; no
    /// sync runs.
    pub fn not_loaded(&mut self, error: LoadError) {
        self.loading = false;
        self.load_error = Some(error);
    }

    /// Adds `failure` to the messages log and shows it as the notice until
    /// the next key: for what the user asked for, and the first sync.
    pub(crate) fn report(&mut self, failure: Failure) {
        self.show_failure = true;
        self.log_failure(failure);
    }

    /// Adds `failure` to the messages log alone: for what ratatoist does in
    /// the background.
    pub(crate) fn log_failure(&mut self, failure: Failure) {
        tracing::error!(
            kind = failure.kind.name(),
            error = %failure.message,
            "failed"
        );
        self.failures.record(failure);
    }

    /// Why the last write to `task_id` was rolled back, while the task has
    /// not changed since.
    pub fn rollback_reason(&self, task_id: &str) -> Option<&Failure> {
        self.rolled_back.get(task_id)
    }

    /// What the sync indicator shows: `synced 4s ago`, or what is wrong.
    /// `None` before anything has synced or failed, and while an error
    /// screen fills the body, which already says what is wrong.
    pub fn indicator(&self) -> Option<Indicator> {
        if self.load_error.is_some() {
            return None;
        }
        let problem = match self.sync_health {
            SyncHealth::Offline => Some("offline"),
            SyncHealth::Failing => Some("sync failing"),
            SyncHealth::Fine if self.live_dropped => Some("reconnecting"),
            SyncHealth::Fine => None,
        };
        if let Some(text) = problem {
            return Some(Indicator {
                text: text.to_owned(),
                alert: true,
            });
        }
        let ago = age(self.now.duration_since(self.last_sync_at.as_ref()?));
        let text = if self.is_idle() {
            format!("idle, synced {ago} ago")
        } else {
            format!("synced {ago} ago")
        };
        Some(Indicator { text, alert: false })
    }

    pub fn is_idle(&self) -> bool {
        let timeout = i64::try_from(self.idle_timeout_secs).unwrap_or(i64::MAX);
        self.idle_timeout_secs > 0
            && self.now.duration_since(&self.last_activity).as_secs() >= timeout
    }

    /// Whether `name` names one of the loaded themes, by its name or the
    /// same in lower case with hyphens for spaces; selects it if so.
    pub fn select_theme_named(&mut self, name: &str) -> bool {
        let wanted = theme::slug(name);
        match self
            .themes
            .iter()
            .position(|t| theme::slug(&t.name) == wanted)
        {
            Some(idx) => {
                self.theme_idx = idx;
                true
            }
            None => false,
        }
    }
}

/// `elapsed` in its largest whole unit, as `4s`, `3m`, `2h` or `5d`.
fn age(elapsed: SignedDuration) -> String {
    let seconds = elapsed.as_secs().max(0);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

/// How reading from Todoist went the last time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SyncHealth {
    #[default]
    Fine,
    /// The last sync could not reach Todoist.
    Offline,
    /// The last sync reached Todoist and failed.
    Failing,
}

/// What the sync indicator shows, and whether it calls for attention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indicator {
    pub text: String,
    /// Sync is failing or the live connection is down.
    pub alert: bool,
}

fn fallback_theme() -> &'static Theme {
    static FALLBACK: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
    FALLBACK.get_or_init(Theme::default)
}

/// The screens of the tab row, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    Inbox,
    Today,
    Upcoming,
    Projects,
    Labels,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Inbox,
        Screen::Today,
        Screen::Upcoming,
        Screen::Projects,
        Screen::Labels,
    ];

    /// Its name in the tab row.
    pub fn title(self) -> &'static str {
        match self {
            Screen::Inbox => "Inbox",
            Screen::Today => "Today",
            Screen::Upcoming => "Upcoming",
            Screen::Projects => "Projects",
            Screen::Labels => "Labels",
        }
    }
}

/// The list filter: what a task list shows, chosen with the filter picker.
/// The default shows open tasks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListFilter {
    #[default]
    Open,
    Completed,
    /// Open and completed tasks.
    All,
    Overdue,
    DueToday,
    /// Due today or in the six days after.
    DueThisWeek,
    Priority(Priority),
}

impl ListFilter {
    /// Every list filter, in the order the filter picker lists them.
    pub const ALL: [ListFilter; 10] = [
        ListFilter::Open,
        ListFilter::Completed,
        ListFilter::All,
        ListFilter::Overdue,
        ListFilter::DueToday,
        ListFilter::DueThisWeek,
        ListFilter::Priority(Priority::P1),
        ListFilter::Priority(Priority::P2),
        ListFilter::Priority(Priority::P3),
        ListFilter::Priority(Priority::P4),
    ];

    /// Its name in the filter picker.
    pub fn label(self) -> &'static str {
        match self {
            ListFilter::Open => "Open",
            ListFilter::Completed => "Completed",
            ListFilter::All => "All",
            ListFilter::Overdue => "Overdue",
            ListFilter::DueToday => "Due today",
            ListFilter::DueThisWeek => "Due this week",
            ListFilter::Priority(priority) => priority.label(),
        }
    }
}

/// The list filters [`State::overview_stats`] counts.
pub const COUNTED_FILTERS: [ListFilter; 7] = [
    ListFilter::Overdue,
    ListFilter::DueToday,
    ListFilter::DueThisWeek,
    ListFilter::Priority(Priority::P1),
    ListFilter::Priority(Priority::P2),
    ListFilter::Priority(Priority::P3),
    ListFilter::Priority(Priority::P4),
];

/// How a task list is sorted, chosen with the sort picker. The default keeps
/// Todoist's own order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Order,
    Priority,
    DueDate,
    Created,
}

impl Sort {
    /// Every sort, in the order the sort picker lists them.
    pub const ALL: [Sort; 4] = [Sort::Order, Sort::Priority, Sort::DueDate, Sort::Created];

    /// Its name in the sort picker.
    pub fn label(self) -> &'static str {
        match self {
            Sort::Order => "Order",
            Sort::Priority => "Priority",
            Sort::DueDate => "Due date",
            Sort::Created => "Created",
        }
    }
}

/// What takes keys on the screen: the tree of the Projects or Labels
/// screen, a task list, or the focused task's detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Browse,
    Tasks,
    Detail,
}

pub struct OverviewStats {
    pub due_today: u32,
    pub due_week: u32,
    pub overdue: u32,
    pub by_priority: [u32; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserRecord {
    pub id: String,
    /// What a comment names its author by: the full name, or else the
    /// email, or else the id.
    pub name: String,
}

impl UserRecord {
    pub fn new(id: String, full_name: Option<String>, email: Option<String>) -> Self {
        let name = [full_name, email]
            .into_iter()
            .flatten()
            .find(|name| !name.is_empty())
            .unwrap_or_else(|| id.clone());
        Self { id, name }
    }
}

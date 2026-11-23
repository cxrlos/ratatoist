use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, NaiveDate};
use ratatui::layout::Size;

use super::failure::{ErrorLog, Failure, LoadError};
use super::modal::Modal;
use super::release::ReleaseState;
use super::sync::OptimisticOp;
use super::text_field::TextField;
use crate::auth::Source;
use crate::keys::Keymap;
use crate::todoist::model::{Comment, Folder, Label, Priority, Project, Section, Task, Workspace};
use crate::ui::icons::{IconSet, Icons};
use crate::ui::theme::{self, Theme};
use crate::ui::views::detail::DetailField;

/// How long the session waits without a key before it counts as idle.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;

/// Everything the screen shows, and nothing else: no client, channel,
/// terminal or file handle. Rendering reads only this.
#[derive(Debug, Clone)]
pub struct State {
    /// Set by the runtime from the clock; fixed in tests.
    pub today: NaiveDate,
    pub now: DateTime<Local>,

    /// True until the first sync answers; the body says it is loading
    /// meanwhile.
    pub loading: bool,
    /// Why the task lists cannot load, when they cannot; the body shows an
    /// error screen instead.
    pub load_error: Option<LoadError>,
    /// Where the token in use came from, to say how to replace it when
    /// Todoist rejects it.
    pub token_source: Option<Source>,
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub folders: Vec<Folder>,
    pub tasks: Vec<Task>,
    pub labels: Vec<Label>,
    pub sections: Vec<Section>,
    pub selected_project: usize,
    pub selected_task: usize,
    pub active_pane: Pane,
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
    pub collapsed: HashSet<String>,
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
    /// What is known about newer releases, for the tab row.
    pub release: ReleaseState,
    pub sort_mode: SortMode,
    pub comments: Vec<Comment>,
    pub detail_field: DetailField,
    pub current_user_id: Option<String>,
    pub user_names: HashMap<String, UserRecord>,
    pub task_filter: TaskFilter,
    pub dock_filter: Option<DockItem>,
    /// The glyphs every view draws with, from `[ui] icons`.
    pub icon_set: IconSet,
    /// The themes the theme picker offers; `theme_idx` is the one drawn.
    pub themes: Vec<Theme>,
    pub theme_idx: usize,
    /// How reading from Todoist went the last time, for the sync indicator.
    pub sync_health: SyncHealth,
    /// The live connection dropped and has not come back yet.
    pub live_dropped: bool,
    pub sync_token: String,
    pub completed_cache: HashMap<String, Vec<Task>>,
    pub comments_by_task: HashMap<String, Vec<Comment>>,
    pub idle_timeout_secs: u64,
    pub last_sync_at: Option<DateTime<Local>>,
    pub collapsed_folders: HashSet<String>,
    pub folder_cursor: Option<usize>,
    pub current_user_name: Option<String>,
    pub overdue_section_collapsed: bool,
    pub last_activity: DateTime<Local>,
    pub(crate) pending_ws_sync: bool,
    pub(crate) comments_fetch_seq: u64,
    pub(crate) websocket_url: Option<String>,
    pub(crate) live_opened: bool,
    /// Revert snapshots of optimistic writes, keyed by command uuid.
    pub(crate) temp_id_pending: HashMap<String, OptimisticOp>,
    /// What each command still waiting for Todoist does, as `complete "Buy
    /// oat milk"`, keyed by its uuid, to name it if it fails.
    pub(crate) pending_actions: HashMap<String, String>,
    pub(crate) next_temp_id: u64,
}

impl State {
    pub fn new(now: DateTime<Local>) -> Self {
        Self {
            today: now.date_naive(),
            now,
            loading: true,
            load_error: None,
            token_source: None,
            projects: Vec::new(),
            workspaces: Vec::new(),
            folders: Vec::new(),
            tasks: Vec::new(),
            labels: Vec::new(),
            sections: Vec::new(),
            selected_project: 0,
            selected_task: 0,
            active_pane: Pane::Tasks,
            keymap: Keymap::default(),
            modals: Vec::new(),
            input: TextField::default(),
            edit_task: None,
            collapsed: HashSet::new(),
            detail_scroll: 0,
            viewport: None,
            screen: Screen::Today,
            notice: None,
            show_failure: false,
            failures: ErrorLog::default(),
            rolled_back: HashMap::new(),
            release: ReleaseState::default(),
            sort_mode: SortMode::Default,
            comments: Vec::new(),
            detail_field: DetailField::Content,
            current_user_id: None,
            user_names: HashMap::new(),
            task_filter: TaskFilter::Active,
            dock_filter: None,
            icon_set: IconSet::default(),
            themes: Theme::built_ins(),
            theme_idx: 0,
            sync_health: SyncHealth::Fine,
            live_dropped: false,
            sync_token: "*".to_owned(),
            completed_cache: HashMap::new(),
            comments_by_task: HashMap::new(),
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            last_sync_at: None,
            collapsed_folders: HashSet::new(),
            folder_cursor: None,
            current_user_name: None,
            overdue_section_collapsed: false,
            last_activity: now,
            pending_ws_sync: false,
            comments_fetch_seq: 0,
            websocket_url: None,
            live_opened: false,
            temp_id_pending: HashMap::new(),
            pending_actions: HashMap::new(),
            next_temp_id: 0,
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
    /// `None` before anything has synced or failed.
    pub fn indicator(&self) -> Option<Indicator> {
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
        let ago = age(self.now - self.last_sync_at?);
        let text = if self.is_idle() {
            format!("idle, synced {ago} ago")
        } else {
            format!("synced {ago} ago")
        };
        Some(Indicator { text, alert: false })
    }

    pub fn is_idle(&self) -> bool {
        let timeout = i64::try_from(self.idle_timeout_secs).unwrap_or(i64::MAX);
        self.idle_timeout_secs > 0 && (self.now - self.last_activity).num_seconds() >= timeout
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
fn age(elapsed: chrono::TimeDelta) -> String {
    let seconds = elapsed.num_seconds().max(0);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Projects,
    Tasks,
    Detail,
}

pub struct OverviewStats {
    pub due_today: u32,
    pub due_week: u32,
    pub overdue: u32,
    pub by_priority: [u32; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFilter {
    Active,
    Done,
    Both,
}

impl TaskFilter {
    pub fn next(self) -> Self {
        match self {
            TaskFilter::Active => TaskFilter::Done,
            TaskFilter::Done => TaskFilter::Both,
            TaskFilter::Both => TaskFilter::Active,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockItem {
    DueOverdue,
    DueToday,
    DueWeek,
    Priority(u8),
}

pub const DOCK_ITEMS: [DockItem; 7] = [
    DockItem::DueOverdue,
    DockItem::DueToday,
    DockItem::DueWeek,
    DockItem::Priority(4),
    DockItem::Priority(3),
    DockItem::Priority(2),
    DockItem::Priority(1),
];

impl DockItem {
    pub fn hint(self) -> &'static str {
        match self {
            DockItem::DueOverdue => "overdue",
            DockItem::DueToday => "due today",
            DockItem::DueWeek => "due this week",
            DockItem::Priority(4) => "urgent (P1)",
            DockItem::Priority(3) => "high (P2)",
            DockItem::Priority(2) => "medium (P3)",
            DockItem::Priority(1) => "no priority",
            DockItem::Priority(_) => "by priority",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Default,
    Priority,
    DueDate,
    Created,
}

impl SortMode {
    pub fn label(&self) -> &'static str {
        match self {
            SortMode::Default => "order",
            SortMode::Priority => "priority",
            SortMode::DueDate => "due",
            SortMode::Created => "created",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SortMode::Default => SortMode::Priority,
            SortMode::Priority => SortMode::DueDate,
            SortMode::DueDate => SortMode::Created,
            SortMode::Created => SortMode::Default,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: String,
    pub display: String,
}

impl UserRecord {
    pub fn new(id: String, full_name: Option<String>, email: Option<String>) -> Self {
        let name = full_name.unwrap_or_default();
        let mail = email.unwrap_or_default();
        let display = match (name.is_empty(), mail.is_empty()) {
            (false, false) => format!("{name} - {mail}"),
            (false, true) => name.clone(),
            (true, false) => mail.clone(),
            _ => id.clone(),
        };
        Self { id, display }
    }
}

pub enum ProjectEntry {
    PersonalHeader,
    WorkspaceHeader(usize),
    FolderHeader(usize),
    Project(usize),
    Separator,
}

pub enum ProjectNavItem {
    Folder(usize),
    Project(usize),
}

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, NaiveDate};
use ratatui::layout::Size;

use super::modal::Modal;
use super::sync::OptimisticOp;
use crate::todoist::model::{Comment, Folder, Label, Project, Section, Task, Workspace};
use crate::ui::theme::Theme;
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

    /// True until the first sync answers; the splash shows meanwhile.
    pub loading: bool,
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub folders: Vec<Folder>,
    pub tasks: Vec<Task>,
    pub labels: Vec<Label>,
    pub sections: Vec<Section>,
    pub selected_project: usize,
    pub selected_task: usize,
    pub active_pane: Pane,
    pub error: Option<AppError>,
    pub input_mode: InputMode,
    /// Overlays, drawn bottom to top; the top one takes keys.
    pub modals: Vec<Modal>,
    pub input_buffer: String,
    pub settings_selection: usize,
    pub collapsed: HashSet<String>,
    /// Rows the task detail is scrolled down by. Scroll keys keep it within
    /// the content at [`State::screen_size`].
    pub detail_scroll: u16,
    /// The terminal size: read when the app starts, then from each resize.
    /// `None` only for a `State` no terminal has shown.
    pub screen: Option<Size>,
    pub sort_mode: SortMode,
    pub comments: Vec<Comment>,
    pub detail_field: DetailField,
    pub current_user_id: Option<String>,
    pub user_names: HashMap<String, UserRecord>,
    pub task_filter: TaskFilter,
    pub dock_focus: Option<usize>,
    pub dock_filter: Option<DockItem>,
    pub themes: Vec<Theme>,
    pub theme_idx: usize,
    pub websocket_connected: bool,
    pub sync_token: String,
    pub completed_cache: HashMap<String, Vec<Task>>,
    pub comments_by_task: HashMap<String, Vec<Comment>>,
    pub idle_timeout_secs: u64,
    pub last_sync_at: Option<DateTime<Local>>,
    pub collapsed_folders: HashSet<String>,
    pub folder_cursor: Option<usize>,
    pub current_user_name: Option<String>,
    pub today_view_active: bool,
    pub overdue_section_collapsed: bool,
    /// The first key of the `z` fold chord was pressed.
    pub pending_z: bool,
    pub last_activity: DateTime<Local>,
    pub(crate) pending_ws_sync: bool,
    pub(crate) comments_fetch_seq: u64,
    pub(crate) websocket_url: Option<String>,
    pub(crate) live_opened: bool,
    /// Revert snapshots of optimistic writes, keyed by command uuid.
    pub(crate) temp_id_pending: HashMap<String, OptimisticOp>,
    pub(crate) next_temp_id: u64,
}

impl State {
    pub fn new(now: DateTime<Local>) -> Self {
        Self {
            today: now.date_naive(),
            now,
            loading: true,
            projects: Vec::new(),
            workspaces: Vec::new(),
            folders: Vec::new(),
            tasks: Vec::new(),
            labels: Vec::new(),
            sections: Vec::new(),
            selected_project: 0,
            selected_task: 0,
            active_pane: Pane::Projects,
            error: None,
            input_mode: InputMode::Vim(VimState::Normal),
            modals: Vec::new(),
            input_buffer: String::new(),
            settings_selection: 0,
            collapsed: HashSet::new(),
            detail_scroll: 0,
            screen: None,
            sort_mode: SortMode::Default,
            comments: Vec::new(),
            detail_field: DetailField::Content,
            current_user_id: None,
            user_names: HashMap::new(),
            task_filter: TaskFilter::Active,
            dock_focus: None,
            dock_filter: None,
            themes: Theme::builtin(),
            theme_idx: 0,
            websocket_connected: false,
            sync_token: "*".to_owned(),
            completed_cache: HashMap::new(),
            comments_by_task: HashMap::new(),
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            last_sync_at: None,
            collapsed_folders: HashSet::new(),
            folder_cursor: None,
            current_user_name: None,
            today_view_active: false,
            overdue_section_collapsed: false,
            pending_z: false,
            last_activity: now,
            pending_ws_sync: false,
            comments_fetch_seq: 0,
            websocket_url: None,
            live_opened: false,
            temp_id_pending: HashMap::new(),
            next_temp_id: 0,
        }
    }

    pub fn theme(&self) -> &Theme {
        self.themes
            .get(self.theme_idx)
            .or_else(|| self.themes.first())
            .unwrap_or_else(|| fallback_theme())
    }

    /// Whether the Settings pane shows, which is whenever it has focus.
    pub fn settings_open(&self) -> bool {
        self.active_pane == Pane::Settings
    }

    pub fn top_modal(&self) -> Option<&Modal> {
        self.modals.last()
    }

    pub fn top_modal_mut(&mut self) -> Option<&mut Modal> {
        self.modals.last_mut()
    }

    /// The terminal size, assuming 80x24 when no terminal has reported one.
    pub fn screen_size(&self) -> Size {
        self.screen.unwrap_or(Size::new(80, 24))
    }

    pub fn sync_age_label(&self) -> String {
        match self.last_sync_at {
            Some(at) => at.format("%Y-%m-%d %H:%M").to_string(),
            None => "--".to_string(),
        }
    }

    pub fn is_idle(&self) -> bool {
        let timeout = i64::try_from(self.idle_timeout_secs).unwrap_or(i64::MAX);
        self.idle_timeout_secs > 0 && (self.now - self.last_activity).num_seconds() >= timeout
    }

    /// Whether `theme` names one of the loaded themes; selects it if so.
    pub fn select_theme_named(&mut self, name: &str) -> bool {
        match self.themes.iter().position(|t| t.name == name) {
            Some(idx) => {
                self.theme_idx = idx;
                true
            }
            None => false,
        }
    }
}

fn fallback_theme() -> &'static Theme {
    static FALLBACK: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
    FALLBACK.get_or_init(|| {
        use ratatui::style::Color;
        Theme {
            name: "terminal".to_owned(),
            base: Color::Reset,
            surface: Color::Reset,
            overlay: Color::DarkGray,
            muted: Color::DarkGray,
            subtle: Color::Gray,
            text: Color::Reset,
            bg_alt: Color::Reset,
            fg_alt: Color::White,
            red: Color::Red,
            orange: Color::LightRed,
            yellow: Color::Yellow,
            green: Color::Green,
            cyan: Color::Cyan,
            blue: Color::Blue,
            purple: Color::Magenta,
            maroon: Color::LightMagenta,
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Projects,
    Tasks,
    Detail,
    Settings,
    StatsDock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Standard,
    Vim(VimState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimState {
    Normal,
    Insert,
}

impl InputMode {
    pub fn label(&self) -> &'static str {
        match self {
            InputMode::Standard => "STANDARD",
            InputMode::Vim(VimState::Normal) => "NORMAL",
            InputMode::Vim(VimState::Insert) => "INSERT",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    pub title: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub recoverable: bool,
}

impl AppError {
    pub(crate) fn from_client(err: &crate::todoist::ClientError, context: &str) -> Self {
        let raw = err.to_string();
        let (title, message, suggestion) = parse_api_error(&raw, context);
        Self {
            title,
            message,
            suggestion,
            recoverable: true,
        }
    }
}

fn parse_api_error(raw: &str, context: &str) -> (String, String, Option<String>) {
    if let Some(json) = raw.find('{').and_then(|json_start| raw.get(json_start..))
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json)
    {
        let error_msg = parsed["error"]
            .as_str()
            .unwrap_or("Unknown error")
            .to_string();
        let error_tag = parsed["error_tag"].as_str().unwrap_or("");

        let suggestion = match error_tag {
            "INVALID_DATE_FORMAT" | "BAD_REQUEST" => Some(
                "Try natural language like \"tomorrow\", \"next monday\", or \"Feb 28\""
                    .to_string(),
            ),
            "NOT_FOUND" => Some("The item may have been deleted. Try refreshing.".to_string()),
            "FORBIDDEN" => Some("You don't have permission for this action.".to_string()),
            "UNAUTHORIZED" => {
                Some("Your API token may have expired. Check your config.".to_string())
            }
            _ => None,
        };

        return (format!("{context} failed"), error_msg, suggestion);
    }

    (format!("{context} failed"), raw.to_string(), None)
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
    TodayView,
}

pub enum ProjectNavItem {
    Folder(usize),
    Project(usize),
    TodayView,
}

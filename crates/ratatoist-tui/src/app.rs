use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::Local;

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::api::models::{Comment, Folder, Label, Project, Section, Task, Workspace};
use ratatoist_core::api::sync::{SyncCommand, SyncRequest, SyncResponse};
use ratatoist_core::sync_state::SyncState;

use crate::keys::{self, KeyAction};
use crate::ui;

static CMD_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_uuid() -> String {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let c = CMD_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{ns:08x}-{c:016x}-4000-8000-000000000000")
}

fn new_temp_id() -> String {
    format!("tmp_{}", CMD_COUNTER.fetch_add(1, Ordering::Relaxed))
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
    #[allow(dead_code)] // Reserved for visual mode selection.
    Visual,
    Insert,
}

impl InputMode {
    pub fn label(&self) -> &'static str {
        match self {
            InputMode::Standard => "STANDARD",
            InputMode::Vim(VimState::Normal) => "NORMAL",
            InputMode::Vim(VimState::Visual) => "VISUAL",
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

#[derive(Debug, Clone)]
pub struct AppError {
    pub title: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub recoverable: bool,
}

impl AppError {
    fn from_api(err: &anyhow::Error, context: &str) -> Self {
        let raw = format!("{err:#}");
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
    if let Some(json_start) = raw.find('{')
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw[json_start..])
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
#[allow(dead_code)]
pub struct UserRecord {
    pub id: String,
    pub full_name: String,
    pub email: String,
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
        Self {
            id,
            full_name: name,
            email: mail,
            display,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskForm {
    pub content: String,
    pub priority: u8,
    pub due_string: String,
    pub project_id: String,
    pub active_field: usize,
    pub editing: bool,
}

impl TaskForm {
    pub fn new(project_id: String) -> Self {
        Self {
            content: String::new(),
            priority: 1,
            due_string: String::new(),
            project_id,
            active_field: 0,
            editing: true,
        }
    }

    pub fn field_count() -> usize {
        4
    }
}

// Tracks what was in local state before an optimistic mutation so we can
// revert if the server rejects the command.
pub enum OptimisticOp {
    TaskAdded {
        temp_id: String,
    },
    #[allow(dead_code)] // Used once delete task (d) is wired up.
    TaskRemoved {
        snapshot: Task,
    },
    TaskUpdated {
        task_id: String,
        before: Task,
    },
    CommentAdded {
        temp_id: String,
        task_id: String,
    },
    ProjectUpdated {
        project_id: String,
        before: Project,
    },
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

enum BgResult {
    SyncDelta(Box<SyncResponse>),
    CommandResults(Box<SyncResponse>),
    CompletedTasks {
        project_id: String,
        records: Result<Vec<Task>>,
    },
    WebSocketConnected,
    WebSocketEvent,
    WebSocketDisconnected,
    Comments {
        task_id: String,
        comments: Result<Vec<Comment>>,
        fetch_seq: u64,
    },
}

pub struct App {
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub folders: Vec<Folder>,
    pub tasks: Vec<Task>,
    pub labels: Vec<Label>,
    pub sections: Vec<Section>,
    pub selected_project: usize,
    pub selected_task: usize,
    pub active_pane: Pane,
    pub running: bool,
    pub error: Option<AppError>,
    pub input_mode: InputMode,
    pub show_settings: bool,
    pub show_help: bool,
    pub show_input: bool,
    pub input_buffer: String,
    pub settings_selection: usize,
    pub collapsed: HashSet<String>,
    pub detail_scroll: u16,
    pub sort_mode: SortMode,
    pub comments: Vec<Comment>,
    pub comment_input: bool,
    pub detail_field: usize,
    pub show_priority_picker: bool,
    pub priority_selection: u8,
    pub editing_field: bool,
    pub task_form: Option<TaskForm>,
    pub current_user_id: Option<String>,
    pub user_names: HashMap<String, UserRecord>,
    pub task_filter: TaskFilter,
    pub dock_focus: Option<usize>,
    pub dock_filter: Option<DockItem>,
    pub themes: Vec<crate::ui::theme::Theme>,
    pub theme_idx: usize,
    pub show_theme_picker: bool,
    pub theme_selection: usize,
    pub websocket_connected: bool,
    pub sync_token: String,
    pub completed_cache: HashMap<String, Vec<Task>>,
    pub comments_by_task: HashMap<String, Vec<Comment>>,
    pub idle_timeout_secs: u64,
    pub idle_forcer: bool,
    pub ephemeral: bool,
    pub last_sync_at: Option<chrono::DateTime<Local>>,
    pub collapsed_folders: HashSet<String>,
    pub folder_cursor: Option<usize>,
    pub current_user_name: Option<String>,
    last_activity: Instant,
    pending_ws_sync: bool,
    comments_fetch_seq: u64,
    websocket_url: Option<String>,
    pending_commands: Vec<SyncCommand>,
    temp_id_pending: HashMap<String, OptimisticOp>,
    bg_tx: mpsc::Sender<BgResult>,
    bg_rx: mpsc::Receiver<BgResult>,
    client: Arc<TodoistClient>,
}

fn load_theme_idx(themes: &[crate::ui::theme::Theme]) -> usize {
    let path = ratatoist_core::config::Config::config_dir().join("ui_settings.json");
    if let Ok(src) = std::fs::read_to_string(&path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&src)
        && let Some(name) = val["theme"].as_str()
        && let Some(idx) = themes.iter().position(|t| t.name == name)
    {
        return idx;
    }
    0
}

fn load_idle_timeout_secs() -> u64 {
    let path = ratatoist_core::config::Config::config_dir().join("ui_settings.json");
    if let Ok(src) = std::fs::read_to_string(&path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&src)
    {
        if let Some(secs) = val["idle_timeout_secs"].as_u64() {
            return secs;
        }
        if let Some(mins) = val["idle_timeout_mins"].as_u64() {
            return mins * 60;
        }
    }
    300
}

impl App {
    pub fn theme(&self) -> &crate::ui::theme::Theme {
        &self.themes[self.theme_idx]
    }

    pub fn cycle_task_filter(&mut self) {
        self.task_filter = self.task_filter.next();
        if matches!(self.task_filter, TaskFilter::Done | TaskFilter::Both) {
            if let Some(pid) = self
                .projects
                .get(self.selected_project)
                .map(|p| p.id.clone())
            {
                if !self.completed_cache.contains_key(&pid) {
                    self.spawn_completed_tasks_fetch(pid);
                }
            }
        }
        let visible_len = self.visible_tasks().len();
        if visible_len == 0 {
            self.selected_task = 0;
        } else if self.selected_task >= visible_len {
            self.selected_task = visible_len - 1;
        }
    }

    pub fn sync_age_label(&self) -> String {
        match self.last_sync_at {
            Some(at) => at.format("%Y-%m-%d %H:%M").to_string(),
            None => "--".to_string(),
        }
    }

    pub fn is_idle(&self) -> bool {
        self.idle_timeout_secs > 0
            && self.last_activity.elapsed() >= Duration::from_secs(self.idle_timeout_secs)
    }

    pub fn cycle_idle_timeout(&mut self) {
        const OPTIONS: &[u64] = &[60, 120, 300, 600, 900, 1800];
        const DEBUG_OPTIONS: &[u64] = &[5, 60, 120, 300, 600, 900, 1800];
        let options = if self.idle_forcer {
            DEBUG_OPTIONS
        } else {
            OPTIONS
        };
        let pos = options
            .iter()
            .position(|&v| v == self.idle_timeout_secs)
            .unwrap_or(2);
        self.idle_timeout_secs = options[(pos + 1) % options.len()];
        self.save_ui_settings();
    }

    pub fn save_ui_settings(&self) {
        if self.ephemeral {
            return;
        }
        let dir = ratatoist_core::config::Config::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("ui_settings.json");
        let name = &self.themes[self.theme_idx].name;
        let json = serde_json::json!({
            "theme": name,
            "idle_timeout_secs": self.idle_timeout_secs,
        });
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        );
    }

    pub fn new(client: TodoistClient, idle_forcer: bool, ephemeral: bool) -> Self {
        let (bg_tx, bg_rx) = mpsc::channel(64);
        let mut themes = crate::ui::theme::Theme::builtin();
        let user_themes_dir = ratatoist_core::config::Config::config_dir().join("themes");
        themes.extend(crate::ui::theme::Theme::load_user_themes(&user_themes_dir));
        let theme_idx = load_theme_idx(&themes);
        let config_dir = ratatoist_core::config::Config::config_dir();
        let sync_token = if ephemeral {
            "*".to_string()
        } else {
            SyncState::load(&config_dir).sync_token
        };
        let idle_timeout_secs = load_idle_timeout_secs();

        Self {
            projects: Vec::new(),
            workspaces: Vec::new(),
            folders: Vec::new(),
            tasks: Vec::new(),
            labels: Vec::new(),
            sections: Vec::new(),
            selected_project: 0,
            selected_task: 0,
            active_pane: Pane::Projects,
            running: true,
            error: None,
            input_mode: InputMode::Vim(VimState::Normal),
            show_settings: false,
            show_help: false,
            show_input: false,
            input_buffer: String::new(),
            settings_selection: 0,
            collapsed: HashSet::new(),
            detail_scroll: 0,
            sort_mode: SortMode::Default,
            comments: Vec::new(),
            comment_input: false,
            detail_field: 0,
            show_priority_picker: false,
            priority_selection: 1,
            editing_field: false,
            task_form: None,
            task_filter: TaskFilter::Active,
            dock_focus: None,
            dock_filter: None,
            current_user_id: None,
            user_names: HashMap::new(),
            themes,
            theme_idx,
            show_theme_picker: false,
            theme_selection: theme_idx,
            websocket_connected: false,
            sync_token,
            completed_cache: HashMap::new(),
            comments_by_task: HashMap::new(),
            idle_timeout_secs,
            idle_forcer,
            ephemeral,
            last_sync_at: None,
            collapsed_folders: HashSet::new(),
            folder_cursor: None,
            current_user_name: None,
            last_activity: Instant::now(),
            pending_ws_sync: false,
            comments_fetch_seq: 0,
            websocket_url: None,
            pending_commands: Vec::new(),
            temp_id_pending: HashMap::new(),
            bg_tx,
            bg_rx,
            client: Arc::new(client),
        }
    }

    pub async fn load_with_splash(&mut self, terminal: &mut DefaultTerminal) {
        info!(sync_token = %self.sync_token, "full sync starting");

        terminal
            .draw(|f| ui::splash::render(f, 0.0, "connecting to todoist...", self.theme()))
            .ok();

        let req = SyncRequest {
            sync_token: "*".to_string(),
            resource_types: vec![
                "items".to_string(),
                "projects".to_string(),
                "sections".to_string(),
                "labels".to_string(),
                "notes".to_string(),
                "collaborators".to_string(),
                "workspaces".to_string(),
                "folders".to_string(),
                "user".to_string(),
            ],
            commands: vec![],
        };

        terminal
            .draw(|f| ui::splash::render(f, 0.3, "syncing data...", self.theme()))
            .ok();

        match self.client.sync(&req).await {
            Ok(resp) => {
                terminal
                    .draw(|f| ui::splash::render(f, 0.8, "applying sync...", self.theme()))
                    .ok();
                self.apply_sync_delta(resp);

                terminal
                    .draw(|f| ui::splash::render(f, 1.0, "ready", self.theme()))
                    .ok();

                info!(
                    projects = self.projects.len(),
                    tasks = self.tasks.len(),
                    labels = self.labels.len(),
                    users = self.user_names.len(),
                    "full sync complete"
                );

                if let Some(url) = self.websocket_url.clone() {
                    self.spawn_websocket(url);
                }
            }
            Err(e) => {
                self.set_error(&e, "Initial sync");
            }
        }
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        info!("entering main loop");

        while self.running {
            self.drain_bg_results();

            terminal.draw(|frame| ui::draw(frame, self))?;

            if event::poll(Duration::from_millis(16))?
                && let Event::Key(key) = event::read()?
            {
                let was_idle = self.is_idle();
                self.last_activity = Instant::now();
                if was_idle && self.pending_ws_sync {
                    self.pending_ws_sync = false;
                    self.spawn_incremental_sync();
                }

                if self.error.is_some() {
                    self.handle_error_dismiss();
                    continue;
                }

                let prev_pane = self.active_pane;
                match keys::handle_key(self, key) {
                    KeyAction::Quit => {
                        info!("quit requested");
                        self.running = false;
                    }
                    KeyAction::ProjectChanged => self.switch_to_project_tasks(),
                    KeyAction::OpenDetail => self.open_detail(),
                    KeyAction::CloseDetail => {
                        self.active_pane = Pane::Tasks;
                        self.detail_scroll = 0;
                    }
                    KeyAction::ToggleSettings => {
                        self.show_settings = !self.show_settings;
                        self.active_pane = if self.show_settings {
                            Pane::Settings
                        } else {
                            Pane::Projects
                        };
                    }
                    KeyAction::ToggleHelp => self.show_help = !self.show_help,
                    KeyAction::ToggleMode => self.toggle_input_mode(),
                    KeyAction::ToggleCollapse => self.toggle_collapse(),
                    KeyAction::ToggleFolderCollapse => self.toggle_folder_collapse(),
                    KeyAction::OpenAllFolds => self.collapsed.clear(),
                    KeyAction::CloseAllFolds => self.close_all_folds(),
                    KeyAction::CompleteTask => self.complete_selected_task(),
                    KeyAction::OpenPriorityPicker => {
                        if let Some(task) = self.selected_task() {
                            self.priority_selection = task.priority;
                            self.show_priority_picker = true;
                        }
                    }
                    KeyAction::SelectPriority => {
                        self.show_priority_picker = false;
                        if let Some(form) = &mut self.task_form {
                            form.priority = self.priority_selection;
                        } else {
                            self.apply_priority(self.priority_selection);
                        }
                    }
                    KeyAction::StarProject => self.star_selected_project(),
                    KeyAction::CycleFilter => self.cycle_task_filter(),
                    KeyAction::CycleSort => {
                        self.sort_mode = self.sort_mode.next();
                        info!(sort = self.sort_mode.label(), "sort mode changed");
                    }
                    KeyAction::StartInput => self.start_input(),
                    KeyAction::StartCommentInput => self.start_comment_input(),
                    KeyAction::StartFieldEdit => self.start_field_edit(),
                    KeyAction::SubmitInput => self.submit_input(),
                    KeyAction::SubmitForm => self.submit_task_form(),
                    KeyAction::FormFieldUp => self.form_field_up(),
                    KeyAction::FormFieldDown => self.form_field_down(),
                    KeyAction::FormEditField => self.form_edit_field(),
                    KeyAction::FormEscNormal => {
                        self.submit_input();
                    }
                    KeyAction::CancelInput => self.cancel_input(),
                    KeyAction::DetailFieldUp => self.move_detail_field(-1),
                    KeyAction::DetailFieldDown => self.move_detail_field(1),
                    KeyAction::OpenThemePicker => {
                        self.theme_selection = self.theme_idx;
                        self.show_theme_picker = true;
                    }
                    KeyAction::SelectTheme => {
                        self.theme_idx = self.theme_selection;
                        self.show_theme_picker = false;
                        self.save_ui_settings();
                    }
                    KeyAction::CloseThemePicker => {
                        self.show_theme_picker = false;
                    }
                    KeyAction::Consumed | KeyAction::None => {}
                }
                if matches!(prev_pane, Pane::Tasks) && !matches!(self.active_pane, Pane::Tasks) {
                    self.dock_filter = None;
                }
            }
        }

        info!("exiting main loop");
        Ok(())
    }

    fn apply_sync_delta(&mut self, resp: SyncResponse) {
        if resp.full_sync {
            if let Some(projects) = resp.projects {
                self.projects = projects
                    .into_iter()
                    .filter(|p| !p.is_deleted.unwrap_or(false))
                    .collect();
                self.sort_projects();
            }
            if let Some(items) = resp.items {
                self.tasks = items.into_iter().filter(|t| !t.is_deleted).collect();
            }
            if let Some(labels) = resp.labels {
                self.labels = labels
                    .into_iter()
                    .filter(|l| !l.is_deleted.unwrap_or(false))
                    .collect();
            }
            if let Some(sections) = resp.sections {
                self.sections = sections
                    .into_iter()
                    .filter(|s| !s.is_deleted.unwrap_or(false))
                    .collect();
            }
            if let Some(notes) = resp.notes {
                self.comments_by_task.clear();
                for note in notes {
                    if !note.is_deleted {
                        let tid = note
                            .item_id
                            .clone()
                            .or_else(|| note.task_id.clone())
                            .unwrap_or_default();
                        self.comments_by_task.entry(tid).or_default().push(note);
                    }
                }
            }
            if let Some(collabs) = resp.collaborators {
                for c in collabs {
                    self.user_names
                        .entry(c.id.clone())
                        .or_insert_with(|| UserRecord::new(c.id, c.name, c.email));
                }
            }
            if let Some(workspaces) = resp.workspaces {
                self.workspaces = workspaces.into_iter().filter(|w| !w.is_deleted).collect();
            }
            if let Some(folders) = resp.folders {
                self.folders = folders.into_iter().filter(|f| !f.is_deleted).collect();
            }
            if let Some(user) = resp.user {
                self.current_user_id = Some(user.id.clone());
                self.websocket_url = user.websocket_url;
                if let Some(name) = &user.full_name {
                    self.current_user_name = Some(name.clone());
                }
                self.user_names
                    .entry(user.id.clone())
                    .or_insert_with(|| UserRecord::new(user.id, user.full_name, user.email));
            }
        } else {
            if let Some(projects) = resp.projects {
                for p in projects {
                    if p.is_deleted.unwrap_or(false) {
                        self.projects.retain(|e| e.id != p.id);
                    } else if let Some(e) = self.projects.iter_mut().find(|e| e.id == p.id) {
                        *e = p;
                    } else {
                        self.projects.push(p);
                    }
                }
                self.sort_projects();
            }
            if let Some(items) = resp.items {
                for item in items {
                    if item.is_deleted {
                        self.tasks.retain(|t| t.id != item.id);
                    } else if let Some(e) = self.tasks.iter_mut().find(|t| t.id == item.id) {
                        *e = item;
                    } else {
                        self.tasks.push(item);
                    }
                }
            }
            if let Some(labels) = resp.labels {
                for l in labels {
                    if l.is_deleted.unwrap_or(false) {
                        self.labels.retain(|e| e.id != l.id);
                    } else if let Some(e) = self.labels.iter_mut().find(|e| e.id == l.id) {
                        *e = l;
                    } else {
                        self.labels.push(l);
                    }
                }
            }
            if let Some(sections) = resp.sections {
                for s in sections {
                    if s.is_deleted.unwrap_or(false) {
                        self.sections.retain(|e| e.id != s.id);
                    } else if let Some(e) = self.sections.iter_mut().find(|e| e.id == s.id) {
                        *e = s;
                    } else {
                        self.sections.push(s);
                    }
                }
            }
            if let Some(notes) = resp.notes {
                let open_task_id = self.selected_task().map(|t| t.id.clone());
                let mut affected_task: Option<String> = None;
                for note in notes {
                    let tid = note
                        .item_id
                        .clone()
                        .or_else(|| note.task_id.clone())
                        .unwrap_or_default();
                    if note.is_deleted {
                        if let Some(list) = self.comments_by_task.get_mut(&tid) {
                            list.retain(|c| c.id != note.id);
                        }
                    } else if let Some(list) = self.comments_by_task.get_mut(&tid) {
                        if let Some(c) = list.iter_mut().find(|c| c.id == note.id) {
                            *c = note;
                        } else {
                            list.push(note);
                        }
                    } else {
                        self.comments_by_task.insert(tid.clone(), vec![note]);
                    }
                    if open_task_id.as_deref() == Some(&tid) {
                        affected_task = Some(tid);
                    }
                }
                if let Some(tid) = affected_task {
                    if let Some(updated) = self.comments_by_task.get(&tid) {
                        self.comments = updated.clone();
                    }
                }
            }
        }

        if !resp.sync_token.is_empty() {
            self.sync_token = resp.sync_token;
            self.save_sync_token();
        }
        self.last_sync_at = Some(Local::now());

        // Keep selection in bounds after any sync.
        let visible_len = self.visible_tasks().len();
        if visible_len == 0 {
            self.selected_task = 0;
        } else if self.selected_task >= visible_len {
            self.selected_task = visible_len - 1;
        }
    }

    fn flush_commands(&mut self) {
        if self.pending_commands.is_empty() {
            return;
        }

        let commands = std::mem::take(&mut self.pending_commands);
        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let sync_token = self.sync_token.clone();

        tokio::spawn(async move {
            let req = SyncRequest {
                sync_token,
                resource_types: vec![],
                commands,
            };
            let result = client.sync(&req).await;
            match result {
                Ok(resp) => {
                    let _ = tx.send(BgResult::CommandResults(Box::new(resp))).await;
                }
                Err(e) => {
                    error!(error = %e, "command flush failed");
                    // Commands stay lost on network failure; next WS-triggered
                    // sync will correct server state.
                }
            }
        });
    }

    fn apply_temp_id_mapping(&mut self, temp_id: &str, real_id: &str) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == temp_id) {
            t.id = real_id.to_string();
        }
        for c in &mut self.comments {
            if c.id == temp_id {
                c.id = real_id.to_string();
            }
            if c.item_id.as_deref() == Some(temp_id) {
                c.item_id = Some(real_id.to_string());
            }
        }
    }

    fn revert_optimistic(&mut self, op: OptimisticOp) {
        match op {
            OptimisticOp::TaskAdded { temp_id } => {
                self.tasks.retain(|t| t.id != temp_id);
            }
            OptimisticOp::TaskRemoved { snapshot } => {
                self.tasks.push(snapshot);
            }
            OptimisticOp::TaskUpdated { task_id, before } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    *t = before;
                }
            }
            OptimisticOp::CommentAdded { temp_id, task_id } => {
                let current = self.selected_task().map(|t| t.id.clone());
                if current.as_deref() == Some(&task_id) {
                    self.comments.retain(|c| c.id != temp_id);
                }
            }
            OptimisticOp::ProjectUpdated { project_id, before } => {
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
                    *p = before;
                }
                self.sort_projects();
            }
        }
    }

    fn save_sync_token(&self) {
        if self.ephemeral {
            return;
        }
        let config_dir = ratatoist_core::config::Config::config_dir();
        let state = SyncState {
            sync_token: self.sync_token.clone(),
        };
        if let Err(e) = state.save(&config_dir) {
            warn!(error = %e, "failed to persist sync token");
        }
    }

    fn spawn_websocket(&self, url: String) {
        let tx = self.bg_tx.clone();
        tokio::spawn(run_websocket(url, tx));
    }

    fn spawn_incremental_sync(&self) {
        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let sync_token = self.sync_token.clone();

        tokio::spawn(async move {
            let req = SyncRequest {
                sync_token,
                resource_types: vec![
                    "items".to_string(),
                    "projects".to_string(),
                    "sections".to_string(),
                    "labels".to_string(),
                    "notes".to_string(),
                ],
                commands: vec![],
            };
            match client.sync(&req).await {
                Ok(resp) => {
                    let _ = tx.send(BgResult::SyncDelta(Box::new(resp))).await;
                }
                Err(e) => {
                    error!(error = %e, "incremental sync failed");
                }
            }
        });
    }

    fn drain_bg_results(&mut self) {
        while let Ok(result) = self.bg_rx.try_recv() {
            match result {
                BgResult::SyncDelta(resp) => {
                    self.apply_sync_delta(*resp);
                }

                BgResult::CommandResults(resp) => {
                    let mut refresh_comments_for: Option<String> = None;
                    for (uuid, status) in &resp.sync_status {
                        if status.is_err() {
                            if let Some(op) = self.temp_id_pending.remove(uuid) {
                                self.revert_optimistic(op);
                            }
                            let msg = status
                                .error_message()
                                .unwrap_or("unknown error")
                                .to_string();
                            error!(uuid, error = %msg, "command rejected by server");
                            self.error = Some(AppError {
                                title: "Command failed".to_string(),
                                message: msg,
                                suggestion: None,
                                recoverable: true,
                            });
                        } else if let Some(op) = self.temp_id_pending.remove(uuid)
                            && let OptimisticOp::CommentAdded { task_id, .. } = &op
                        {
                            let current = self.selected_task().map(|t| t.id.clone());
                            if current.as_deref() == Some(task_id.as_str()) {
                                refresh_comments_for = Some(task_id.clone());
                            }
                        }
                    }
                    for (temp_id, real_id) in &resp.temp_id_mapping {
                        self.apply_temp_id_mapping(temp_id, real_id);
                    }
                    if !resp.sync_token.is_empty() {
                        self.sync_token = resp.sync_token.clone();
                        self.save_sync_token();
                    }
                    if let Some(tid) = refresh_comments_for {
                        self.spawn_comments_fetch(tid);
                    }
                }

                BgResult::CompletedTasks {
                    project_id,
                    records,
                } => match records {
                    Ok(r) => {
                        self.completed_cache.insert(project_id, r);
                    }
                    Err(e) => self.set_error(&e, "Load completed tasks"),
                },

                BgResult::WebSocketConnected => {
                    debug!("websocket connected");
                    self.websocket_connected = true;
                }
                BgResult::WebSocketEvent => {
                    self.websocket_connected = true;
                    if self.is_idle() {
                        self.pending_ws_sync = true;
                    } else {
                        self.spawn_incremental_sync();
                    }
                }
                BgResult::WebSocketDisconnected => {
                    debug!("websocket disconnected");
                    self.websocket_connected = false;
                }

                BgResult::Comments {
                    task_id,
                    comments,
                    fetch_seq,
                } => match comments {
                    Ok(c) => {
                        let count = c.len() as i32;
                        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            t.note_count = Some(count);
                        }
                        self.comments_by_task.insert(task_id.clone(), c.clone());
                        let current_tid = self.selected_task().map(|t| t.id.clone());
                        if current_tid.as_deref() == Some(&task_id)
                            && fetch_seq == self.comments_fetch_seq
                        {
                            self.comments = c;
                        }
                    }
                    Err(e) => self.set_error(&e, "Load comments"),
                },
            }
        }
    }

    fn open_detail(&mut self) {
        let visible = self.visible_tasks();
        if let Some(task) = visible.get(self.selected_task) {
            let task_id = task.id.clone();
            let task_project_id = task.project_id.clone();

            if self.dock_filter.is_some()
                && let Some(pos) = self.projects.iter().position(|p| p.id == task_project_id)
            {
                self.selected_project = pos;
            }

            self.active_pane = Pane::Detail;
            self.detail_scroll = 0;
            self.detail_field = 0;

            // Serve cached comments immediately, refresh in background.
            if let Some(cached) = self.comments_by_task.get(&task_id) {
                self.comments = cached.clone();
            } else {
                self.comments.clear();
            }
            self.spawn_comments_fetch(task_id);
        }
    }

    fn spawn_comments_fetch(&mut self, task_id: String) {
        self.comments_fetch_seq += 1;
        let fetch_seq = self.comments_fetch_seq;
        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let tid = task_id.clone();

        tokio::spawn(async move {
            let comments = client.get_comments(&tid).await;
            let _ = tx
                .send(BgResult::Comments {
                    task_id: tid,
                    comments,
                    fetch_seq,
                })
                .await;
        });
    }

    fn spawn_completed_tasks_fetch(&self, project_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let pid = project_id.clone();

        tokio::spawn(async move {
            let records = client.get_completed_tasks(Some(&pid), None).await;
            let _ = tx
                .send(BgResult::CompletedTasks {
                    project_id: pid,
                    records,
                })
                .await;
        });
    }

    fn switch_to_project_tasks(&mut self) {
        self.selected_task = 0;
        self.detail_scroll = 0;
    }

    fn complete_selected_task(&mut self) {
        let (task_id, was_checked, is_recurring) = {
            let visible = self.visible_tasks();
            let Some(task) = visible.get(self.selected_task) else {
                return;
            };
            (
                task.id.clone(),
                task.checked,
                task.due.as_ref().map(|d| d.is_recurring).unwrap_or(false),
            )
        };

        let before = self.tasks.iter().find(|t| t.id == task_id).cloned();
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.checked = !was_checked;
        }

        let new_len = self.visible_tasks().len();
        if new_len > 0 && self.selected_task >= new_len {
            self.selected_task = new_len - 1;
        }

        let cmd_type = if was_checked {
            "item_reopen"
        } else if is_recurring {
            // item_complete advances the series; item_close would end it.
            "item_complete"
        } else {
            "item_close"
        };

        let uuid = new_uuid();
        self.pending_commands.push(SyncCommand {
            r#type: cmd_type.to_string(),
            temp_id: None,
            uuid: uuid.clone(),
            args: serde_json::json!({ "id": task_id }),
        });

        if let Some(snapshot) = before {
            self.temp_id_pending.insert(
                uuid,
                OptimisticOp::TaskUpdated {
                    task_id,
                    before: snapshot,
                },
            );
        }

        self.flush_commands();
    }

    fn start_input(&mut self) {
        let project_id = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.task_form = Some(TaskForm::new(project_id));
        self.show_input = true;
        self.input_buffer.clear();
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Insert);
        }
    }

    fn submit_input(&mut self) {
        let content = self.input_buffer.trim().to_string();

        if self.comment_input {
            if !content.is_empty() {
                self.submit_comment(content);
            }
            self.cancel_input();
            return;
        }

        if self.editing_field {
            if !content.is_empty() {
                self.submit_field_edit(content);
            }
            self.cancel_input();
            return;
        }

        if let Some(form) = &self.task_form
            && form.editing
        {
            let field = form.active_field;
            let Some(mut form) = self.task_form.take() else {
                return;
            };
            match field {
                0 => {
                    // Content goes verbatim; the API parses any inline
                    // natural-language dates or priorities.
                    form.content = content;
                }
                2 => form.due_string = content,
                _ => {}
            }
            form.editing = false;
            self.task_form = Some(form);
            self.input_buffer.clear();
            self.show_input = false;
            if let InputMode::Vim(_) = self.input_mode {
                self.input_mode = InputMode::Vim(VimState::Normal);
            }
            return;
        }

        self.cancel_input();
    }

    pub fn submit_task_form(&mut self) {
        let Some(form) = self.task_form.take() else {
            return;
        };

        if form.content.trim().is_empty() {
            self.cancel_input();
            return;
        }

        let project_id = form.project_id.clone();

        let temp_id = new_temp_id();
        let uuid = new_uuid();

        let optimistic = Task {
            id: temp_id.clone(),
            content: form.content.clone(),
            project_id: project_id.clone(),
            priority: form.priority,
            ..Task::default()
        };
        self.tasks.push(optimistic);
        self.temp_id_pending.insert(
            uuid.clone(),
            OptimisticOp::TaskAdded {
                temp_id: temp_id.clone(),
            },
        );

        let mut args = serde_json::json!({
            "content": form.content,
            "project_id": project_id,
        });
        if !form.due_string.is_empty() {
            args["due_string"] = serde_json::Value::String(form.due_string);
        }
        if form.priority > 1 {
            args["priority"] = serde_json::Value::Number(serde_json::Number::from(form.priority));
        }

        self.pending_commands.push(SyncCommand {
            r#type: "item_add".to_string(),
            temp_id: Some(temp_id),
            uuid,
            args,
        });

        self.flush_commands();

        self.task_form = None;
        self.show_input = false;
        self.input_buffer.clear();
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Normal);
        }
    }

    fn submit_comment(&mut self, content: String) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id.clone();

        let temp_id = new_temp_id();
        let uuid = new_uuid();

        let now = chrono::Utc::now().to_rfc3339();
        let optimistic = Comment {
            id: temp_id.clone(),
            content: content.clone(),
            posted_at: Some(now),
            posted_by_uid: self.current_user_id.clone(),
            task_id: Some(task_id.clone()),
            item_id: Some(task_id.clone()),
            ..Comment::default()
        };
        self.comments.push(optimistic);
        self.comments_fetch_seq += 1;

        self.temp_id_pending.insert(
            uuid.clone(),
            OptimisticOp::CommentAdded {
                temp_id: temp_id.clone(),
                task_id: task_id.clone(),
            },
        );
        self.pending_commands.push(SyncCommand {
            r#type: "note_add".to_string(),
            temp_id: Some(temp_id),
            uuid,
            args: serde_json::json!({ "item_id": task_id, "content": content }),
        });
        self.flush_commands();
    }

    fn submit_field_edit(&mut self, value: String) {
        let (task_id, before) = {
            let Some(task) = self.selected_task() else {
                return;
            };
            (task.id.clone(), task.clone())
        };

        let uuid = new_uuid();
        let args = match self.detail_field {
            0 => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.content = value.clone();
                }
                serde_json::json!({ "id": task_id, "content": value })
            }
            2 => {
                // Due string: server parses and returns the Due object — no
                // optimistic update possible here.
                serde_json::json!({ "id": task_id, "due_string": value })
            }
            3 => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.description = value.clone();
                }
                serde_json::json!({ "id": task_id, "description": value })
            }
            _ => return,
        };

        self.temp_id_pending.insert(
            uuid.clone(),
            OptimisticOp::TaskUpdated {
                task_id: task_id.clone(),
                before,
            },
        );
        self.pending_commands.push(SyncCommand {
            r#type: "item_update".to_string(),
            temp_id: None,
            uuid,
            args,
        });
        self.flush_commands();
    }

    pub fn form_field_up(&mut self) {
        if let Some(form) = &mut self.task_form
            && !form.editing
        {
            let count = TaskForm::field_count();
            form.active_field = if form.active_field == 0 {
                count - 1
            } else {
                form.active_field - 1
            };
        }
    }

    pub fn form_field_down(&mut self) {
        if let Some(form) = &mut self.task_form
            && !form.editing
        {
            form.active_field = (form.active_field + 1) % TaskForm::field_count();
        }
    }

    pub fn form_edit_field(&mut self) {
        if let Some(form) = &mut self.task_form {
            match form.active_field {
                0 => {
                    self.input_buffer = form.content.clone();
                    form.editing = true;
                    self.show_input = true;
                    if let InputMode::Vim(_) = self.input_mode {
                        self.input_mode = InputMode::Vim(VimState::Insert);
                    }
                }
                1 => {
                    self.priority_selection = form.priority;
                    self.show_priority_picker = true;
                }
                2 => {
                    self.input_buffer = form.due_string.clone();
                    form.editing = true;
                    self.show_input = true;
                    if let InputMode::Vim(_) = self.input_mode {
                        self.input_mode = InputMode::Vim(VimState::Insert);
                    }
                }
                3 => {
                    let cur = self
                        .projects
                        .iter()
                        .position(|p| p.id == form.project_id)
                        .unwrap_or(0);
                    let next = (cur + 1) % self.projects.len().max(1);
                    if let Some(p) = self.projects.get(next) {
                        form.project_id = p.id.clone();
                    }
                }
                _ => {}
            }
        }
    }

    fn cancel_input(&mut self) {
        self.show_input = false;
        self.comment_input = false;
        self.editing_field = false;
        self.task_form = None;
        self.input_buffer.clear();
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Normal);
        }
    }

    fn star_selected_project(&mut self) {
        let Some(project) = self.projects.get(self.selected_project) else {
            return;
        };
        let pid = project.id.clone();
        let before = project.clone();
        let new_fav = !project.is_favorite;

        if let Some(p) = self.projects.iter_mut().find(|p| p.id == pid) {
            p.is_favorite = new_fav;
        }
        self.sort_projects();

        let uuid = new_uuid();
        self.temp_id_pending.insert(
            uuid.clone(),
            OptimisticOp::ProjectUpdated {
                project_id: pid.clone(),
                before,
            },
        );
        self.pending_commands.push(SyncCommand {
            r#type: "project_update".to_string(),
            temp_id: None,
            uuid,
            args: serde_json::json!({ "id": pid, "is_favorite": new_fav }),
        });
        self.flush_commands();
    }

    fn sort_projects(&mut self) {
        let selected_id = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.clone());
        let source = self.projects.clone();
        let mut ordered: Vec<Project> = Vec::with_capacity(source.len());

        let personal: Vec<Project> = source
            .iter()
            .filter(|p| p.workspace_id.is_none())
            .cloned()
            .collect();
        collect_project_subtree(None, &personal, &mut ordered);

        let workspaces = self.workspaces.clone();
        for ws in &workspaces {
            let ws_projects: Vec<Project> = source
                .iter()
                .filter(|p| p.workspace_id.as_deref() == Some(ws.id.as_str()))
                .cloned()
                .collect();
            if ws_projects.is_empty() {
                continue;
            }

            let no_folder: Vec<Project> = ws_projects
                .iter()
                .filter(|p| p.folder_id.is_none())
                .cloned()
                .collect();
            collect_project_subtree(None, &no_folder, &mut ordered);

            let mut ws_folders: Vec<&Folder> = self
                .folders
                .iter()
                .filter(|f| f.workspace_id == ws.id)
                .collect();
            ws_folders.sort_by_key(|f| f.child_order);

            for folder in ws_folders {
                let in_folder: Vec<Project> = ws_projects
                    .iter()
                    .filter(|p| p.folder_id.as_deref() == Some(folder.id.as_str()))
                    .cloned()
                    .collect();
                collect_project_subtree(None, &in_folder, &mut ordered);
            }
        }

        let ordered_ids: HashSet<String> = ordered.iter().map(|p| p.id.clone()).collect();
        for p in &source {
            if !ordered_ids.contains(&p.id) {
                ordered.push(p.clone());
            }
        }

        self.projects = ordered;
        if let Some(id) = selected_id
            && let Some(pos) = self.projects.iter().position(|p| p.id == id)
        {
            self.selected_project = pos;
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
                        .position(|w| w.id.as_str() == ws_id.unwrap())
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
        loop {
            let Some(parent_id) = self
                .projects
                .iter()
                .find(|p| p.id == current)
                .and_then(|p| p.parent_id.as_deref())
            else {
                break;
            };
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

    pub fn toggle_folder_collapse(&mut self) {
        let fid = if let Some(fi) = self.folder_cursor {
            self.folders.get(fi).map(|f| f.id.clone())
        } else {
            self.projects
                .get(self.selected_project)
                .and_then(|p| p.folder_id.clone())
        };
        let Some(fid) = fid else {
            return;
        };
        if self.collapsed_folders.contains(&fid) {
            self.collapsed_folders.remove(&fid);
        } else {
            self.collapsed_folders.insert(fid.clone());
        }
        if let Some(fi) = self.folders.iter().position(|f| f.id == fid) {
            self.folder_cursor = Some(fi);
        }
    }

    fn apply_priority(&mut self, new_priority: u8) {
        let (task_id, before, old_priority) = {
            let Some(task) = self.selected_task() else {
                return;
            };
            (task.id.clone(), task.clone(), task.priority)
        };

        if old_priority == new_priority {
            return;
        }

        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.priority = new_priority;
        }

        let uuid = new_uuid();
        self.temp_id_pending.insert(
            uuid.clone(),
            OptimisticOp::TaskUpdated {
                task_id: task_id.clone(),
                before,
            },
        );
        self.pending_commands.push(SyncCommand {
            r#type: "item_update".to_string(),
            temp_id: None,
            uuid,
            args: serde_json::json!({ "id": task_id, "priority": new_priority }),
        });
        self.flush_commands();
    }

    fn start_comment_input(&mut self) {
        self.comment_input = true;
        self.show_input = true;
        self.input_buffer.clear();
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Insert);
        }
    }

    fn start_field_edit(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };

        if self.detail_field == 1 {
            self.priority_selection = task.priority;
            self.show_priority_picker = true;
            return;
        }

        let prefill = match self.detail_field {
            0 => task.content.clone(),
            2 => task
                .due
                .as_ref()
                .and_then(|d| d.string.clone())
                .unwrap_or_default(),
            3 => task.description.clone(),
            _ => return,
        };
        self.editing_field = true;
        self.show_input = true;
        self.input_buffer = prefill;
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Insert);
        }
    }

    fn move_detail_field(&mut self, delta: i32) {
        let max_fields = 4;
        let current = self.detail_field as i32;
        self.detail_field = (current + delta).rem_euclid(max_fields) as usize;
    }

    fn toggle_collapse(&mut self) {
        let visible = self.visible_tasks();
        let Some(task) = visible.get(self.selected_task) else {
            return;
        };
        let task_id = task.id.clone();
        let parent_id = task.parent_id.clone();

        if self.has_children(&task_id) {
            if self.collapsed.contains(&task_id) {
                self.collapsed.remove(&task_id);
            } else {
                self.collapsed.insert(task_id);
            }
            return;
        }

        if let Some(pid) = parent_id {
            self.collapsed.insert(pid.clone());
            if let Some(pos) = self.visible_tasks().iter().position(|t| t.id == pid) {
                self.selected_task = pos;
            }
        }
    }

    fn close_all_folds(&mut self) {
        let parent_ids: HashSet<String> = self
            .tasks
            .iter()
            .filter_map(|t| t.parent_id.clone())
            .collect();
        for task in &self.tasks {
            if parent_ids.contains(&task.id) {
                self.collapsed.insert(task.id.clone());
            }
        }
    }

    pub fn toggle_input_mode(&mut self) {
        self.input_mode = match self.input_mode {
            InputMode::Vim(_) => InputMode::Standard,
            InputMode::Standard => InputMode::Vim(VimState::Normal),
        };
        info!(mode = self.input_mode.label(), "input mode toggled");
    }

    fn set_error(&mut self, err: &anyhow::Error, context: &str) {
        let app_err = AppError::from_api(err, context);
        error!(context, error = %app_err.message, "app error");
        self.error = Some(app_err);
    }

    fn handle_error_dismiss(&mut self) {
        if let Some(err) = self.error.take() {
            if !err.recoverable {
                info!("unrecoverable error dismissed, exiting");
                self.running = false;
            } else {
                debug!("error dismissed, continuing");
            }
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
        let today = crate::ui::dates::today_str();
        let week_end = crate::ui::dates::offset_days_str(7);

        let mut due_today = 0u32;
        let mut due_week = 0u32;
        let mut overdue = 0u32;
        let mut by_priority = [0u32; 5];

        for task in &self.tasks {
            if task.is_deleted {
                continue;
            }
            if !task.checked {
                let p = task.priority as usize;
                if p < by_priority.len() {
                    by_priority[p] += 1;
                }
            }
            if let Some(due) = &task.due {
                if due.date == today && !task.checked {
                    due_today += 1;
                }
                if due.date < today && !task.checked {
                    overdue += 1;
                }
                if due.date >= today && due.date <= week_end {
                    due_week += 1;
                }
            }
        }

        OverviewStats {
            due_today,
            due_week,
            overdue,
            by_priority,
        }
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
        let today = crate::ui::dates::today_str();
        let week_end = crate::ui::dates::offset_days_str(7);

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
                    return match dock {
                        DockItem::DueOverdue => {
                            t.due.as_ref().is_some_and(|d| d.date < today) && !t.checked
                        }
                        DockItem::DueToday => t.due.as_ref().is_some_and(|d| d.date == today),
                        DockItem::DueWeek => t
                            .due
                            .as_ref()
                            .is_some_and(|d| d.date >= today && d.date <= week_end),
                        DockItem::Priority(p) => t.priority == p && !t.checked,
                    };
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
            SortMode::Priority => top_level.sort_by(|a, b| b.priority.cmp(&a.priority)),
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

        if matches!(self.task_filter, TaskFilter::Done | TaskFilter::Both) {
            if let Some(pid) = self
                .projects
                .get(self.selected_project)
                .map(|p| p.id.clone())
            {
                self.append_cached_completed(&pid, &mut result);
            }
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
        {
            if let Some(cached) = self.completed_cache.get(pid) {
                return cached
                    .iter()
                    .any(|t| self.is_cached_descendant_of(t, &task.id, cached));
            }
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
                    .map_or(true, |pid| !cached_ids.contains(pid.as_str()))
            })
            .collect();
        roots.sort_by_key(|t| t.child_order);

        for root in roots {
            // If this cached root has an active parent not yet shown, add it as a context row.
            if let Some(ref pid) = root.parent_id {
                if !already_shown.contains(pid.as_str()) {
                    if let Some(parent) = self.tasks.iter().find(|t| t.id == *pid && !t.is_deleted)
                    {
                        result.push(parent);
                    }
                }
            }
            result.push(root);
            self.collect_cached_children(&root.id, cached, &mut *result);
        }
    }

    fn collect_cached_children<'a>(
        &self,
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
            self.collect_cached_children(&child.id, cached, result);
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

fn collect_project_subtree(parent_id: Option<&str>, all: &[Project], out: &mut Vec<Project>) {
    let mut children: Vec<&Project> = all
        .iter()
        .filter(|p| p.parent_id.as_deref() == parent_id)
        .collect();
    children.sort_by(|a, b| {
        let a_pin = a.is_inbox() || a.is_favorite;
        let b_pin = b.is_inbox() || b.is_favorite;
        b_pin.cmp(&a_pin).then(a.child_order.cmp(&b.child_order))
    });
    for child in children {
        out.push(child.clone());
        collect_project_subtree(Some(&child.id), all, out);
    }
}

async fn run_websocket(url: String, tx: mpsc::Sender<BgResult>) {
    use futures_util::StreamExt;
    use tokio_tungstenite::connect_async_tls_with_config;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut backoff_secs = 5u64;
    loop {
        let connect_result = (|| async {
            let mut req = url.as_str().into_client_request()?;
            req.headers_mut()
                .insert("Origin", "https://app.todoist.com".parse()?);
            connect_async_tls_with_config(req, None, false, None).await
        })()
        .await;

        match connect_result {
            Ok((ws_stream, _)) => {
                backoff_secs = 5;
                let _ = tx.send(BgResult::WebSocketConnected).await;

                let (_, mut read) = ws_stream.split();
                while read.next().await.is_some() {
                    let _ = tx.send(BgResult::WebSocketEvent).await;
                }
                let _ = tx.send(BgResult::WebSocketDisconnected).await;
                // Clean disconnect — reconnect quickly without growing backoff.
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Err(e) => {
                debug!(error = %e, "websocket connection failed, retrying");
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(60);
    }
}

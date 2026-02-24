use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::api::models::{
    Comment, CreateComment, CreateTask, Project, Task, UpdateProject,
};

use crate::keys::{self, KeyAction};
use crate::ui;

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
#[allow(dead_code)]
pub enum VimState {
    Normal,
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

    #[allow(dead_code)]
    pub fn is_vim(&self) -> bool {
        matches!(self, InputMode::Vim(_))
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

    #[allow(dead_code)]
    fn fatal(err: &anyhow::Error) -> Self {
        Self {
            title: "Fatal Error".to_string(),
            message: format!("{err:#}"),
            suggestion: None,
            recoverable: false,
        }
    }
}

struct ParsedContent {
    content: String,
    priority: Option<u8>,
    due: Option<String>,
    warning: Option<String>,
}

const NATURAL_DATE_KEYWORDS: &[&str] = &[
    "next sunday",
    "next saturday",
    "next friday",
    "next thursday",
    "next wednesday",
    "next tuesday",
    "next monday",
    "next week",
    "tomorrow",
    "yesterday",
    "today",
];

fn parse_task_content(input: &str) -> ParsedContent {
    let mut priority = None;
    let mut due: Option<String> = None;
    let mut warning = None;
    let mut cleaned_parts = Vec::new();

    let lower = input.to_lowercase();

    for kw in NATURAL_DATE_KEYWORDS {
        if lower.contains(kw) {
            due = Some(kw.to_string());
            break;
        }
    }

    for word in input.split_whitespace() {
        let w = word.to_lowercase();

        if priority.is_none() && matches!(w.as_str(), "p1" | "p2" | "p3" | "p4") {
            priority = Some(match w.as_str() {
                "p1" => 4,
                "p2" => 3,
                "p3" => 2,
                _ => 1,
            });
            continue;
        }

        if due.is_none()
            && let Some(parsed) = try_parse_date(&w)
        {
            match parsed {
                DateParsed::Valid(d) => {
                    due = Some(d);
                    continue;
                }
                DateParsed::Invalid(msg) => {
                    warning = Some(msg);
                }
            }
        }

        if due
            .as_ref()
            .is_some_and(|d| d.split_whitespace().any(|dw| dw == w))
        {
            continue;
        }

        cleaned_parts.push(word);
    }

    ParsedContent {
        content: cleaned_parts.join(" "),
        priority,
        due,
        warning,
    }
}

enum DateParsed {
    Valid(String),
    Invalid(String),
}

fn try_parse_date(token: &str) -> Option<DateParsed> {
    let parts_dash: Vec<&str> = token.split('-').collect();
    let parts_slash: Vec<&str> = token.split('/').collect();

    let parts = if parts_dash.len() == 3 {
        Some(parts_dash)
    } else if parts_slash.len() == 3 {
        Some(parts_slash)
    } else {
        None
    };

    let parts = parts?;

    let (year, month, day) = if parts[0].len() == 4 {
        // YYYY-MM-DD or YYYY/MM/DD
        (
            parts[0].parse::<u32>().ok()?,
            parts[1].parse::<u32>().ok()?,
            parts[2].parse::<u32>().ok()?,
        )
    } else if parts[2].len() == 4 {
        // DD-MM-YYYY or DD/MM/YYYY
        (
            parts[2].parse::<u32>().ok()?,
            parts[1].parse::<u32>().ok()?,
            parts[0].parse::<u32>().ok()?,
        )
    } else {
        return Some(DateParsed::Invalid(format!(
            "Unrecognized date format: '{token}'. Use YYYY-MM-DD, DD/MM/YYYY, or DD-MM-YYYY."
        )));
    };

    if !(1..=12).contains(&month) {
        return Some(DateParsed::Invalid(format!(
            "Invalid month {month} in '{token}'. Month must be 01-12."
        )));
    }

    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 31,
    };

    if day < 1 || day > max_day {
        return Some(DateParsed::Invalid(format!(
            "Invalid day {day} for month {month} in '{token}'."
        )));
    }

    if !(2020..=2100).contains(&year) {
        return Some(DateParsed::Invalid(format!(
            "Year {year} seems unlikely in '{token}'. Expected 2020-2100."
        )));
    }

    Some(DateParsed::Valid(format!("{year:04}-{month:02}-{day:02}")))
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
    pub project_idx: usize,
    pub active_field: usize,
    pub editing: bool,
}

impl TaskForm {
    pub fn new(project_idx: usize) -> Self {
        Self {
            content: String::new(),
            priority: 1,
            due_string: String::new(),
            project_idx,
            active_field: 0,
            editing: true,
        }
    }

    pub fn field_count() -> usize {
        4
    }

    #[allow(dead_code)]
    pub fn field_label(idx: usize) -> &'static str {
        match idx {
            0 => "Content",
            1 => "Priority",
            2 => "Due date",
            3 => "Project",
            _ => "",
        }
    }
}

enum BgResult {
    Tasks {
        project_id: String,
        tasks: Result<Vec<Task>>,
    },
    TaskClosed {
        task_id: String,
        project_id: String,
        result: Result<()>,
    },
    TaskCreated {
        project_id: String,
        result: Box<Result<Task>>,
    },
    TaskUpdated {
        task_id: String,
        result: Box<Result<Task>>,
    },
    Comments {
        task_id: String,
        comments: Result<Vec<Comment>>,
    },
    CommentCreated {
        task_id: String,
        result: Result<Comment>,
    },
    ProjectUpdated {
        project_id: String,
        result: Result<Project>,
    },
}

pub struct App {
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
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
    pub refreshing: bool,
    pub detail_scroll: u16,
    pub sort_mode: SortMode,
    pub comments: Vec<Comment>,
    pub comment_input: bool,
    pub detail_field: usize,
    pub show_priority_picker: bool,
    pub priority_selection: u8,
    pub editing_field: bool,
    #[allow(dead_code)]
    pub edit_buffer: String,
    pub task_form: Option<TaskForm>,
    pub current_user_id: Option<String>,
    pub user_names: HashMap<String, UserRecord>,
    pub task_filter: TaskFilter,
    pub dock_focus: Option<usize>,
    pub dock_filter: Option<DockItem>,
    pub pending_done: HashSet<String>,
    pub completed_tasks: HashMap<String, Task>,
    pub themes: Vec<crate::ui::theme::Theme>,
    pub theme_idx: usize,
    pub show_theme_picker: bool,
    pub theme_selection: usize,
    task_cache: HashMap<String, Vec<Task>>,
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

impl App {
    pub fn theme(&self) -> &crate::ui::theme::Theme {
        &self.themes[self.theme_idx]
    }

    pub fn cycle_task_filter(&mut self) {
        self.task_filter = self.task_filter.next();
        self.pending_done.clear();
        let visible_len = self.visible_tasks().len();
        if visible_len == 0 {
            self.selected_task = 0;
        } else if self.selected_task >= visible_len {
            self.selected_task = visible_len - 1;
        }
    }

    fn completed_path() -> std::path::PathBuf {
        ratatoist_core::config::Config::config_dir().join("completed.json")
    }

    fn load_completed_tasks() -> HashMap<String, Task> {
        let path = Self::completed_path();
        let tasks: Vec<Task> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|src| serde_json::from_str(&src).ok())
            .unwrap_or_default();
        tasks.into_iter().map(|t| (t.id.clone(), t)).collect()
    }

    fn save_completed_tasks(&self) {
        let path = Self::completed_path();
        let tasks: Vec<&Task> = self.completed_tasks.values().collect();
        if let Ok(json) = serde_json::to_string(&tasks) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn save_theme_preference(&self) {
        let dir = ratatoist_core::config::Config::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("ui_settings.json");
        let name = &self.themes[self.theme_idx].name;
        let json = serde_json::json!({ "theme": name });
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        );
    }

    pub fn new(client: TodoistClient) -> Self {
        let (bg_tx, bg_rx) = mpsc::channel(32);
        let mut themes = crate::ui::theme::Theme::builtin();
        let user_themes_dir = ratatoist_core::config::Config::config_dir().join("themes");
        themes.extend(crate::ui::theme::Theme::load_user_themes(&user_themes_dir));
        let theme_idx = load_theme_idx(&themes);
        Self {
            projects: Vec::new(),
            tasks: Vec::new(),
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
            refreshing: false,
            detail_scroll: 0,
            sort_mode: SortMode::Default,
            comments: Vec::new(),
            comment_input: false,
            detail_field: 0,
            show_priority_picker: false,
            priority_selection: 1,
            editing_field: false,
            edit_buffer: String::new(),
            task_form: None,
            task_filter: TaskFilter::Active,
            dock_focus: None,
            dock_filter: None,
            pending_done: HashSet::new(),
            completed_tasks: Self::load_completed_tasks(),
            current_user_id: None,
            user_names: HashMap::new(),
            themes,
            theme_idx,
            show_theme_picker: false,
            theme_selection: theme_idx,
            task_cache: HashMap::new(),
            bg_tx,
            bg_rx,
            client: Arc::new(client),
        }
    }

    pub async fn load_with_splash(&mut self, terminal: &mut DefaultTerminal) {
        info!("full sync starting");

        terminal
            .draw(|f| ui::splash::render(f, 0.0, "connecting to todoist...", self.theme()))
            .ok();

        if let Ok(user) = self.client.get_user().await {
            let record = UserRecord::new(user.id.clone(), user.full_name, user.email);
            info!(user_id = %record.id, display = %record.display, "loaded current user");
            self.current_user_id = Some(user.id.clone());
            self.user_names.insert(user.id, record);
        }

        let projects = match self.client.get_projects().await {
            Ok(p) => p,
            Err(e) => {
                self.set_error(&e, "Load projects");
                return;
            }
        };

        let total = projects.len();
        info!(count = total, "fetched projects, syncing tasks");

        terminal
            .draw(|f| {
                ui::splash::render(
                    f,
                    0.05,
                    &format!("found {total} projects, loading tasks..."),
                    self.theme(),
                )
            })
            .ok();

        for (i, project) in projects.iter().enumerate() {
            let progress = 0.05 + 0.95 * ((i + 1) as f64 / total as f64);
            let status = format!("syncing {} ({}/{})", project.name, i + 1, total);

            terminal
                .draw(|f| ui::splash::render(f, progress, &status, self.theme()))
                .ok();

            match self.client.get_tasks(Some(&project.id)).await {
                Ok(tasks) => {
                    self.task_cache.insert(project.id.clone(), tasks);
                }
                Err(e) => {
                    error!(project_id = %project.id, "failed to sync tasks");
                    self.set_error(&e, "Sync tasks");
                }
            }
        }

        self.projects = projects;

        let shared: Vec<String> = self
            .projects
            .iter()
            .filter(|p| p.is_shared)
            .map(|p| p.id.clone())
            .collect();

        for pid in &shared {
            if let Ok(collabs) = self.client.get_collaborators(pid).await {
                for c in collabs {
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        self.user_names.entry(c.id.clone())
                    {
                        let record = UserRecord::new(c.id, c.name, c.email);
                        debug!(user_id = %record.id, display = %record.display, "cached collaborator");
                        e.insert(record);
                    }
                }
            }
        }

        if let Some(project) = self.projects.first()
            && let Some(cached) = self.task_cache.get(&project.id)
        {
            self.tasks = cached.clone();
        }

        info!(
            projects = self.projects.len(),
            cached_projects = self.task_cache.len(),
            users = self.user_names.len(),
            "full sync complete"
        );
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        info!("entering main loop");

        while self.running {
            self.drain_bg_results();

            terminal.draw(|frame| ui::draw(frame, self))?;

            if event::poll(Duration::from_millis(16))?
                && let Event::Key(key) = event::read()?
            {
                if self.error.is_some() {
                    self.handle_error_dismiss();
                    continue;
                }

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
                        self.save_theme_preference();
                    }
                    KeyAction::CloseThemePicker => {
                        self.show_theme_picker = false;
                    }
                    KeyAction::Consumed | KeyAction::None => {}
                }
            }
        }

        info!("exiting main loop");
        Ok(())
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
                if let Some(cached) = self.task_cache.get(&task_project_id) {
                    self.tasks = cached.clone();
                }
            }

            self.active_pane = Pane::Detail;
            self.detail_scroll = 0;
            self.detail_field = 0;
            self.comments.clear();
            self.spawn_comments_fetch(task_id);
        }
    }

    fn spawn_comments_fetch(&self, task_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let tid = task_id.clone();

        tokio::spawn(async move {
            let comments = client.get_comments(&tid).await;
            let _ = tx
                .send(BgResult::Comments {
                    task_id: tid,
                    comments,
                })
                .await;
        });
    }

    fn switch_to_project_tasks(&mut self) {
        let Some(project) = self.projects.get(self.selected_project) else {
            return;
        };
        let pid = project.id.clone();

        self.pending_done.clear();

        if let Some(cached) = self.task_cache.get(&pid) {
            self.tasks = cached.clone();
            self.selected_task = 0;
        } else {
            self.tasks.clear();
            self.selected_task = 0;
        }

        self.spawn_bg_fetch(pid);
    }

    fn spawn_bg_fetch(&self, project_id: String) {
        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let pid = project_id.clone();

        tokio::spawn(async move {
            let tasks = client.get_tasks(Some(&pid)).await;
            let _ = tx
                .send(BgResult::Tasks {
                    project_id: pid,
                    tasks,
                })
                .await;
        });
    }

    fn complete_selected_task(&mut self) {
        let visible = self.visible_tasks();
        let Some(task) = visible.get(self.selected_task) else {
            return;
        };
        let task_id = task.id.clone();
        let was_checked = task.checked;
        let task_project_id = task.project_id.clone();
        let task_snapshot = (**task).clone();

        // Optimistic update: task may be in self.tasks (current project) or task_cache (cross-project)
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.checked = !was_checked;
        } else if let Some(cached) = self.task_cache.get_mut(&task_project_id) {
            if let Some(t) = cached.iter_mut().find(|t| t.id == task_id) {
                t.checked = !was_checked;
            }
        }

        if !was_checked {
            self.pending_done.insert(task_id.clone());
            let mut snapshot = task_snapshot;
            snapshot.checked = true;
            self.completed_tasks.insert(task_id.clone(), snapshot);
            self.save_completed_tasks();
        } else {
            self.pending_done.remove(&task_id);
            self.completed_tasks.remove(&task_id);
            self.save_completed_tasks();
        }

        let new_len = self.visible_tasks().len();
        if new_len > 0 && self.selected_task >= new_len {
            self.selected_task = new_len - 1;
        }

        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let tid = task_id.clone();
        let pid = task_project_id;

        tokio::spawn(async move {
            let result = if was_checked {
                client.reopen_task(&tid).await
            } else {
                client.close_task(&tid).await
            };
            let _ = tx
                .send(BgResult::TaskClosed {
                    task_id: tid,
                    project_id: pid,
                    result,
                })
                .await;
        });
    }

    fn start_input(&mut self) {
        self.task_form = Some(TaskForm::new(self.selected_project));
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
                    let parsed = parse_task_content(&content);
                    form.content = parsed.content;
                    if let Some(p) = parsed.priority {
                        form.priority = p;
                    }
                    if let Some(d) = parsed.due {
                        form.due_string = d;
                    }
                    if let Some(warn) = parsed.warning {
                        self.task_form = Some(form);
                        self.input_buffer.clear();
                        self.show_input = false;
                        if let InputMode::Vim(_) = self.input_mode {
                            self.input_mode = InputMode::Vim(VimState::Normal);
                        }
                        self.error = Some(AppError {
                            title: "Date format issue".to_string(),
                            message: warn,
                            suggestion: Some(
                                "Accepted formats: YYYY-MM-DD, DD/MM/YYYY, DD-MM-YYYY, \
                                 or natural language (today, tomorrow, next monday)"
                                    .to_string(),
                            ),
                            recoverable: true,
                        });
                        return;
                    }
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

        let project_id = self.projects.get(form.project_idx).map(|p| p.id.clone());
        let due = if form.due_string.is_empty() {
            None
        } else {
            Some(form.due_string.clone())
        };
        let priority = if form.priority > 1 {
            Some(form.priority)
        } else {
            None
        };

        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let pid = project_id.clone().unwrap_or_default();

        tokio::spawn(async move {
            let result = client
                .create_task(&CreateTask {
                    content: form.content,
                    description: None,
                    project_id,
                    priority,
                    due_string: due,
                    labels: None,
                })
                .await;
            let _ = tx
                .send(BgResult::TaskCreated {
                    project_id: pid,
                    result: Box::new(result),
                })
                .await;
        });

        self.task_form = None;
        self.show_input = false;
        self.input_buffer.clear();
        if let InputMode::Vim(_) = self.input_mode {
            self.input_mode = InputMode::Vim(VimState::Normal);
        }
    }

    fn submit_comment(&self, content: String) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id.clone();

        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let tid = task_id.clone();

        tokio::spawn(async move {
            let result = client
                .create_comment(&CreateComment {
                    content,
                    task_id: Some(tid.clone()),
                    project_id: None,
                })
                .await;
            let _ = tx
                .send(BgResult::CommentCreated {
                    task_id: tid,
                    result,
                })
                .await;
        });
    }

    fn submit_field_edit(&mut self, value: String) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id.clone();
        let field = self.detail_field;

        let body = match field {
            0 => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.content = value.clone();
                }
                ratatoist_core::api::models::UpdateTask {
                    content: Some(value),
                    description: None,
                    priority: None,
                    due_string: None,
                    labels: None,
                }
            }
            2 => ratatoist_core::api::models::UpdateTask {
                content: None,
                description: None,
                priority: None,
                due_string: Some(value),
                labels: None,
            },
            3 => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    t.description = value.clone();
                }
                ratatoist_core::api::models::UpdateTask {
                    content: None,
                    description: Some(value),
                    priority: None,
                    due_string: None,
                    labels: None,
                }
            }
            _ => return,
        };

        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let tid = task_id.clone();

        tokio::spawn(async move {
            let result = client.update_task(&tid, &body).await;
            let _ = tx
                .send(BgResult::TaskUpdated {
                    task_id: tid,
                    result: Box::new(result),
                })
                .await;
        });
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
                    form.project_idx = (form.project_idx + 1) % self.projects.len().max(1);
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

    fn drain_bg_results(&mut self) {
        let mut had_results = false;
        while let Ok(result) = self.bg_rx.try_recv() {
            had_results = true;
            match result {
                BgResult::Tasks { project_id, tasks } => match tasks {
                    Ok(fresh) => {
                        let is_current = self
                            .projects
                            .get(self.selected_project)
                            .is_some_and(|p| p.id == project_id);
                        if is_current {
                            let preserved: Vec<Task> = self
                                .tasks
                                .iter()
                                .filter(|t| self.pending_done.contains(&t.id))
                                .filter(|t| !fresh.iter().any(|f| f.id == t.id))
                                .cloned()
                                .collect();
                            self.tasks = fresh.clone();
                            self.tasks.extend(preserved);
                            let visible_len = self.visible_tasks().len();
                            if self.selected_task >= visible_len {
                                self.selected_task = visible_len.saturating_sub(1);
                            }
                        }
                        self.task_cache.insert(project_id, fresh);
                    }
                    Err(e) => self.set_error(&e, "Sync tasks"),
                },
                BgResult::TaskClosed {
                    task_id,
                    project_id,
                    result,
                } => {
                    if let Err(e) = result {
                        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            t.checked = !t.checked;
                        } else if let Some(cached) = self.task_cache.get_mut(&project_id) {
                            if let Some(t) = cached.iter_mut().find(|t| t.id == task_id) {
                                t.checked = !t.checked;
                            }
                        }
                        self.completed_tasks.remove(&task_id);
                        self.pending_done.remove(&task_id);
                        self.save_completed_tasks();
                        self.set_error(&e, "Complete task");
                    } else {
                        self.task_cache.remove(&project_id);
                    }
                }
                BgResult::TaskCreated { project_id, result } => match *result {
                    Ok(task) => {
                        let current_pid = self
                            .projects
                            .get(self.selected_project)
                            .map(|p| p.id.as_str());
                        if current_pid == Some(project_id.as_str()) {
                            self.tasks.push(task);
                        }
                        self.task_cache.remove(&project_id);
                    }
                    Err(e) => {
                        self.set_error(&e, "Create task");
                        if self.task_form.is_none() {
                            let mut form = TaskForm::new(self.selected_project);
                            form.editing = false;
                            self.task_form = Some(form);
                        }
                    }
                },
                BgResult::TaskUpdated { task_id, result } => match *result {
                    Ok(updated) => {
                        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            *t = updated;
                        }
                        self.invalidate_current_project_cache();
                    }
                    Err(e) => self.set_error(&e, "Update task"),
                },
                BgResult::Comments { task_id, comments } => match comments {
                    Ok(c) => {
                        let count = c.len() as i32;
                        let mut project_id_for_cache = None;
                        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                            t.note_count = Some(count);
                            project_id_for_cache = Some(t.project_id.clone());
                        }
                        if let Some(pid) = project_id_for_cache
                            && let Some(cached) = self.task_cache.get_mut(&pid)
                            && let Some(ct) = cached.iter_mut().find(|t| t.id == task_id)
                        {
                            ct.note_count = Some(count);
                        }
                        let current_tid = self.selected_task().map(|t| t.id.clone());
                        if current_tid.as_deref() == Some(&task_id) {
                            self.comments = c;
                        }
                    }
                    Err(e) => self.set_error(&e, "Load comments"),
                },
                BgResult::CommentCreated { task_id, result } => match result {
                    Ok(comment) => {
                        let current_tid = self.selected_task().map(|t| t.id.clone());
                        if current_tid.as_deref() == Some(&task_id) {
                            self.comments.push(comment);
                        }
                    }
                    Err(e) => self.set_error(&e, "Add comment"),
                },
                BgResult::ProjectUpdated {
                    project_id, result, ..
                } => match result {
                    Ok(updated) => {
                        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
                            p.is_favorite = updated.is_favorite;
                        }
                        self.sort_projects();
                    }
                    Err(e) => {
                        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
                            p.is_favorite = !p.is_favorite;
                        }
                        self.sort_projects();
                        self.set_error(&e, "Update project");
                    }
                },
            }
        }
        self.refreshing = !had_results && self.refreshing;
    }

    fn star_selected_project(&mut self) {
        let Some(project) = self.projects.get_mut(self.selected_project) else {
            return;
        };
        project.is_favorite = !project.is_favorite;
        let pid = project.id.clone();
        let new_fav = project.is_favorite;
        let name = project.name.clone();

        self.sort_projects();

        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();

        tokio::spawn(async move {
            let result = client
                .update_project(
                    &pid,
                    &UpdateProject {
                        name: Some(name),
                        color: None,
                        is_favorite: Some(new_fav),
                    },
                )
                .await;
            let _ = tx
                .send(BgResult::ProjectUpdated {
                    project_id: pid,
                    result,
                })
                .await;
        });
    }

    fn sort_projects(&mut self) {
        let selected_id = self
            .projects
            .get(self.selected_project)
            .map(|p| p.id.clone());

        self.projects.sort_by(|a, b| {
            let a_pin = a.is_inbox() || a.is_favorite;
            let b_pin = b.is_inbox() || b.is_favorite;
            b_pin.cmp(&a_pin).then(a.child_order.cmp(&b.child_order))
        });

        if let Some(id) = selected_id
            && let Some(pos) = self.projects.iter().position(|p| p.id == id)
        {
            self.selected_project = pos;
        }
    }

    fn apply_priority(&mut self, new_priority: u8) {
        let Some(task) = self.selected_task() else {
            return;
        };
        if task.priority == new_priority {
            return;
        }
        let task_id = task.id.clone();

        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.priority = new_priority;
        }

        let body = ratatoist_core::api::models::UpdateTask {
            content: None,
            description: None,
            priority: Some(new_priority),
            due_string: None,
            labels: None,
        };

        let client = Arc::clone(&self.client);
        let tx = self.bg_tx.clone();
        let tid = task_id.clone();

        tokio::spawn(async move {
            let result = client.update_task(&tid, &body).await;
            let _ = tx
                .send(BgResult::TaskUpdated {
                    task_id: tid,
                    result: Box::new(result),
                })
                .await;
        });
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

    fn invalidate_current_project_cache(&mut self) {
        if let Some(project) = self.projects.get(self.selected_project) {
            self.task_cache.remove(&project.id);
        }
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

    #[allow(dead_code)]
    fn set_fatal_error(&mut self, err: &anyhow::Error) {
        error!(error = %format!("{err:#}"), "fatal error");
        self.error = Some(AppError::fatal(err));
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

    #[allow(dead_code)]
    pub fn resolve_user_name(&self, uid: &str) -> String {
        if let Some(record) = self.user_names.get(uid) {
            return record.display.clone();
        }
        format!("user-{}", &uid[..uid.len().min(6)])
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

        for tasks in self.task_cache.values() {
            for task in tasks {
                if let Some(due) = &task.due {
                    if due.date == today {
                        due_today += 1;
                    }
                    if due.date < today && !task.checked {
                        overdue += 1;
                    }
                    if due.date >= today && due.date <= week_end {
                        due_week += 1;
                    }
                }
                if !task.checked {
                    let p = (task.priority as usize).min(4);
                    by_priority[p] += 1;
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

        let source: Vec<&Task> = if self.dock_filter.is_some() {
            self.task_cache
                .values()
                .flat_map(|tasks| tasks.iter())
                .collect()
        } else {
            self.tasks.iter().collect()
        };

        let mut top_level: Vec<&Task> = source
            .into_iter()
            .filter(|t| t.parent_id.is_none())
            .filter(|t| {
                self.pending_done.contains(&t.id)
                    || match self.task_filter {
                        TaskFilter::Active => !t.checked,
                        TaskFilter::Done => t.checked,
                        TaskFilter::Both => true,
                    }
            })
            .filter(|t| match self.dock_filter {
                None => true,
                Some(DockItem::DueOverdue) => {
                    t.due.as_ref().is_some_and(|d| d.date < today) && !t.checked
                }
                Some(DockItem::DueToday) => t.due.as_ref().is_some_and(|d| d.date == today),
                Some(DockItem::DueWeek) => t
                    .due
                    .as_ref()
                    .is_some_and(|d| d.date >= today && d.date <= week_end),
                Some(DockItem::Priority(p)) => t.priority == p && !t.checked,
            })
            .collect();

        // Include persisted completed tasks for current project in Done/Both view
        if matches!(self.task_filter, TaskFilter::Done | TaskFilter::Both)
            && self.dock_filter.is_none()
        {
            let current_pid = self
                .projects
                .get(self.selected_project)
                .map(|p| p.id.as_str());
            let existing_ids: HashSet<&str> = top_level.iter().map(|t| t.id.as_str()).collect();
            for task in self.completed_tasks.values() {
                if task.parent_id.is_none()
                    && current_pid == Some(task.project_id.as_str())
                    && !existing_ids.contains(task.id.as_str())
                {
                    top_level.push(task);
                }
            }
        }

        match self.sort_mode {
            SortMode::Default => top_level.sort_by_key(|t| t.child_order),
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
                self.collect_visible_children(&task.id, &mut result);
            }
        }
        result
    }

    fn collect_visible_children<'a>(&'a self, parent_id: &str, result: &mut Vec<&'a Task>) {
        let mut children: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent_id))
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
}

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::api::client::TodoistClient;
use crate::api::models::{Project, Task};
use crate::keys::{self, KeyAction};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Projects,
    Tasks,
}

pub struct App {
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub selected_project: usize,
    pub selected_task: usize,
    pub active_pane: Pane,
    pub running: bool,
    client: TodoistClient,
}

impl App {
    pub fn new(client: TodoistClient) -> Self {
        Self {
            projects: Vec::new(),
            tasks: Vec::new(),
            selected_project: 0,
            selected_task: 0,
            active_pane: Pane::Projects,
            running: true,
            client,
        }
    }

    pub async fn load_initial_data(&mut self) -> Result<()> {
        self.projects = self.client.get_projects().await?;
        if let Some(project) = self.projects.first() {
            self.tasks = self.client.get_tasks(Some(&project.id)).await?;
        }
        Ok(())
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            terminal.draw(|frame| ui::draw(frame, self))?;

            if event::poll(Duration::from_millis(16))?
                && let Event::Key(key) = event::read()?
            {
                match keys::handle_key(self, key) {
                    KeyAction::Quit => self.running = false,
                    KeyAction::LoadTasks => self.load_selected_project_tasks().await?,
                    KeyAction::Consumed | KeyAction::None => {}
                }
            }
        }
        Ok(())
    }

    async fn load_selected_project_tasks(&mut self) -> Result<()> {
        if let Some(project) = self.projects.get(self.selected_project) {
            let pid = project.id.clone();
            self.tasks = self.client.get_tasks(Some(&pid)).await?;
            self.selected_task = 0;
            self.active_pane = Pane::Tasks;
        }
        Ok(())
    }

    pub fn selected_project_name(&self) -> &str {
        self.projects
            .get(self.selected_project)
            .map(|p| p.name.as_str())
            .unwrap_or("Tasks")
    }
}

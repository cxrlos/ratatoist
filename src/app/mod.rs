//! The event-driven core: [`State`] in, [`Event`]s applied by [`update`],
//! [`Effect`]s out. [`App`] owns the state and the terminal and redraws only
//! when an event changed the screen.

mod action;
mod confirm;
mod effect;
mod event;
mod failure;
mod modal;
mod palette;
mod picker;
mod release;
mod state;
mod sync;
mod text_field;
mod update;
mod view;

pub use action::{Action, Editable};
pub use confirm::{Confirm, Confirmed};
pub use effect::Effect;
pub use event::Event;
pub use failure::{ERROR_LOG_CAP, ErrorKind, ErrorLog, Failure, LoadError};
pub use modal::{InputKind, Modal, TaskForm};
pub use palette::Palette;
pub use picker::{Picker, PickerKind, open_picker};
pub use release::ReleaseState;
pub use state::{
    DEFAULT_IDLE_TIMEOUT_SECS, DOCK_ITEMS, DockItem, Indicator, ListFilter, OverviewStats, Pane,
    ProjectEntry, ProjectNavItem, Screen, Sort, SortMode, State, SyncHealth, TaskFilter,
    UserRecord,
};
pub use text_field::TextField;
pub use update::{Update, perform, update};

use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::ui;

pub struct App<B: Backend> {
    state: State,
    terminal: Terminal<B>,
    frames_drawn: u64,
}

impl<B: Backend> App<B> {
    /// Builds the app on `backend` showing `state`, and draws the first frame.
    /// The terminal reports a resize only when its size changes, so the
    /// starting size is read here.
    pub fn new(backend: B, mut state: State) -> Result<Self, B::Error> {
        let terminal = Terminal::new(backend)?;
        if state.viewport.is_none() {
            state.viewport = Some(terminal.size()?);
        }
        let mut app = Self {
            state,
            terminal,
            frames_drawn: 0,
        };
        app.draw()?;
        Ok(app)
    }

    /// Applies `event`, redraws if the screen changed, and returns the
    /// effects the runtime must carry out.
    pub fn handle(&mut self, event: Event) -> Result<Vec<Effect>, B::Error> {
        let Update { redraw, effects } = update(&mut self.state, event);
        if redraw {
            self.draw()?;
        }
        Ok(effects)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// How many frames have been drawn, so tests can assert that an event
    /// that changes nothing does not redraw.
    pub fn frames_drawn(&self) -> u64 {
        self.frames_drawn
    }

    pub fn backend(&self) -> &B {
        self.terminal.backend()
    }

    fn draw(&mut self) -> Result<(), B::Error> {
        let state = &self.state;
        self.terminal.draw(|frame| ui::draw(frame, state))?;
        self.frames_drawn += 1;
        Ok(())
    }
}

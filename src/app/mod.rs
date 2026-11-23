//! The event-driven core: [`State`] in, [`Event`]s applied by [`update`],
//! [`Effect`]s out. [`App`] owns the state and the terminal and redraws only
//! when an event changed the screen.

mod effect;
mod event;
pub mod legacy;
mod state;
mod update;

pub use effect::Effect;
pub use event::Event;
pub use state::State;
pub use update::{Update, update};

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
    pub fn new(backend: B, state: State) -> Result<Self, B::Error> {
        let mut app = Self {
            state,
            terminal: Terminal::new(backend)?,
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
        self.terminal.draw(|frame| ui::draw_state(frame, state))?;
        self.frames_drawn += 1;
        Ok(())
    }
}

use crossterm::event::{KeyCode, KeyModifiers};

use super::{Effect, Event, State};

/// What one event did: whether the screen changed, and what to ask the
/// outside world for.
#[derive(Debug, Default, PartialEq)]
pub struct Update {
    pub redraw: bool,
    pub effects: Vec<Effect>,
}

impl Update {
    fn quit() -> Self {
        Self {
            redraw: false,
            effects: vec![Effect::Quit],
        }
    }
}

pub fn update(state: &mut State, event: Event) -> Update {
    match event {
        Event::Key(key)
            if key.code == KeyCode::Char('q')
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)) =>
        {
            Update::quit()
        }
        Event::Shutdown => Update::quit(),
        Event::Resize { .. } => Update {
            redraw: true,
            effects: Vec::new(),
        },
        Event::Tick { now } => {
            state.now = now;
            state.today = now.date_naive();
            Update::default()
        }
        _ => Update::default(),
    }
}

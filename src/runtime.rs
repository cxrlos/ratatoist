//! The async loop: terminal input, the shutdown signal and effects carried
//! out against a [`TodoistClient`], each answer fed back as an [`Event`].

use std::future::Future;
use std::io;

use crossterm::event::Event as TerminalEvent;
use futures_util::{Stream, StreamExt};
use ratatui::backend::Backend;

use crate::app::{App, Effect, Event, State};
use crate::config::Config;
use crate::todoist::TodoistClient;

/// The first state, with "now" read from the clock.
pub fn initial_state(config: &Config, idle_forcer: bool) -> State {
    let _ = config;
    let mut state = State::new(chrono::Local::now());
    state.idle_forcer = idle_forcer;
    state
}

/// Runs until the app asks to quit, `shutdown` resolves or terminal input
/// ends.
pub async fn run<B, C, I, S>(
    app: &mut App<B>,
    client: &C,
    mut input: I,
    shutdown: S,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    C: TodoistClient,
    I: Stream<Item = io::Result<TerminalEvent>> + Unpin,
    S: Future<Output = ()>,
{
    let _ = client;
    let mut shutdown = std::pin::pin!(shutdown);
    loop {
        let event = tokio::select! {
            () = &mut shutdown => Event::Shutdown,
            next = input.next() => match next {
                None => return Ok(()),
                Some(read) => match read? {
                    TerminalEvent::Key(key) => Event::Key(key),
                    TerminalEvent::Resize(width, height) => Event::Resize { width, height },
                    _ => continue,
                },
            },
        };
        let effects = app.handle(event)?;
        if effects.contains(&Effect::Quit) {
            return Ok(());
        }
    }
}

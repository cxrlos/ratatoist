//! When ratatoist asks Todoist what changed without being asked: shortly
//! after the live connection says something changed, and on a timer while
//! the connection is down. Both pause while the session is idle.

use std::time::Duration;

use jiff::SignedDuration;

use super::failure::{ErrorKind, Failure};
use super::{Effect, State};
use crate::todoist::LiveMessage;

/// How long a change the live connection reports waits for others before
/// one sync picks them all up.
pub const LIVE_DEBOUNCE: Duration = Duration::from_secs(2);
/// How often ratatoist syncs while the live connection is down.
pub const POLL_INTERVAL: SignedDuration = SignedDuration::from_secs(60);

/// The live connection, as far as its messages tell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LiveLink {
    /// It has not connected yet this session.
    #[default]
    Never,
    Up,
    /// It was up and dropped; it is reconnecting.
    Down,
}

/// Applies one message from the live connection. Whether the indicator may
/// have changed.
pub(super) fn message(state: &mut State, message: LiveMessage, effects: &mut Vec<Effect>) -> bool {
    let before = state.live;
    match message {
        LiveMessage::Connected => {
            state.live = LiveLink::Up;
            state.poll_due = None;
            // What changed while it was down was never announced.
            if before == LiveLink::Down {
                schedule_sync(state, effects);
            }
        }
        LiveMessage::Changed => {
            state.live = LiveLink::Up;
            state.poll_due = None;
            schedule_sync(state, effects);
        }
        LiveMessage::Disconnected => {
            if before == LiveLink::Up {
                state.log_failure(Failure::new(
                    ErrorKind::Offline,
                    "keep the live connection open",
                    "the live-updates connection dropped",
                    &state.now,
                ));
            }
            state.live = LiveLink::Down;
        }
    }
    before != state.live
}

/// Asks for one sync [`LIVE_DEBOUNCE`] from now, unless one is already
/// waiting: a burst of changes gives one sync, and a steady stream still
/// syncs once per debounce.
fn schedule_sync(state: &mut State, effects: &mut Vec<Effect>) {
    if state.is_idle() || state.live_sync_scheduled {
        return;
    }
    state.live_sync_scheduled = true;
    effects.push(Effect::ScheduleLiveSync {
        after: LIVE_DEBOUNCE,
    });
}

/// The debounce after a live change ran out.
pub(super) fn sync_due(state: &mut State, effects: &mut Vec<Effect>) {
    state.live_sync_scheduled = false;
    if state.is_idle() || !state.account_loaded() {
        return;
    }
    effects.push(Effect::Sync(state.background_sync_request()));
}

/// Polls on a tick, once `state.now` is the tick's time: a sync every
/// [`POLL_INTERVAL`] while the live connection is not up, the account is
/// loaded and the session is not idle. The first tick that polls only
/// starts the interval.
pub(super) fn tick(state: &mut State, effects: &mut Vec<Effect>) {
    let polls = state.live != LiveLink::Up && state.account_loaded() && !state.is_idle();
    if !polls {
        state.poll_due = None;
        return;
    }
    let now = state.now.timestamp();
    match state.poll_due {
        Some(due) if now < due => {}
        Some(_) => {
            effects.push(Effect::Sync(state.background_sync_request()));
            state.poll_due = now.checked_add(POLL_INTERVAL).ok();
        }
        None => state.poll_due = now.checked_add(POLL_INTERVAL).ok(),
    }
}

/// The first key after the session was idle: one sync catches up on what
/// idle paused, and polling starts its interval again.
pub(super) fn back_from_idle(state: &mut State, effects: &mut Vec<Effect>) {
    state.poll_due = None;
    if state.account_loaded() {
        effects.push(Effect::Sync(state.background_sync_request()));
    }
}

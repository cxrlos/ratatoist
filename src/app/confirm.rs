//! Questions asked before something that discards unsent changes: a full
//! re-sync drops the commands Todoist has not answered, and quitting leaves
//! them unsent. With nothing pending neither asks.

use super::effect::Effect;
use super::modal::Modal;
use super::state::State;

/// What a confirmation runs when the user says yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmed {
    /// Sync everything from scratch, dropping the commands not sent yet.
    Resync,
    /// Quit with commands not sent yet.
    Quit,
}

/// A question on screen: a title, the thing it is about, what saying yes
/// costs, and the verb the yes key shows (`Enter/y re-sync`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub title: String,
    /// What the action applies to, on one line, cut with `…` when it is
    /// too long.
    pub subject: String,
    /// What to know before confirming, one line each.
    pub consequences: Vec<String>,
    pub verb: &'static str,
    pub confirmed: Confirmed,
}

/// Re-syncs everything at once, or asks first while commands are pending.
pub(super) fn resync(state: &mut State, effects: &mut Vec<Effect>) {
    match unsent(state) {
        None => state.force_full_resync(effects),
        Some(changes) => state.modals.push(Modal::Confirm(Confirm {
            title: "Re-sync everything?".to_owned(),
            subject: "Every project and task loads again from Todoist.".to_owned(),
            consequences: vec![format!("{changes} not sent yet and will be dropped.")],
            verb: "re-sync",
            confirmed: Confirmed::Resync,
        })),
    }
}

/// Quits at once, or asks first while commands are pending.
pub(super) fn quit(state: &mut State, effects: &mut Vec<Effect>) {
    match unsent(state) {
        None => effects.push(Effect::Quit),
        Some(changes) => state.modals.push(Modal::Confirm(Confirm {
            title: "Quit with unsent changes?".to_owned(),
            subject: "Todoist has not answered every change yet.".to_owned(),
            consequences: vec![format!("{changes} not sent yet.")],
            verb: "quit",
            confirmed: Confirmed::Quit,
        })),
    }
}

/// Closes the confirmation on top and runs what it asked about. Whether
/// there was one.
pub(super) fn confirm(state: &mut State, effects: &mut Vec<Effect>) -> bool {
    let Some(Modal::Confirm(confirm)) = state.top_modal() else {
        return false;
    };
    let confirmed = confirm.confirmed;
    state.modals.pop();
    match confirmed {
        Confirmed::Resync => state.force_full_resync(effects),
        Confirmed::Quit => effects.push(Effect::Quit),
    }
    true
}

/// Closes the confirmation on top without running anything. Whether there
/// was one.
pub(super) fn decline(state: &mut State) -> bool {
    match state.top_modal() {
        Some(Modal::Confirm(_)) => {
            state.modals.pop();
            true
        }
        _ => false,
    }
}

/// How many commands Todoist has not answered, as `2 changes are`, or
/// `None` when there are none.
fn unsent(state: &State) -> Option<String> {
    match state.temp_id_pending.len() {
        0 => None,
        1 => Some("1 change is".to_owned()),
        count => Some(format!("{count} changes are")),
    }
}

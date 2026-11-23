//! The action registry and the keymap built from it. Key dispatch, chords,
//! the which-key popup, the bottom bar, the cheatsheet, the palette and the
//! config all read bindings from [`ACTIONS`]; no key is bound anywhere else.

mod key;
mod keymap;
mod reference;
mod registry;

pub use key::{Key, KeySequence, ParseKeyError};
pub use keymap::{Change, Keymap, Override, Resolution, Target};
pub use reference::keymap_reference;
pub use registry::{
    ACTIONS, ActionSpec, Availability, Group, Hint, HintRow, OverlayKind, Preset, STANDARD, by_id,
    spec,
};

use crate::app::{Modal, Pane, Screen, State};

/// Where a binding applies. Several contexts are active at once, most
/// specific first, and the first one that binds a key wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Context {
    /// Everywhere outside an overlay.
    Global,
    /// A task list: Inbox, Today, Upcoming, or a list opened from Projects
    /// or Labels.
    List,
    /// A task's detail.
    Detail,
    /// The Projects and Labels screens.
    Browse,
    /// The Projects screen, ahead of [`Context::Browse`] there.
    Projects,
    /// An overlay such as the cheatsheet, which takes every key.
    Overlay,
}

impl Context {
    pub const ALL: [Context; 6] = [
        Context::Global,
        Context::List,
        Context::Detail,
        Context::Browse,
        Context::Projects,
        Context::Overlay,
    ];

    /// The name of its table in `config.toml`, as in `[keys.list]`.
    pub fn name(self) -> &'static str {
        match self {
            Context::Global => "global",
            Context::List => "list",
            Context::Detail => "detail",
            Context::Browse => "browse",
            Context::Projects => "projects",
            Context::Overlay => "overlay",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Context::ALL.into_iter().find(|c| c.name() == name)
    }

    /// The heading of its table in the keymap reference, and of the
    /// cheatsheet pane of its own keys.
    pub fn title(self) -> &'static str {
        match self {
            Context::Global => "Everywhere",
            Context::List => "Task list",
            Context::Detail => "Task detail",
            Context::Browse => "Projects and Labels",
            Context::Projects => "Projects",
            Context::Overlay => "Overlays",
        }
    }
}

/// The contexts taking keys now, most specific first. An open which-key
/// popup is ignored: it only waits for the rest of a chord from below it.
pub fn active_contexts(state: &State) -> Vec<Context> {
    let overlay = state
        .modals
        .iter()
        .rev()
        .find(|modal| !matches!(modal, Modal::WhichKey { .. }));
    match overlay {
        Some(_) => vec![Context::Overlay],
        None => screen_contexts(state),
    }
}

/// The contexts of the screen and its focused pane under any overlay.
/// While the first sync runs, or when the tasks cannot load, only the keys
/// that apply everywhere.
pub fn screen_contexts(state: &State) -> Vec<Context> {
    if state.loading || state.load_error.is_some() {
        return vec![Context::Global];
    }
    match (state.screen, state.active_pane) {
        (_, Pane::Detail) => vec![Context::Detail, Context::Global],
        (Screen::Projects, Pane::Projects) => {
            vec![Context::Projects, Context::Browse, Context::Global]
        }
        (Screen::Labels, _) => vec![Context::Browse, Context::Global],
        (_, Pane::Tasks | Pane::Projects) => vec![Context::List, Context::Global],
    }
}

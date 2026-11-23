//! The action registry and the keys built from it. Key dispatch, chords,
//! the which-key popup, the bottom bar, the cheatsheet, the palette and the
//! config all read bindings from [`ACTIONS`]; no key is bound anywhere else.

mod key;
mod legacy;
mod registry;

pub use key::{Key, KeySequence, ParseKeyError};
pub use legacy::{DetailScroll, KeyAction, handle_key, handle_key_while_loading};
pub use registry::{
    ACTIONS, ActionSpec, Availability, Group, Hint, HintRow, OverlayKind, Preset, STANDARD, by_id,
    spec,
};

/// Where a binding applies. Several contexts are active at once, most
/// specific first, and the first one that binds a key wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Context {
    /// Everywhere outside an overlay.
    Global,
    /// A task list: Inbox, Today, Upcoming, or a list opened from Projects,
    /// Labels or Analytics.
    List,
    /// A task's detail.
    Detail,
    /// The Projects and Labels screens.
    Browse,
    /// The Projects screen, ahead of [`Context::Browse`] there.
    Projects,
    /// The Analytics screen.
    Analytics,
    /// An overlay such as the cheatsheet, which takes every key.
    Overlay,
}

impl Context {
    pub const ALL: [Context; 7] = [
        Context::Global,
        Context::List,
        Context::Detail,
        Context::Browse,
        Context::Projects,
        Context::Analytics,
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
            Context::Analytics => "analytics",
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
            Context::Analytics => "Analytics",
            Context::Overlay => "Overlays",
        }
    }
}

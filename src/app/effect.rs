use crate::release::Record;
use crate::todoist::CompletedQuery;
use crate::todoist::sync::{SyncCommand, SyncRequest};

/// A request to the outside world. The runtime carries it out and, where
/// noted, sends the answer back as an [`super::Event`].
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Read from Todoist. Answered by [`super::Event::Synced`].
    Sync(SyncRequest),
    /// Write these commands, one flush. Answered by
    /// [`super::Event::CommandsSent`].
    SendCommands(Vec<SyncCommand>),
    /// Answered by [`super::Event::Comments`].
    FetchComments {
        task_id: String,
        fetch_seq: u64,
    },
    /// Answered by [`super::Event::CompletedTasks`].
    FetchCompletedTasks(CompletedQuery),
    /// Open the live-updates connection. Each message arrives as
    /// [`super::Event::Live`].
    OpenLive {
        url: String,
    },
    /// Write the store and the sync token to the cache, or delete it when
    /// there is nothing worth keeping. Nothing comes back.
    SaveCache,
    /// Delete the sync token versions before 0.8.0 kept, which the cache
    /// supersedes. Nothing comes back.
    ForgetOldSyncState,
    /// Ask Todoist who is signed in. Answered by
    /// [`super::Event::UserFetched`].
    FetchUser,
    /// Ask GitHub for the latest release. Answered by
    /// [`super::Event::ReleaseChecked`].
    CheckRelease,
    /// Write what the release checks found to the release cache. Nothing
    /// comes back.
    SaveRelease(Record),
    /// Keep the theme with this slug as `[theme] name` in `config.toml`.
    /// Answered by [`super::Event::ThemeSaved`], or by nothing when there is
    /// no config folder (`--demo`).
    SaveTheme(String),
    /// Open this page in the user's browser. Nothing comes back.
    OpenUrl(String),
    Quit,
}

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
    /// Save the sync token to disk. Nothing comes back.
    PersistSyncToken(String),
    /// Ask GitHub for the latest release. Answered by
    /// [`super::Event::ReleaseChecked`].
    CheckRelease,
    /// Write what the release checks found to the release cache. Nothing
    /// comes back.
    SaveRelease(Record),
    /// Open this page in the user's browser. Nothing comes back.
    OpenUrl(String),
    Quit,
}

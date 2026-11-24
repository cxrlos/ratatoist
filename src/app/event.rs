use crossterm::event::KeyEvent;
use semver::Version;

use crate::todoist::model::{Comment, Task};
use crate::todoist::sync::SyncResponse;
use crate::todoist::{ClientError, CompletedQuery, LiveMessage};

/// Everything that can change what ratatoist shows.
#[derive(Debug)]
pub enum Event {
    /// The runtime has started: the first sync begins.
    Start,
    Key(KeyEvent),
    /// Text pasted into the terminal, which bracketed paste delivers whole
    /// instead of as keys.
    Paste(String),
    Resize {
        width: u16,
        height: u16,
    },
    /// The clock moved on: the runtime sends the current time.
    Tick {
        now: chrono::DateTime<chrono::Local>,
    },
    /// The answer to [`super::Effect::Sync`].
    Synced(Result<Box<SyncResponse>, ClientError>),
    /// The answer to [`super::Effect::SendCommands`], for the commands with
    /// these uuids.
    CommandsSent {
        uuids: Vec<String>,
        result: Result<Box<SyncResponse>, ClientError>,
    },
    /// The answer to [`super::Effect::FetchComments`].
    Comments {
        task_id: String,
        fetch_seq: u64,
        result: Result<Vec<Comment>, ClientError>,
    },
    /// The answer to [`super::Effect::FetchCompletedTasks`].
    CompletedTasks {
        query: CompletedQuery,
        result: Result<Vec<Task>, ClientError>,
    },
    /// A message from the connection [`super::Effect::OpenLive`] opened.
    Live(LiveMessage),
    /// The answer to [`super::Effect::CheckRelease`]: the latest stable
    /// release, or `None` when the check failed, which the runtime logged.
    ReleaseChecked(Option<Version>),
    /// SIGINT, SIGTERM or SIGHUP: quit through the normal path.
    Shutdown,
}

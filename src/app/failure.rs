//! Failures the user is told about, and the messages log that keeps the most
//! recent of them.

use std::collections::VecDeque;
use std::fmt;

use chrono::{DateTime, Local};

/// Why the task lists cannot load at all, so the body shows an error
/// screen in their place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// Neither `TODOIST_API_TOKEN` nor the keyring holds a token.
    NoToken,
    /// Todoist refused the token the first sync sent.
    TokenRejected,
}

/// How many failures the messages log keeps; older ones drop off.
pub const ERROR_LOG_CAP: usize = 100;

/// What kind of failure it was, which decides its summary and the fix the
/// notice and the log suggest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Todoist could not be reached: no network, a timeout, a dropped
    /// connection.
    Offline,
    /// Todoist asked ratatoist to slow down, until this time when it said.
    RateLimited {
        until: Option<DateTime<Local>>,
    },
    /// Todoist refused the API token (401 or 403).
    TokenRejected,
    /// Todoist refused a command as invalid (a `sync_status` error).
    Validation,
    /// Todoist failed on its side (a 5xx).
    Server,
    Other,
}

/// Something ratatoist tried that did not work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: ErrorKind,
    /// What ratatoist was doing, as `complete "Renew passport photos"`.
    pub action: String,
    /// The full message, with its causes.
    pub message: String,
}

impl Failure {
    pub fn new(kind: ErrorKind, action: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            action: action.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// One failure in the messages log, with when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedFailure {
    pub at: DateTime<Local>,
    pub failure: Failure,
}

/// The most recent failures of this session, up to [`ERROR_LOG_CAP`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ErrorLog(VecDeque<LoggedFailure>);

impl ErrorLog {
    pub fn record(&mut self, at: DateTime<Local>, failure: Failure) {
        if self.0.len() == ERROR_LOG_CAP {
            self.0.pop_front();
        }
        self.0.push_back(LoggedFailure { at, failure });
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &LoggedFailure> {
        self.0.iter().rev()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

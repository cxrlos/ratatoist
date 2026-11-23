//! Failures the user is told about, and the messages log that keeps the most
//! recent of them.

use std::collections::VecDeque;
use std::fmt;

use chrono::{DateTime, Local};

use crate::todoist::ClientError;
use crate::todoist::sync::SyncCommandError;

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

impl ErrorKind {
    /// The kind of `error`, which happened at `at`: a rate limit's wait
    /// becomes the time it lifts.
    pub fn of(error: &ClientError, at: DateTime<Local>) -> Self {
        match error {
            ClientError::Unauthorized => Self::TokenRejected,
            ClientError::RateLimited { retry_after } => Self::RateLimited {
                until: retry_after
                    .and_then(|wait| chrono::Duration::from_std(wait).ok())
                    .and_then(|wait| at.checked_add_signed(wait)),
            },
            ClientError::Http { status, .. } => match status {
                401 | 403 => Self::TokenRejected,
                429 => Self::RateLimited { until: None },
                500..=599 => Self::Server,
                400..=499 => Self::Validation,
                _ => Self::Other,
            },
            ClientError::Network(_) => Self::Offline,
            ClientError::Decode(_) | ClientError::Other(_) => Self::Other,
        }
    }

    /// A short name, as the messages log lists it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::RateLimited { .. } => "rate limited",
            Self::TokenRejected => "token rejected",
            Self::Validation => "validation",
            Self::Server => "server error",
            Self::Other => "error",
        }
    }

    /// What went wrong, in one line.
    pub fn summary(&self) -> String {
        match self {
            Self::Offline => "Todoist could not be reached".to_owned(),
            Self::RateLimited { until: Some(until) } => format!(
                "Todoist's rate limit is used up until {}",
                until.format("%H:%M")
            ),
            Self::RateLimited { until: None } => "Todoist's rate limit is used up".to_owned(),
            Self::TokenRejected => "Todoist rejected the API token".to_owned(),
            Self::Validation => "Todoist refused the change".to_owned(),
            Self::Server => "Todoist failed on its side".to_owned(),
            Self::Other => "Something failed".to_owned(),
        }
    }

    /// What the user can do about it, in one line.
    pub fn fix(&self) -> &'static str {
        match self {
            Self::Offline => "Check the network; ratatoist syncs again when it is back.",
            Self::RateLimited { .. } => "Wait for it to lift, then try again.",
            Self::TokenRejected => "Run `ratatoist auth login` with a new token.",
            Self::Validation => "The message says why; change it and try again.",
            Self::Server => "Try again in a moment.",
            Self::Other => "The message says what went wrong.",
        }
    }
}

/// Something ratatoist tried that did not work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: ErrorKind,
    /// What ratatoist was doing, as `complete "Renew passport photos"`.
    pub action: String,
    /// When it failed.
    pub at: DateTime<Local>,
    /// The raw message, as Todoist or the client gave it.
    pub message: String,
}

impl Failure {
    pub fn new(
        kind: ErrorKind,
        action: impl Into<String>,
        message: impl Into<String>,
        at: DateTime<Local>,
    ) -> Self {
        Self {
            kind,
            action: action.into(),
            at,
            message: message.into(),
        }
    }

    /// `error`, which ended `action` at `at`.
    pub fn from_client(
        action: impl Into<String>,
        error: &ClientError,
        at: DateTime<Local>,
    ) -> Self {
        Self::new(ErrorKind::of(error, at), action, error.to_string(), at)
    }

    /// A command Todoist refused in its `sync_status`, with Todoist's
    /// message.
    pub fn from_command(
        action: impl Into<String>,
        error: &SyncCommandError,
        at: DateTime<Local>,
    ) -> Self {
        Self::new(ErrorKind::Validation, action, error.error.clone(), at)
    }

    /// The one line the notice row shows: what failed, why and the fix.
    pub fn notice(&self) -> String {
        format!(
            "Could not {}: {}. {}",
            self.action,
            self.kind.summary(),
            self.kind.fix()
        )
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// The most recent failures of this session, up to [`ERROR_LOG_CAP`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ErrorLog(VecDeque<Failure>);

impl ErrorLog {
    /// Adds `failure` as the newest entry, or, when the newest entry is the
    /// same failure (kind, action and message), moves that entry's time to
    /// `failure`'s, so a failure that repeats takes one entry.
    pub fn record(&mut self, failure: Failure) {
        if let Some(newest) = self.0.back_mut()
            && newest.kind == failure.kind
            && newest.action == failure.action
            && newest.message == failure.message
        {
            newest.at = failure.at;
            return;
        }
        if self.0.len() == ERROR_LOG_CAP {
            self.0.pop_front();
        }
        self.0.push_back(failure);
    }

    pub fn newest(&self) -> Option<&Failure> {
        self.0.back()
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &Failure> {
        self.0.iter().rev()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn the_log_keeps_only_the_newest_failures() {
        let at = Local
            .with_ymd_and_hms(2026, 11, 23, 9, 0, 0)
            .earliest()
            .unwrap();
        let mut log = ErrorLog::default();
        for n in 0..ERROR_LOG_CAP + 5 {
            log.record(Failure::new(ErrorKind::Other, "count", n.to_string(), at));
        }

        assert_eq!(log.len(), ERROR_LOG_CAP);
        let newest: Vec<&str> = log
            .newest_first()
            .map(|failure| failure.message.as_str())
            .take(2)
            .collect();
        assert_eq!(newest, ["104", "103"]);
        assert_eq!(
            log.newest_first().last().map(|f| f.message.as_str()),
            Some("5")
        );
    }
}

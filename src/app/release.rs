//! The release notice: when to check GitHub for a newer release, what the
//! answer shows, and skipping a release (ADR 0002).

use jiff::Timestamp;
use semver::Version;

use super::{Effect, State};
use crate::release::{self, Channel, Record};

/// What the app knows about newer releases, and whether it checks for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseState {
    /// Whether this session checks GitHub. Off under `--demo`, in tests
    /// and when the user turned it off.
    check: bool,
    channel: Channel,
    running: Version,
    record: Record,
}

impl Default for ReleaseState {
    /// Never checks and knows of no release.
    fn default() -> Self {
        Self::new(false, Channel::default(), Record::default())
    }
}

impl ReleaseState {
    /// Checking GitHub when `check`, for an install through `channel`, and
    /// knowing what the cache's `record` says.
    pub fn new(check: bool, channel: Channel, record: Record) -> Self {
        Self {
            check,
            channel,
            running: release::running(),
            record,
        }
    }

    /// The newer release the tab row announces: newer than the running
    /// version, not skipped, and on a channel that shows notices.
    pub fn notice(&self) -> Option<&Version> {
        let latest = self.record.latest.as_ref()?;
        let skipped = self
            .record
            .skipped
            .as_ref()
            .is_some_and(|skipped| skipped >= latest);
        (self.channel.shows_notice() && *latest > self.running && !skipped).then_some(latest)
    }

    /// How to upgrade the way ratatoist was installed, where it says so.
    pub fn upgrade(&self) -> Option<&'static str> {
        self.channel.upgrade()
    }
}

/// At each tick, a check when one is due. It counts as the day's check from
/// when it starts, so no second one starts while it runs, and a failed one
/// is tried again only a day later.
pub(super) fn tick(state: &mut State, now: Timestamp) -> Option<Effect> {
    let release = &mut state.release;
    if !release.check || !release.record.due(now) {
        return None;
    }
    release.record.checked_at = Some(now);
    Some(Effect::CheckRelease)
}

/// The answer to a check: the latest release, or `None` when the check
/// failed, which the runtime has logged and which shows nothing. Whether
/// the notice changed.
pub(super) fn checked(
    state: &mut State,
    latest: Option<Version>,
    effects: &mut Vec<Effect>,
) -> bool {
    let release = &mut state.release;
    let before = release.notice().cloned();
    if latest.is_some() {
        release.record.latest = latest;
    }
    effects.push(Effect::SaveRelease(release.record.clone()));
    release.notice() != before.as_ref()
}

/// Hides the notice until a release newer than the one it shows.
pub(super) fn skip(state: &mut State, effects: &mut Vec<Effect>) -> bool {
    let release = &mut state.release;
    let Some(version) = release.notice().cloned() else {
        return false;
    };
    release.record.skipped = Some(version);
    effects.push(Effect::SaveRelease(release.record.clone()));
    true
}

/// Opens the GitHub page of the release the notice shows.
pub(super) fn show_notes(state: &State, effects: &mut Vec<Effect>) -> bool {
    if let Some(version) = state.release.notice() {
        effects.push(Effect::OpenUrl(release::release_url(version)));
    }
    false
}

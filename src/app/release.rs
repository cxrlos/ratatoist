//! What the app knows about newer releases, for the notice in the tab row.

use semver::Version;

/// The newest stable release, when it is newer than the running one and
/// the user has not skipped it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseState {
    notice: Option<Version>,
}

impl ReleaseState {
    /// The version the tab row announces, if any.
    pub fn notice(&self) -> Option<&Version> {
        self.notice.as_ref()
    }
}

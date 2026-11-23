//! The OS keyring entry (service `ratatoist`), behind [`Keyring`] so tests use
//! an in-memory store.

use super::ApiToken;

pub trait Keyring {
    /// The stored token, or `None` when there is no entry.
    fn get(&self) -> anyhow::Result<Option<ApiToken>>;
    fn set(&self, token: &ApiToken) -> anyhow::Result<()>;
    /// Removes the entry; removing a missing entry is not an error.
    fn delete(&self) -> anyhow::Result<()>;
}

/// The platform keyring: the macOS Keychain, or the Secret Service on Linux.
#[derive(Debug, Default)]
pub struct OsKeyring;

impl Keyring for OsKeyring {
    fn get(&self) -> anyhow::Result<Option<ApiToken>> {
        Ok(None)
    }

    fn set(&self, _token: &ApiToken) -> anyhow::Result<()> {
        anyhow::bail!("the OS keyring is not wired up yet")
    }

    fn delete(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

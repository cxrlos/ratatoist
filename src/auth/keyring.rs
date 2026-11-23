//! The OS keyring entry (service `ratatoist`), behind [`Keyring`] so tests use
//! [`MemoryKeyring`].

use std::sync::{Arc, Mutex};

use keyring_core::{CredentialStore, Entry, Error};

use super::{ACCOUNT, ApiToken, SERVICE, TOKEN_VAR};

/// Somewhere to keep the one token. The binary only ever uses [`OsKeyring`].
pub trait Keyring {
    /// The stored token, or `None` when there is no entry.
    fn get(&self) -> Result<Option<ApiToken>, KeyringError>;
    fn set(&self, token: &ApiToken) -> Result<(), KeyringError>;
    /// Removes the entry. Returns whether there was one; removing a missing
    /// entry is not an error.
    fn delete(&self) -> Result<bool, KeyringError>;
}

/// The keyring could not be used. There is no fallback store, so the message
/// points at `TODOIST_API_TOKEN`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the OS keyring is unavailable ({reason}); set {TOKEN_VAR} to use ratatoist without it")]
pub struct KeyringError {
    reason: String,
}

impl KeyringError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl From<Error> for KeyringError {
    // Display only: the Debug of some variants (BadEncoding, BadDataFormat)
    // holds the raw secret bytes.
    fn from(error: Error) -> Self {
        Self::new(error.to_string())
    }
}

/// The platform keyring: the macOS Keychain, or the Secret Service on Linux
/// (GNOME Keyring, KWallet). It connects on each use, so a run that never
/// needs the keyring (`TODOIST_API_TOKEN` is set) works on a machine
/// without one.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeyring;

impl OsKeyring {
    fn entry(self) -> Result<Entry, KeyringError> {
        Ok(platform_store()?.build(SERVICE, ACCOUNT, None)?)
    }
}

#[cfg(target_os = "macos")]
fn platform_store() -> Result<Arc<CredentialStore>, KeyringError> {
    Ok(apple_native_keyring_store::keychain::Store::new()?)
}

#[cfg(target_os = "linux")]
fn platform_store() -> Result<Arc<CredentialStore>, KeyringError> {
    Ok(dbus_secret_service_keyring_store::Store::new()?)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_store() -> Result<Arc<CredentialStore>, KeyringError> {
    Err(KeyringError::new(
        "ratatoist supports the keyring on Linux and macOS only",
    ))
}

impl Keyring for OsKeyring {
    fn get(&self) -> Result<Option<ApiToken>, KeyringError> {
        match self.entry()?.get_password() {
            Ok(stored) => ApiToken::new(&stored)
                .map(Some)
                .map_err(|_| KeyringError::new("the stored entry is not a valid API token")),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn set(&self, token: &ApiToken) -> Result<(), KeyringError> {
        Ok(self.entry()?.set_password(token.expose())?)
    }

    fn delete(&self) -> Result<bool, KeyringError> {
        match self.entry()?.delete_credential() {
            Ok(()) => Ok(true),
            Err(Error::NoEntry) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

/// A keyring held in memory, for tests. Clones share the entry.
#[derive(Debug, Default, Clone)]
pub struct MemoryKeyring(Arc<Mutex<Option<ApiToken>>>);

impl MemoryKeyring {
    /// A keyring already holding `token`.
    pub fn holding(token: ApiToken) -> Self {
        Self(Arc::new(Mutex::new(Some(token))))
    }

    /// The stored token, without going through [`Keyring`].
    pub fn stored(&self) -> Option<ApiToken> {
        self.0.lock().ok().and_then(|entry| entry.clone())
    }

    fn entry(&self) -> Result<std::sync::MutexGuard<'_, Option<ApiToken>>, KeyringError> {
        self.0
            .lock()
            .map_err(|_| KeyringError::new("the in-memory keyring is poisoned"))
    }
}

impl Keyring for MemoryKeyring {
    fn get(&self) -> Result<Option<ApiToken>, KeyringError> {
        Ok(self.entry()?.clone())
    }

    fn set(&self, token: &ApiToken) -> Result<(), KeyringError> {
        *self.entry()? = Some(token.clone());
        Ok(())
    }

    fn delete(&self) -> Result<bool, KeyringError> {
        Ok(self.entry()?.take().is_some())
    }
}

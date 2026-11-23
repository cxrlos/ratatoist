//! Finding the Todoist token, and the `ratatoist auth` commands.

pub mod keyring;
pub mod token;

pub use token::{ApiToken, InvalidApiToken};

use crate::cli::AuthCommand;

/// Where a resolved token came from, for `ratatoist auth status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Env,
    Keyring,
    /// The plaintext `api_token` in `config.toml`, read until the keyring
    /// migration replaces it.
    ConfigFile,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub token: ApiToken,
    pub source: Source,
}

/// The token to use: `TODOIST_API_TOKEN`, then the keyring.
pub fn resolve() -> anyhow::Result<Resolved> {
    if let Ok(raw) = std::env::var("TODOIST_API_TOKEN")
        && !raw.trim().is_empty()
    {
        return Ok(Resolved {
            token: ApiToken::new(&raw)?,
            source: Source::Env,
        });
    }
    if let Some(token) = legacy_config_token()? {
        return Ok(Resolved {
            token,
            source: Source::ConfigFile,
        });
    }
    anyhow::bail!("No Todoist token: run \"ratatoist auth login\", or set TODOIST_API_TOKEN.")
}

/// Runs `ratatoist auth login | status | logout`.
pub async fn run(command: AuthCommand) -> anyhow::Result<()> {
    let _ = command;
    anyhow::bail!("ratatoist auth is not built yet")
}

fn legacy_config_token() -> anyhow::Result<Option<ApiToken>> {
    #[derive(serde::Deserialize)]
    struct File {
        api_token: Option<String>,
    }
    let path = crate::paths::config_dir()?.join("config.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(file) = toml::from_str::<File>(&text) else {
        return Ok(None);
    };
    match file.api_token {
        Some(raw) if !raw.trim().is_empty() => Ok(Some(ApiToken::new(&raw)?)),
        _ => Ok(None),
    }
}

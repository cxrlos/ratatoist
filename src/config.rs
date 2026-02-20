use std::fmt;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct ConfigFile {
    api_token: Option<String>,
}

pub struct Config {
    pub(crate) api_token: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        if let Ok(token) = std::env::var("TODOIST_API_TOKEN")
            && !token.is_empty()
        {
            return Ok(Self { api_token: token });
        }

        let path = Self::config_path();
        if path.exists() {
            Self::check_file_permissions(&path)?;
            let contents = std::fs::read_to_string(&path).context("failed to read config file")?;
            let file: ConfigFile =
                toml::from_str(&contents).context("failed to parse config file")?;
            if let Some(token) = file.api_token
                && !token.is_empty()
            {
                return Ok(Self { api_token: token });
            }
        }

        anyhow::bail!(
            "No Todoist API token found.\n\n\
             Set it in one of two ways:\n\
             1. Environment variable:\n\
             \x20  export TODOIST_API_TOKEN=\"your-token-here\"\n\n\
             2. Config file at {}:\n\
             \x20  api_token = \"your-token-here\"\n\n\
             Get your token from https://app.todoist.com/app/settings/integrations",
            Self::config_path().display()
        )
    }

    pub fn token(&self) -> &str {
        &self.api_token
    }

    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("ratatoist")
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    #[cfg(unix)]
    fn check_file_permissions(path: &std::path::Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path).context("failed to read config file metadata")?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            anyhow::bail!(
                "Config file {} has insecure permissions ({:o}).\n\
                 It contains your API token and should be owner-readable only.\n\
                 Fix with: chmod 600 {}",
                path.display(),
                mode,
                path.display()
            );
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn check_file_permissions(_path: &std::path::Path) -> Result<()> {
        Ok(())
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("api_token", &"[REDACTED]")
            .finish()
    }
}

//! Where ratatoist keeps its files: XDG base directories on Linux and on
//! macOS alike, so `~/.config/ratatoist` is the config folder everywhere.

use std::path::PathBuf;

use etcetera::BaseStrategy;
use etcetera::base_strategy::Xdg;

const APP: &str = "ratatoist";

#[derive(Debug, thiserror::Error)]
#[error("cannot find the home directory, so ratatoist has nowhere to keep its files")]
pub struct NoHome;

/// `$XDG_CONFIG_HOME/ratatoist`, by default `~/.config/ratatoist`.
pub fn config_dir() -> Result<PathBuf, NoHome> {
    Ok(xdg()?.config_dir().join(APP))
}

/// `$XDG_STATE_HOME/ratatoist`, by default `~/.local/state/ratatoist`.
pub fn state_dir() -> Result<PathBuf, NoHome> {
    let xdg = xdg()?;
    let base = xdg
        .state_dir()
        .unwrap_or_else(|| xdg.home_dir().join(".local/state"));
    Ok(base.join(APP))
}

/// `$XDG_CACHE_HOME/ratatoist`, by default `~/.cache/ratatoist`.
pub fn cache_dir() -> Result<PathBuf, NoHome> {
    Ok(xdg()?.cache_dir().join(APP))
}

fn xdg() -> Result<Xdg, NoHome> {
    Xdg::new().map_err(|_| NoHome)
}

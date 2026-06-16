use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncState {
    pub sync_token: String,
}

impl SyncState {
    pub fn load(config_dir: &Path) -> Self {
        if let Ok(src) = std::fs::read_to_string(Self::path(config_dir))
            && let Ok(state) = serde_json::from_str::<SyncState>(&src)
        {
            return state;
        }
        Self {
            sync_token: "*".to_string(),
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let path = Self::path(config_dir);
        std::fs::write(&path, serde_json::to_string(self)?)?;
        Ok(())
    }

    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("sync_state.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ratatoist-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_defaults_to_star_when_missing() {
        let dir = temp_dir("missing");
        let _ = std::fs::remove_file(SyncState::path(&dir));
        assert_eq!(SyncState::load(&dir).sync_token, "*");
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_dir("roundtrip");
        let state = SyncState {
            sync_token: "abc123".to_string(),
        };
        state.save(&dir).unwrap();
        assert_eq!(SyncState::load(&dir).sync_token, "abc123");
    }

    #[test]
    fn load_falls_back_to_star_on_corrupt_json() {
        let dir = temp_dir("corrupt");
        std::fs::write(SyncState::path(&dir), "{ not valid json").unwrap();
        assert_eq!(SyncState::load(&dir).sync_token, "*");
    }
}

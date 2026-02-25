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

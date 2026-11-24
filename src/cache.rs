//! The local cache: the store and its sync token in `cache.json` in the
//! cache folder, so a start paints the last known tasks at once and asks
//! Todoist only for what changed since (ADR 0003).

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::app::{State, Store};

const FILE: &str = "cache.json";

/// Where versions before 0.8.0 kept the sync token alone, in the config
/// folder. The cache supersedes it.
const OLD_SYNC_STATE: &str = "sync_state.json";

/// Bumped whenever the shape of [`Cache`] or of the store changes, so an
/// older file is discarded instead of half read.
pub const SCHEMA_VERSION: u32 = 1;

/// What `cache.json` holds. Never the API token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    pub version: u32,
    /// The Todoist user the store belongs to.
    pub user_id: String,
    pub sync_token: String,
    pub store: Store,
}

impl Cache {
    /// What `state` would leave in the cache: `None` before a sync has
    /// answered with the user, or after `R` asked for a full sync, when there
    /// is nothing worth keeping.
    pub fn of(state: &State) -> Option<Self> {
        let user_id = state.store.current_user_id.clone()?;
        if state.sync_token == "*" || state.sync_token.is_empty() {
            return None;
        }
        Some(Self {
            version: SCHEMA_VERSION,
            user_id,
            sync_token: state.sync_token.clone(),
            store: state.store.clone(),
        })
    }

    /// The cache in `dir`, or `None` when it is missing, unreadable, from
    /// another schema version or has no sync token to resume from.
    pub fn load(dir: &Path) -> Option<Self> {
        let src = std::fs::read(Self::path(dir)).ok()?;
        let cache: Self = serde_json::from_slice(&src).ok()?;
        let usable = cache.version == SCHEMA_VERSION
            && !cache.user_id.is_empty()
            && cache.sync_token != "*"
            && !cache.sync_token.is_empty();
        usable.then_some(cache)
    }

    /// Writes the cache atomically, readable by the user alone, creating
    /// `dir` first. The temporary file carries the process id, so two runs
    /// at once never share one, and is removed when the write fails.
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create {}", dir.display()))?;
        let tmp = dir.join(format!("{FILE}.ratatoist-{}.tmp", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        let written = self.write_then_rename(&tmp, &Self::path(dir));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        written
    }

    fn write_then_rename(&self, tmp: &Path, path: &Path) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(tmp)
            .with_context(|| format!("could not create {}", tmp.display()))?;
        file.write_all(&serde_json::to_vec(self)?)?;
        file.sync_all()?;
        std::fs::rename(tmp, path).with_context(|| format!("could not replace {}", path.display()))
    }

    /// Deletes the cache in `dir`, if there is one.
    pub fn remove(dir: &Path) -> io::Result<()> {
        remove_if_there(&Self::path(dir))
    }

    pub fn path(dir: &Path) -> PathBuf {
        dir.join(FILE)
    }
}

/// Deletes the sync token older versions kept in `config_dir`, if it is
/// there.
pub fn remove_old_sync_state(config_dir: &Path) -> io::Result<()> {
    remove_if_there(&old_sync_state_path(config_dir))
}

pub fn old_sync_state_path(config_dir: &Path) -> PathBuf {
    config_dir.join(OLD_SYNC_STATE)
}

fn remove_if_there(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::todoist::model::{Reminder, Task, UserInfo};
    use crate::todoist::sync::SyncResponse;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cache() -> Cache {
        let mut store = Store::default();
        store.merge(
            SyncResponse {
                items: Some(vec![Task {
                    id: "t1".to_owned(),
                    content: "Buy oat milk".to_owned(),
                    ..Task::default()
                }]),
                reminders: Some(vec![Reminder {
                    id: "r1".to_owned(),
                    item_id: "t1".to_owned(),
                    minute_offset: Some(30),
                    ..Reminder::default()
                }]),
                user: Some(UserInfo {
                    id: "u1".to_owned(),
                    ..UserInfo::default()
                }),
                ..SyncResponse::empty(true)
            },
            |_| false,
        );
        Cache {
            version: SCHEMA_VERSION,
            user_id: "u1".to_owned(),
            sync_token: "token-1".to_owned(),
            store,
        }
    }

    #[test]
    fn a_saved_cache_loads_back_and_only_its_owner_can_read_it() {
        let dir = temp_dir("roundtrip");
        cache().save(&dir).unwrap();

        let mode = std::fs::metadata(Cache::path(&dir))
            .unwrap()
            .permissions()
            .mode();
        let loaded = Cache::load(&dir).unwrap();
        let leftovers = std::fs::read_dir(&dir).unwrap().count();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(loaded.user_id, "u1");
        assert_eq!(loaded.sync_token, "token-1");
        assert_eq!(
            loaded.store.tasks.by_id("t1").map(|t| t.content.as_str()),
            Some("Buy oat milk")
        );
        assert_eq!(
            loaded.store.reminders.by_id("r1").map(|r| r.minute_offset),
            Some(Some(30))
        );
        assert_eq!(leftovers, 1, "only cache.json is left in the folder");
    }

    #[test]
    fn saving_leaves_another_runs_temporary_file_alone() {
        let dir = temp_dir("other-run");
        let other = dir.join("cache.json.tmp");
        std::fs::write(&other, "another run is writing").unwrap();

        cache().save(&dir).unwrap();

        let left = std::fs::read_to_string(&other);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left.unwrap(), "another run is writing");
    }

    #[test]
    fn a_failed_save_leaves_no_temporary_file() {
        let dir = temp_dir("failed-save");
        // A folder where cache.json goes makes the rename fail.
        std::fs::create_dir_all(Cache::path(&dir).join("in-the-way")).unwrap();

        let saved = cache().save(&dir);

        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(saved.is_err());
        assert_eq!(names, ["cache.json"]);
    }

    #[test]
    fn saving_creates_a_missing_folder() {
        let dir = temp_dir("no-folder").join("cache").join("ratatoist");
        cache().save(&dir).unwrap();

        assert!(Cache::load(&dir).is_some());
    }

    #[test]
    fn a_missing_corrupt_or_other_version_cache_is_discarded() {
        let dir = temp_dir("discard");
        assert!(Cache::load(&dir).is_none(), "missing");

        std::fs::write(Cache::path(&dir), "{ not valid json").unwrap();
        assert!(Cache::load(&dir).is_none(), "corrupt");

        Cache {
            version: SCHEMA_VERSION + 1,
            ..cache()
        }
        .save(&dir)
        .unwrap();
        assert!(Cache::load(&dir).is_none(), "another schema version");

        Cache {
            sync_token: "*".to_owned(),
            ..cache()
        }
        .save(&dir)
        .unwrap();
        assert!(Cache::load(&dir).is_none(), "no token to resume from");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_file_holds_no_pretty_printing() {
        let dir = temp_dir("compact");
        cache().save(&dir).unwrap();

        let text = std::fs::read_to_string(Cache::path(&dir)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(!text.contains('\n'));
    }

    #[test]
    fn a_state_before_its_first_answer_or_after_r_leaves_nothing_to_cache() {
        let now = jiff::civil::date(2026, 11, 22)
            .at(9, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .unwrap();
        let mut state = State::new(now);
        assert!(Cache::of(&state).is_none());

        let cached = cache();
        state.store = cached.store;
        state.sync_token = cached.sync_token;
        assert_eq!(Cache::of(&state).map(|c| c.user_id).as_deref(), Some("u1"));

        state.sync_token = "*".to_owned();
        assert!(Cache::of(&state).is_none());
    }

    #[test]
    fn removing_what_is_not_there_is_fine() {
        let dir = temp_dir("remove");
        std::fs::write(old_sync_state_path(&dir), r#"{"sync_token":"abc"}"#).unwrap();

        remove_old_sync_state(&dir).unwrap();
        remove_old_sync_state(&dir).unwrap();
        Cache::remove(&dir).unwrap();
        let left = old_sync_state_path(&dir).exists();
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(!left);
    }
}

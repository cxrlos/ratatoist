//! The release check (ADR 0002): GitHub's latest stable ratatoist release,
//! how ratatoist was installed, and the cache that keeps the check to once
//! a day.
//!
//! The request carries no token and no account data: only ratatoist's
//! version in the User-Agent.

use std::ffi::OsStr;
use std::fs::{self, DirBuilder};
use std::io::ErrorKind;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::header::ACCEPT;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::config::UpdateSettings;
use crate::paths;

/// Where GitHub answers with the latest stable release. It skips drafts and
/// pre-releases.
pub const LATEST_URL: &str = "https://api.github.com/repos/cxrlos/ratatoist/releases/latest";

/// Set to any non-empty value, it turns the check off.
pub const OFF_VAR: &str = "RATATOIST_NO_UPDATE_CHECK";

/// How long a check holds before the next one is due.
pub const CHECK_EVERY: TimeDelta = TimeDelta::hours(24);

/// How long one check may take, connecting included.
const TIMEOUT: Duration = Duration::from_secs(10);

const FILE: &str = "release.json";

/// How ratatoist was installed, as the running binary's path tells it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// A Homebrew Cellar.
    Homebrew,
    /// A system bin directory, where only the package manager installs; on
    /// Arch that is the AUR package.
    Aur,
    /// The Nix store.
    Nix,
    /// The cargo bin dir, where `cargo install`, `cargo binstall` and the
    /// shell installer all put ratatoist without a receipt, or anywhere
    /// else.
    #[default]
    CargoBin,
}

impl Channel {
    /// The channel of the binary at `path`, which should have its symlinks
    /// resolved, since Homebrew links its binaries out of the Cellar.
    pub fn of(path: &Path) -> Self {
        if path.starts_with("/nix/store") {
            return Self::Nix;
        }
        if path.components().any(|part| part.as_os_str() == "Cellar") {
            return Self::Homebrew;
        }
        if ["/usr/bin", "/usr/sbin", "/bin"]
            .iter()
            .any(|dir| path.parent() == Some(Path::new(dir)))
        {
            return Self::Aur;
        }
        Self::CargoBin
    }

    /// The channel of the running binary, or [`Channel::CargoBin`] when its
    /// path cannot be read.
    pub fn of_running() -> Self {
        std::env::current_exe()
            .and_then(fs::canonicalize)
            .map(|path| Self::of(&path))
            .unwrap_or_default()
    }

    /// Whether a newer release shows as a notice. The AUR and Nix ship a
    /// release when they package it, which may be later, so they never do.
    pub fn shows_notice(self) -> bool {
        matches!(self, Self::Homebrew | Self::CargoBin)
    }

    /// How to upgrade an install of this channel, where ratatoist says it:
    /// the AUR and Nix own their updates, so they get no hint.
    pub fn upgrade(self) -> Option<&'static str> {
        match self {
            Self::Homebrew => Some("brew upgrade ratatoist"),
            Self::CargoBin => Some("see https://github.com/cxrlos/ratatoist#quick-start"),
            Self::Aur | Self::Nix => None,
        }
    }
}

/// The version ratatoist runs as.
pub fn running() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// The stable version a release tag names, as `v0.7.1`. A pre-release tag
/// names none, so it never counts as newer.
pub fn tag_version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .ok()
        .filter(|version| version.pre.is_empty())
}

/// The GitHub page of `version`'s release.
pub fn release_url(version: &Version) -> String {
    format!("https://github.com/cxrlos/ratatoist/releases/tag/v{version}")
}

/// What turned the check off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    /// `[updates] check = false`.
    Config,
    /// [`OFF_VAR`] is set.
    Env,
}

/// What turns the check off, if anything: `settings`, or `env`, the value
/// of [`OFF_VAR`], when it is not empty.
pub fn off_switch(settings: &UpdateSettings, env: Option<&OsStr>) -> Option<Off> {
    if env.is_some_and(|value| !value.is_empty()) {
        Some(Off::Env)
    } else if !settings.check {
        Some(Off::Config)
    } else {
        None
    }
}

/// Whether this run checks in the background: only the TUI on a real
/// account, with nothing turning the check off. `--demo` and the `auth`
/// commands never do.
pub fn checks_in_background(cli: &Cli, off: Option<Off>) -> bool {
    cli.command.is_none() && !cli.demo && off.is_none()
}

/// The background check this run makes: GitHub and the cache when
/// [`checks_in_background`] says so for `cli`, `settings` and `env` (the
/// value of [`OFF_VAR`]), otherwise `None`. Every start goes through it,
/// `--demo` included, so this is what keeps the demo off GitHub. Also
/// `None`, logged, when there is no cache dir or HTTP client.
pub fn background_check(
    cli: &Cli,
    settings: &UpdateSettings,
    env: Option<&OsStr>,
) -> Option<ReleaseCheck> {
    let off = off_switch(settings, env);
    if !checks_in_background(cli, off) {
        tracing::info!(?off, "not checking for a newer release");
        return None;
    }
    ReleaseCheck::github()
        .inspect_err(|error| tracing::warn!("no release check: {error:#}"))
        .ok()
}

/// What the last checks found: when the last one ran, the newest release
/// it saw and the release the user skipped.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Record {
    pub checked_at: Option<DateTime<Utc>>,
    pub latest: Option<Version>,
    pub skipped: Option<Version>,
}

impl Record {
    /// Whether a check is due at `now`: never checked, a day after the
    /// last one, or the clock was set back past it.
    pub fn due(&self, now: DateTime<Utc>) -> bool {
        self.checked_at.is_none_or(|checked| {
            let since = now - checked;
            since >= CHECK_EVERY || since < TimeDelta::zero()
        })
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    checked_at: Option<DateTime<Utc>>,
    latest: Option<String>,
    skipped: Option<String>,
}

/// The cache file of the release check, under the XDG cache dir.
#[derive(Debug, Clone)]
pub struct ReleaseCache {
    path: PathBuf,
}

impl ReleaseCache {
    /// The cache file `release.json` in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            path: dir.into().join(FILE),
        }
    }

    /// `release.json` in [`paths::cache_dir`] (by default
    /// `~/.cache/ratatoist/release.json`, on macOS too).
    pub fn in_cache_dir() -> anyhow::Result<Self> {
        Ok(Self::new(paths::cache_dir()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The temporary file [`Self::save`] writes before the rename, named
    /// for this process so two sessions saving at once never share one.
    fn tmp_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(format!(".{}.tmp", std::process::id()));
        PathBuf::from(name)
    }

    /// What the last checks found, or an empty record when there is no
    /// file or it cannot be read, which only makes the next check due.
    pub fn load(&self) -> Record {
        let file = match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<File>(&bytes)
                .with_context(|| format!("decoding {}", self.path.display())),
            Err(error) if error.kind() == ErrorKind::NotFound => return Record::default(),
            Err(error) => Err(error).with_context(|| format!("reading {}", self.path.display())),
        };
        match file {
            Ok(file) => Record {
                checked_at: file.checked_at,
                latest: file.latest.as_deref().and_then(tag_version),
                skipped: file.skipped.as_deref().and_then(tag_version),
            },
            Err(error) => {
                tracing::warn!("discarding the release cache: {error:#}");
                Record::default()
            }
        }
    }

    /// Writes `record` atomically: to a temporary file beside the cache,
    /// then renamed over it, so a crash leaves the old file or the new one.
    pub fn save(&self, record: &Record) -> anyhow::Result<()> {
        let file = File {
            checked_at: record.checked_at,
            latest: record.latest.as_ref().map(ToString::to_string),
            skipped: record.skipped.as_ref().map(ToString::to_string),
        };
        let json = serde_json::to_vec(&file).context("encoding the release cache")?;
        if let Some(dir) = self.path.parent() {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = self.tmp_path();
        fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &self.path).with_context(|| format!("replacing {}", self.path.display()))
    }
}

/// Where the latest release is asked for: GitHub, or a fake server in
/// tests.
#[derive(Debug, Clone)]
pub struct ReleaseSource {
    url: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
}

impl ReleaseSource {
    /// GitHub's latest release of ratatoist.
    pub fn github() -> anyhow::Result<Self> {
        Self::new(LATEST_URL)
    }

    /// The latest release as `url` answers it, in GitHub's shape.
    pub fn new(url: impl Into<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("ratatoist/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            url: url.into(),
            http,
        })
    }

    /// The version of the latest stable release.
    pub async fn latest(&self) -> anyhow::Result<Version> {
        let response = self
            .http
            .get(&self.url)
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .context("could not reach GitHub")?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("GitHub answered {status}");
        }
        let body = response
            .bytes()
            .await
            .context("could not read GitHub's answer")?;
        let latest: Latest =
            serde_json::from_slice(&body).context("could not read GitHub's answer")?;
        tag_version(&latest.tag_name)
            .with_context(|| format!("the release tag {} is not a version", latest.tag_name))
    }
}

/// Everything the background check needs: where to ask and where to keep
/// what it found.
#[derive(Debug, Clone)]
pub struct ReleaseCheck {
    pub source: ReleaseSource,
    pub cache: ReleaseCache,
}

impl ReleaseCheck {
    /// GitHub and the cache under the XDG cache dir.
    pub fn github() -> anyhow::Result<Self> {
        Ok(Self {
            source: ReleaseSource::github()?,
            cache: ReleaseCache::in_cache_dir()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use chrono::TimeZone;
    use clap::Parser;
    use semver::Version;

    use super::*;
    use crate::cli::Cli;
    use crate::config::UpdateSettings;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn a_homebrew_cellar_path_is_homebrew() {
        for path in [
            "/opt/homebrew/Cellar/ratatoist/0.7.0/bin/ratatoist",
            "/usr/local/Cellar/ratatoist/0.7.0/bin/ratatoist",
            "/home/linuxbrew/.linuxbrew/Cellar/ratatoist/0.7.0/bin/ratatoist",
        ] {
            assert_eq!(Channel::of(Path::new(path)), Channel::Homebrew);
        }
    }

    #[test]
    fn a_system_bin_path_is_the_aur() {
        for path in [
            "/usr/bin/ratatoist",
            "/usr/sbin/ratatoist",
            "/bin/ratatoist",
        ] {
            assert_eq!(Channel::of(Path::new(path)), Channel::Aur);
        }
    }

    #[test]
    fn a_nix_store_path_is_nix() {
        let path = Path::new("/nix/store/abc123-ratatoist-0.7.0/bin/ratatoist");
        assert_eq!(Channel::of(path), Channel::Nix);
    }

    #[test]
    fn anything_else_is_the_cargo_bin_dir() {
        for path in [
            "/home/remy/.cargo/bin/ratatoist",
            "/Users/remy/.cargo/bin/ratatoist",
            "/usr/local/bin/ratatoist",
            "/home/remy/bin/ratatoist",
            "ratatoist",
        ] {
            assert_eq!(Channel::of(Path::new(path)), Channel::CargoBin);
        }
    }

    #[test]
    fn only_homebrew_and_the_cargo_bin_dir_show_the_notice() {
        assert!(Channel::Homebrew.shows_notice());
        assert!(Channel::CargoBin.shows_notice());
        assert!(!Channel::Aur.shows_notice());
        assert!(!Channel::Nix.shows_notice());
    }

    #[test]
    fn a_tag_is_its_version_with_or_without_a_v() {
        assert_eq!(tag_version("v0.7.1"), Some(version("0.7.1")));
        assert_eq!(tag_version("0.7.1"), Some(version("0.7.1")));
        assert_eq!(tag_version("nightly"), None);
        assert_eq!(tag_version("v0.8.0-rc.1"), None);
    }

    #[test]
    fn the_running_version_is_the_crate_version() {
        assert_eq!(running(), version(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn the_config_or_any_non_empty_env_value_switches_the_check_off() {
        let on = UpdateSettings { check: true };
        let off = UpdateSettings { check: false };
        assert_eq!(off_switch(&on, None), None);
        assert_eq!(off_switch(&on, Some(OsStr::new(""))), None);
        assert_eq!(off_switch(&on, Some(OsStr::new("1"))), Some(Off::Env));
        assert_eq!(off_switch(&on, Some(OsStr::new("no"))), Some(Off::Env));
        assert_eq!(off_switch(&off, None), Some(Off::Config));
    }

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn only_the_tui_on_a_real_account_checks_in_the_background() {
        assert!(checks_in_background(&cli(&["ratatoist"]), None));
        assert!(!checks_in_background(
            &cli(&["ratatoist"]),
            Some(Off::Config)
        ));
        assert!(!checks_in_background(&cli(&["ratatoist"]), Some(Off::Env)));
        assert!(!checks_in_background(&cli(&["ratatoist", "--demo"]), None));
        for auth in [
            &["ratatoist", "auth", "status"][..],
            &["ratatoist", "auth", "login"],
            &["ratatoist", "auth", "logout"],
        ] {
            assert!(!checks_in_background(&cli(auth), None));
        }
    }

    #[test]
    fn only_the_tui_on_a_real_account_gets_a_background_check() {
        let on = UpdateSettings { check: true };
        let off = UpdateSettings { check: false };
        assert!(background_check(&cli(&["ratatoist"]), &on, None).is_some());
        assert!(background_check(&cli(&["ratatoist", "--demo"]), &on, None).is_none());
        assert!(background_check(&cli(&["ratatoist", "auth", "status"]), &on, None).is_none());
        assert!(background_check(&cli(&["ratatoist"]), &off, None).is_none());
        assert!(background_check(&cli(&["ratatoist"]), &on, Some(OsStr::new("1"))).is_none());
    }

    #[test]
    fn a_check_is_due_a_day_after_the_last_one() {
        let at = |day, hour, minute, second| {
            Utc.with_ymd_and_hms(2026, 11, day, hour, minute, second)
                .unwrap()
        };
        let record = Record {
            checked_at: Some(at(18, 9, 0, 0)),
            ..Record::default()
        };
        assert!(Record::default().due(at(18, 9, 0, 0)));
        assert!(!record.due(at(18, 9, 0, 1)));
        assert!(!record.due(at(19, 8, 59, 59)));
        assert!(record.due(at(19, 9, 0, 0)));
        // A clock set back past the last check does not wait a day more.
        assert!(record.due(at(17, 9, 0, 0)));
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-release-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_cache_keeps_the_check_time_latest_and_skipped_versions() {
        let cache = ReleaseCache::new(scratch_dir("round-trip"));
        let record = Record {
            checked_at: Some(Utc.with_ymd_and_hms(2026, 11, 18, 9, 0, 0).unwrap()),
            latest: Some(version("0.7.1")),
            skipped: Some(version("0.7.1")),
        };

        cache.save(&record).unwrap();

        assert_eq!(cache.load(), record);
        let json = std::fs::read_to_string(cache.path()).unwrap();
        assert_eq!(
            json,
            r#"{"checked_at":"2026-11-18T09:00:00Z","latest":"0.7.1","skipped":"0.7.1"}"#
        );
        let dir = cache.path().parent().unwrap();
        let names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["release.json"]);
    }

    #[test]
    fn each_process_saves_through_its_own_temporary_file() {
        let cache = ReleaseCache::new(scratch_dir("temporary"));
        let tmp = cache.tmp_path();
        assert_eq!(tmp.parent(), cache.path().parent());
        assert_eq!(
            tmp.file_name().unwrap().to_string_lossy(),
            format!("release.json.{}.tmp", std::process::id())
        );
    }

    #[test]
    fn a_missing_or_broken_cache_is_an_empty_record() {
        let cache = ReleaseCache::new(scratch_dir("broken"));
        assert_eq!(cache.load(), Record::default());

        std::fs::create_dir_all(cache.path().parent().unwrap()).unwrap();
        std::fs::write(cache.path(), "not json").unwrap();
        assert_eq!(cache.load(), Record::default());
    }
}

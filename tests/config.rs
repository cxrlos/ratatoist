// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ratatoist::auth::{self, ApiToken, MemoryKeyring, Source};
use ratatoist::config::{self, Config};

/// A fresh config folder for one test, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-config-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// A config folder holding `config.toml` with `text`.
    fn with_config(test: &str, text: &str) -> Self {
        let dir = Self::new(test);
        fs::write(dir.0.join("config.toml"), text).unwrap();
        dir
    }

    fn theme(&self, file: &str, json: &str) {
        fs::create_dir_all(self.0.join("themes")).unwrap();
        fs::write(self.0.join("themes").join(file), json).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = make_writable(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_writable(dir: &Path) -> std::io::Result<()> {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755))
}

fn load(dir: &TempDir) -> Config {
    config::load(&dir.0)
}

const MINE: &str = r##"{
  "name": "Mine",
  "base00": "#101010", "base01": "#202020", "base02": "#303030", "base03": "#404040",
  "base04": "#b0b0b0", "base05": "#e0e0e0", "base06": "#f0f0f0", "base07": "#ffffff",
  "base08": "#ff5555", "base09": "#ffaa55", "base0A": "#ffff55", "base0B": "#55ff55",
  "base0C": "#55ffff", "base0D": "#5555ff", "base0E": "#ff55ff", "base0F": "#aa5500"
}"##;

#[test]
fn a_missing_file_is_the_default_config() {
    let dir = TempDir::new("missing");

    let config = load(&dir);

    assert_eq!(config.warnings, Vec::<String>::new());
    assert_eq!(config.theme, None);
    assert_eq!(config.idle_timeout, None);
    assert!(config.user_themes.is_empty());
}

#[test]
fn every_section_is_read() {
    let dir = TempDir::with_config(
        "sections",
        "[ui]\n\n[keys]\n\n[theme]\nname = \"Nord\"\n\n[sync]\nidle-timeout = 120\n",
    );

    let config = load(&dir);

    assert_eq!(config.warnings, Vec::<String>::new());
    assert_eq!(config.theme.as_deref(), Some("Nord"));
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(120)));
}

#[test]
fn the_wide_layout_width_is_read_from_ui() {
    let config = config::parse("[ui]\nwide-layout-columns = 140\n", &[]);

    assert_eq!(config.warnings, Vec::<String>::new());
    assert_eq!(config.wide_layout_columns, Some(140));
}

#[test]
fn a_bad_wide_layout_width_warns_and_keeps_the_default() {
    let config = config::parse("[ui]\nwide-layout-columns = \"wide\"\n", &[]);

    assert_eq!(config.wide_layout_columns, None);
    assert_eq!(config.warnings.len(), 1);
    assert!(config.warnings[0].contains("ui.wide-layout-columns"));
}

#[test]
fn a_theme_is_found_by_its_slug_and_kept_by_its_name() {
    let dir = TempDir::with_config("slug", "[theme]\nname = \"rose-pine\"\n");

    assert_eq!(load(&dir).theme.as_deref(), Some("Rose Pine"));
}

#[test]
fn a_syntax_error_is_reported_with_line_and_column_and_the_file_is_untouched() {
    let text = "[theme]\nname = \"Nord\"\n\n[sync]\nidle-timeout = = 5\n";
    let dir = TempDir::with_config("syntax", text);

    let config = load(&dir);

    assert_eq!(config.warnings.len(), 1, "{:?}", config.warnings);
    assert!(
        config.warnings[0].starts_with("config.toml:5:"),
        "{:?}",
        config.warnings
    );
    assert!(config.warnings[0].contains("[sync] uses its defaults"));
    assert_eq!(config.theme.as_deref(), Some("Nord"));
    assert_eq!(config.idle_timeout, None);
    assert_eq!(fs::read_to_string(dir.0.join("config.toml")).unwrap(), text);
}

#[test]
fn a_syntax_error_before_any_section_drops_the_whole_file() {
    let dir = TempDir::with_config("toplevel", "this is not toml\n[theme]\nname = \"Nord\"\n");

    let config = load(&dir);

    assert_eq!(config.warnings.len(), 1, "{:?}", config.warnings);
    assert!(config.warnings[0].starts_with("config.toml:1:"));
    assert!(config.warnings[0].contains("using the default config"));
    assert_eq!(config.theme, None);
}

#[test]
fn an_unknown_key_warns_with_its_line_and_column() {
    let dir = TempDir::with_config("unknown", "[sync]\nidle-timeout = 60\n  colour = 1\n");

    let config = load(&dir);

    assert_eq!(
        config.warnings,
        vec!["config.toml:3:3: unknown key `colour` in [sync] is ignored".to_owned()]
    );
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(60)));
}

#[test]
fn an_unknown_table_warns() {
    let dir = TempDir::with_config("table", "[colors]\nred = 1\n");

    let config = load(&dir);

    assert_eq!(config.warnings.len(), 1);
    assert!(
        config.warnings[0].starts_with("config.toml:1:2: unknown table `[colors]`"),
        "{:?}",
        config.warnings
    );
}

#[test]
fn a_bad_theme_value_falls_back_while_sync_still_applies() {
    let dir = TempDir::with_config(
        "badtheme",
        "[theme]\nname = 5\n\n[sync]\nidle-timeout = 90\n",
    );

    let config = load(&dir);

    assert_eq!(config.warnings.len(), 1);
    assert!(
        config.warnings[0].starts_with("config.toml:2:8: "),
        "{:?}",
        config.warnings
    );
    assert!(config.warnings[0].contains("[theme] uses its defaults"));
    assert_eq!(config.theme, None);
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(90)));
}

#[test]
fn an_unknown_theme_name_falls_back_to_the_default() {
    let dir = TempDir::with_config("nosuchtheme", "[theme]\nname = \"Paper\"\n");

    let config = load(&dir);

    assert_eq!(config.theme, None);
    assert_eq!(config.warnings.len(), 1);
    assert!(
        config.warnings[0].starts_with("config.toml:2:8: no theme `Paper`"),
        "{:?}",
        config.warnings
    );
}

#[test]
fn a_bad_idle_timeout_warns_and_keeps_the_default() {
    let dir = TempDir::with_config("badidle", "[sync]\nidle-timeout = \"5m\"\n");

    let config = load(&dir);

    assert_eq!(config.idle_timeout, None);
    assert_eq!(config.warnings.len(), 1);
    assert!(
        config.warnings[0].starts_with("config.toml:2:16: `sync.idle-timeout` must be"),
        "{:?}",
        config.warnings
    );
}

#[test]
fn an_unknown_key_in_ui_is_reported() {
    let dir = TempDir::with_config("ui", "[ui]\ncolour = \"none\"\n");

    let config = load(&dir);

    assert_eq!(
        config.warnings,
        vec!["config.toml:2:1: unknown key `colour` in [ui] is ignored".to_owned()]
    );
}

#[test]
fn api_token_is_known_and_never_read() {
    let dir = TempDir::with_config(
        "token",
        "api_token = \"0123456789abcdef\"\n\n[sync]\nidle-timeout = 30\n",
    );

    let config = load(&dir);

    assert_eq!(config.warnings, Vec::<String>::new());
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(30)));
    assert!(!format!("{config:?}").contains("0123456789abcdef"));
}

/// What startup does with the config folder: load the config, then resolve
/// the token (which runs the migration when `config.toml` has `api_token`).
fn start_up(dir: &Path) -> Config {
    let config = config::load(dir);
    let keyring = MemoryKeyring::holding(ApiToken::new(TOKEN).unwrap());
    let mut notes = Vec::new();
    let resolved = auth::resolve_from(None, &keyring, dir, &mut notes).unwrap();
    assert_eq!(resolved.source, Source::Keyring);
    assert!(notes.is_empty(), "{}", String::from_utf8_lossy(&notes));
    config
}

const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

/// Every entry under `dir` with its inode, change time and modification
/// time: any write, even one of the same bytes, changes the times, and a
/// replace changes the inode.
fn snapshot(dir: &Path) -> Vec<(PathBuf, u64, i64, i64, SystemTime)> {
    let mut entries = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                pending.push(path.clone());
            }
            entries.push((
                path,
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
                metadata.modified().unwrap(),
            ));
        }
    }
    entries.sort();
    entries
}

#[test]
fn starting_up_writes_nothing_to_a_writable_config_folder() {
    let text = "# mine\n[theme]\nname = \"Mine\"\n[sync]\nidle-timeout = = 1\n";
    let dir = TempDir::with_config("startupwritable", text);
    dir.theme("mine.json", MINE);
    let file = dir.0.join("config.toml");
    // An old modification time, so even a same-bytes rewrite within the
    // timestamp granularity shows up.
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    for path in [&file, &dir.0.join("themes/mine.json")] {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    let before = snapshot(&dir.0);

    let config = start_up(&dir.0);

    assert_eq!(config.warnings.len(), 1, "{:?}", config.warnings);
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    assert_eq!(snapshot(&dir.0), before);
}

#[test]
fn starting_up_on_a_read_only_config_folder_writes_nothing() {
    let text = "# mine\n[sync]\nidle-timeout = 30\n";
    let dir = TempDir::with_config("startupreadonly", text);
    let file = dir.0.join("config.toml");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o555)).unwrap();
    let before = snapshot(&dir.0);

    let config = start_up(&dir.0);

    assert_eq!(config.warnings, Vec::<String>::new());
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(30)));
    assert_eq!(snapshot(&dir.0), before);
}

#[test]
fn loading_a_read_only_config_folder_writes_nothing() {
    let text = "# mine\n[sync]\nidle-timeout = = 1\n";
    let dir = TempDir::with_config("readonly", text);
    let file = dir.0.join("config.toml");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o555)).unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();

    let config = load(&dir);

    assert_eq!(config.warnings.len(), 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    let entries: Vec<_> = fs::read_dir(&dir.0).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

#[test]
fn user_theme_files_load_and_a_broken_one_is_a_warning() {
    let dir = TempDir::with_config("themes", "[theme]\nname = \"Mine\"\n");
    dir.theme("mine.json", MINE);
    dir.theme("broken.json", "{ \"name\": \"Broken\" ");
    dir.theme("notes.txt", "not a theme");

    let config = load(&dir);

    let names: Vec<&str> = config.user_themes.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["Mine"]);
    assert_eq!(config.theme.as_deref(), Some("Mine"));
    assert_eq!(config.warnings.len(), 1, "{:?}", config.warnings);
    assert!(
        config.warnings[0].starts_with("themes/broken.json:1:19: "),
        "{:?}",
        config.warnings
    );
    assert!(config.warnings[0].contains("skipped"));
}

#[test]
fn the_old_macos_folder_is_read_when_the_xdg_one_is_absent() {
    let home = TempDir::new("legacy");
    let xdg = home.0.join(".config/ratatoist");
    let old = home.0.join("Library/Application Support/ratatoist");
    fs::create_dir_all(&old).unwrap();

    let (dir, warning) = config::source_dir(&xdg, Some(&old));

    assert_eq!(dir, old);
    let warning = warning.unwrap();
    assert!(warning.contains("mv "), "{warning}");
    assert!(warning.contains(&old.display().to_string()));
    assert!(warning.contains(&xdg.display().to_string()));
}

#[test]
fn the_xdg_folder_wins_when_it_exists() {
    let home = TempDir::new("legacywins");
    let xdg = home.0.join(".config/ratatoist");
    let old = home.0.join("Library/Application Support/ratatoist");
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&xdg).unwrap();

    assert_eq!(config::source_dir(&xdg, Some(&old)), (xdg.clone(), None));
    assert_eq!(config::source_dir(&xdg, None), (xdg, None));
}

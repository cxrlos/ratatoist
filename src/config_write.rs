//! The one setting ratatoist writes in `config.toml`: `[theme] name`, when a
//! theme is picked (ADR 0004). Every other byte of the file stays as it is.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::config::FILE;

/// Why a picked theme was not written to `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeNotSaved {
    /// The file at `path` is left untouched, for the reason `why`.
    Refused { path: String, why: &'static str },
    /// Reading or writing the file at `path` failed.
    Failed { path: String, error: String },
}

/// Sets `[theme] name = "<slug>"` in `config_dir/config.toml`, creating the
/// file or the table when missing, through `toml_edit` so comments, order
/// and layout survive. A symlinked file is followed and its target changed.
/// A file that does not parse, or whose `theme` is not a table, is left
/// alone.
pub fn save_theme(config_dir: &Path, slug: &str) -> Result<(), ThemeNotSaved> {
    let path = target(&config_dir.join(FILE)).map_err(|error| failed(config_dir, &error))?;
    let shown = path.display().to_string();
    let failed = |error: io::Error| ThemeNotSaved::Failed {
        path: shown.clone(),
        error: error.to_string(),
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(failed(error)),
    };
    let refused = |why| ThemeNotSaved::Refused {
        path: shown.clone(),
        why,
    };
    let mut document: DocumentMut = text.parse().map_err(|_| refused("does not parse"))?;
    let theme = document
        .entry("theme")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| refused("has a `theme` that is not a table"))?;
    let mut name = Value::from(slug);
    match theme.get_mut("name") {
        Some(Item::Value(old)) => {
            *name.decor_mut() = old.decor().clone();
            *old = name;
        }
        Some(other) => *other = Item::Value(name),
        None => {
            theme.insert("name", Item::Value(name));
        }
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(failed)?;
    }
    replace(&path, &document.to_string()).map_err(failed)
}

/// The file `link` names: the target when it is a symlink, so the write
/// changes the real file and keeps the link.
fn target(link: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(link) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(link),
        _ => Ok(link.to_owned()),
    }
}

fn failed(path: &Path, error: &io::Error) -> ThemeNotSaved {
    ThemeNotSaved::Failed {
        path: path.join(FILE).display().to_string(),
        error: error.to_string(),
    }
}

/// Writes `text` to `path` through a temporary file in the same folder, with
/// the file's mode (0600 for a new one), renamed over it. The temporary name
/// carries the process id, so two runs at once never share one.
pub(crate) fn replace(path: &Path, text: &str) -> io::Result<()> {
    let temporary = path.with_extension(format!("toml.ratatoist-{}.tmp", std::process::id()));
    let mode = fs::metadata(path)
        .map(|metadata| metadata.permissions().mode())
        .unwrap_or(0o600);
    let written = fs::write(&temporary, text)
        .and_then(|()| fs::set_permissions(&temporary, fs::Permissions::from_mode(mode)))
        .and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::{ThemeNotSaved, save_theme};

    /// An empty folder for one test.
    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ratatoist-config-write-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const COMMENTED: &str = "\
# My ratatoist settings.

[ui]
icons = \"nerd\"   # the patched font

# Colours.
[theme]
# Picked by hand once.
name   =   \"dracula\"   # dark
# after

[sync]
idle-timeout = 300
";

    #[test]
    fn picking_changes_only_the_name_line_of_a_commented_file() {
        let dir = folder("commented");
        fs::write(dir.join("config.toml"), COMMENTED).unwrap();

        save_theme(&dir, "nord").unwrap();

        let written = fs::read_to_string(dir.join("config.toml")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            written,
            COMMENTED.replace("\"dracula\"", "\"nord\""),
            "only the value changed"
        );
    }

    #[test]
    fn a_file_without_theme_gains_just_that_table_and_key() {
        let dir = folder("no-table");
        let before = "# Mine.\n[ui]\nicons = \"unicode\" # plain\n";
        fs::write(dir.join("config.toml"), before).unwrap();

        save_theme(&dir, "nord").unwrap();

        let written = fs::read_to_string(dir.join("config.toml")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(written, format!("{before}\n[theme]\nname = \"nord\"\n"));
    }

    #[test]
    fn a_missing_file_and_folder_are_created_with_just_the_key() {
        let dir = folder("missing").join("ratatoist");

        save_theme(&dir, "nord").unwrap();

        let written = fs::read_to_string(dir.join("config.toml")).unwrap();
        let mode = fs::metadata(dir.join("config.toml"))
            .unwrap()
            .permissions()
            .mode();
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
        assert_eq!(written, "[theme]\nname = \"nord\"\n");
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_symlinked_file_keeps_its_link_and_its_target_changes() {
        let dir = folder("symlink");
        let dotfiles = dir.join("dotfiles");
        let config = dir.join("config");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(&config).unwrap();
        fs::write(
            dotfiles.join("ratatoist.toml"),
            "[theme]\nname = \"dracula\"\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(dotfiles.join("ratatoist.toml"), config.join("config.toml"))
            .unwrap();

        save_theme(&config, "nord").unwrap();

        let link = fs::symlink_metadata(config.join("config.toml")).unwrap();
        let target = fs::read_to_string(dotfiles.join("ratatoist.toml")).unwrap();
        let leftovers = fs::read_dir(&dotfiles).unwrap().count();
        fs::remove_dir_all(&dir).unwrap();
        assert!(link.file_type().is_symlink());
        assert_eq!(target, "[theme]\nname = \"nord\"\n");
        assert_eq!(leftovers, 1, "no temporary file is left behind");
    }

    #[test]
    fn a_file_that_does_not_parse_is_left_untouched() {
        let dir = folder("unparsable");
        let broken = "[theme\nname = \"dracula\"\n";
        fs::write(dir.join("config.toml"), broken).unwrap();

        let result = save_theme(&dir, "nord");

        let after = fs::read_to_string(dir.join("config.toml")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(
            result,
            Err(ThemeNotSaved::Refused {
                why: "does not parse",
                ..
            })
        ));
        assert_eq!(after, broken);
    }

    #[test]
    fn a_theme_that_is_not_a_table_is_left_untouched() {
        let dir = folder("not-a-table");
        let odd = "theme = \"dracula\"\n";
        fs::write(dir.join("config.toml"), odd).unwrap();

        let result = save_theme(&dir, "nord");

        let after = fs::read_to_string(dir.join("config.toml")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(result, Err(ThemeNotSaved::Refused { .. })));
        assert_eq!(after, odd);
    }

    #[test]
    fn a_folder_that_cannot_be_written_fails_with_the_path() {
        let dir = folder("read-only");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();

        let result = save_theme(&dir, "nord");

        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        match result {
            Err(ThemeNotSaved::Failed { path, .. }) => assert!(path.ends_with("config.toml")),
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}

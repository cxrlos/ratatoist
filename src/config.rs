//! `config.toml` in the config folder. ratatoist never writes this file;
//! `docs/config.md` documents every key.
//!
//! ```toml
//! [ui]                       # no settings yet
//! [keys]                     # no settings yet
//!
//! [theme]
//! name = "Nord"              # a built-in, or the name inside a themes/*.json file
//!
//! [sync]
//! idle-timeout = 300         # seconds without a key press before the session is idle
//! ```
//!
//! A problem is reported as `config.toml:LINE:COL: problem`, and only the
//! section it is in falls back to its defaults. Unknown keys and tables are
//! reported and ignored. The top-level `api_token` of older versions is a
//! known key so it does not warn, and this loader never reads its value.

use std::fs;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Duration;

use toml::Spanned;
use toml::de::{DeTable, DeValue};
use toml_parser::parser::{Event, EventKind};

use crate::ui::theme::{Base16Scheme, Theme};

/// The config file's name inside the config folder.
pub const FILE: &str = "config.toml";

/// What the config file asks for. Anything missing or broken falls back to
/// its default, with a line in `warnings` naming the file, line and column.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// `[theme] name`: a built-in theme or one from `themes/`, by its name.
    pub theme: Option<String>,
    /// `[sync] idle-timeout`.
    pub idle_timeout: Option<Duration>,
    /// Theme files loaded from `themes/` in the config folder.
    pub user_themes: Vec<Theme>,
    /// One line per problem found while loading, for the user to read.
    pub warnings: Vec<String>,
}

/// Reads `config.toml` and the theme files under `dir`, or under the old
/// macOS folder when `dir` does not exist and that one does.
pub fn load(dir: &Path) -> Config {
    let (dir, moved) = source_dir(dir, macos_legacy_dir().as_deref());
    let mut warnings: Vec<String> = moved.into_iter().collect();

    let (user_themes, theme_warnings) = load_user_themes(&dir.join("themes"));
    let mut config = match fs::read_to_string(dir.join(FILE)) {
        Ok(text) => parse(&text, &user_themes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(error) => Config {
            warnings: vec![format!(
                "{FILE}: could not read it ({error}); using the default config"
            )],
            ..Config::default()
        },
    };
    warnings.append(&mut config.warnings);
    warnings.extend(theme_warnings);
    config.warnings = warnings;
    config.user_themes = user_themes;
    config
}

/// The folder to read: `xdg`, unless it does not exist and `legacy` (the
/// pre-0.6.0 macOS folder) does. Reading the old folder comes with a warning
/// holding the command that moves it.
pub fn source_dir(xdg: &Path, legacy: Option<&Path>) -> (PathBuf, Option<String>) {
    match legacy {
        Some(legacy) if !xdg.exists() && legacy.is_dir() => {
            let parent = xdg.parent().unwrap_or(xdg);
            let warning = format!(
                "ratatoist read its config from {}, the old macOS folder; move it with: \
                 mkdir -p \"{}\" && mv \"{}\" \"{}\"",
                legacy.display(),
                parent.display(),
                legacy.display(),
                xdg.display()
            );
            (legacy.to_owned(), Some(warning))
        }
        _ => (xdg.to_owned(), None),
    }
}

#[cfg(target_os = "macos")]
fn macos_legacy_dir() -> Option<PathBuf> {
    etcetera::home_dir()
        .ok()
        .map(|home| home.join("Library/Application Support/ratatoist"))
}

#[cfg(not(target_os = "macos"))]
fn macos_legacy_dir() -> Option<PathBuf> {
    None
}

/// Every `*.json` Base16 file in `dir`, sorted by theme name, and a warning
/// for each one that does not load.
fn load_user_themes(dir: &Path) -> (Vec<Theme>, Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut themes = Vec::new();
    let mut warnings = Vec::new();
    for path in paths {
        let name = format!(
            "themes/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let loaded = fs::read_to_string(&path)
            .map_err(|error| format!("{name}: could not read it ({error})"))
            .and_then(|json| {
                serde_json::from_str::<Base16Scheme>(&json)
                    .map_err(|error| format!("{name}:{}:{}: {error}", error.line(), error.column()))
            });
        match loaded {
            Ok(scheme) => themes.push(Theme::from_scheme(&scheme)),
            Err(warning) => warnings.push(format!("{warning}; the theme is skipped")),
        }
    }
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    (themes, warnings)
}

/// Reads config `text`, matching `[theme] name` against the built-in themes
/// and `user_themes`.
fn parse(text: &str, user_themes: &[Theme]) -> Config {
    let mut reader = Reader {
        text,
        warnings: Vec::new(),
    };
    let (document, syntax_errors) = DeTable::parse_recoverable(text);
    let headers = headers(text);
    let mut broken: Vec<String> = Vec::new();
    let mut whole_file_broken = false;
    // Recovery after one mistake often reports more after it, so only the
    // first syntax error of each section, or of the file, is kept.
    for error in syntax_errors {
        let offset = error.span().map(|span| span.start);
        let table = offset.and_then(|offset| table_at(&headers, offset));
        if whole_file_broken || table.as_ref().is_some_and(|table| broken.contains(table)) {
            continue;
        }
        let fallback = match table.as_deref() {
            None => {
                whole_file_broken = true;
                "; using the default config".to_owned()
            }
            Some(name) if SECTIONS.contains(&name) => format!("; [{name}] uses its defaults"),
            Some(_) => String::new(),
        };
        reader.warn(offset, format!("{}{fallback}", error.message().trim_end()));
        broken.extend(table);
    }

    let mut config = Config::default();
    if !whole_file_broken {
        for (name, value) in document.get_ref() {
            let name_str: &str = name.get_ref();
            if broken.iter().any(|table| table == name_str) {
                continue;
            }
            match name_str {
                "ui" | "keys" => {
                    reader.section::<Empty>(name_str, value, read_nothing);
                }
                "theme" => {
                    let section = reader.section("theme", value, read_theme);
                    if let Some((wanted, offset)) = section.name {
                        config.theme = reader.theme(&wanted, offset, user_themes);
                    }
                }
                "sync" => {
                    config.idle_timeout = reader.section("sync", value, read_sync).idle_timeout;
                }
                // Read only by the one-time move into the keyring (see `auth`).
                "api_token" => {}
                _ => {
                    let what = match value.get_ref() {
                        DeValue::Table(_) => format!("table `[{name_str}]`"),
                        _ => format!("key `{name_str}`"),
                    };
                    reader.warn_at(
                        name,
                        format!(
                            "unknown {what} is ignored; ratatoist reads [ui], [keys], [theme] and [sync]"
                        ),
                    );
                }
            }
        }
    }

    let mut warnings = reader.warnings;
    warnings.sort_by_key(|(offset, _)| *offset);
    config.warnings = warnings.into_iter().map(|(_, warning)| warning).collect();
    config
}

/// The top-level tables ratatoist reads.
const SECTIONS: [&str; 4] = ["ui", "keys", "theme", "sync"];

/// Where each table header starts and the top-level table it opens, in
/// file order. TOML's own parser finds them, so a header-like line inside a
/// string is not one and a quoted name such as `["ui"]` is the table it
/// names.
fn headers(text: &str) -> Vec<(usize, String)> {
    let source = toml_parser::Source::new(text);
    let tokens = source.lex().into_vec();
    let mut events: Vec<Event> = Vec::new();
    toml_parser::parser::parse_document(&tokens, &mut events, &mut ());
    let mut headers = Vec::new();
    let mut open = None;
    for event in &events {
        match event.kind() {
            EventKind::StdTableOpen | EventKind::ArrayTableOpen => {
                open = Some(event.span().start())
            }
            EventKind::SimpleKey => {
                if let (Some(start), Some(raw)) = (open.take(), source.get(event)) {
                    let mut name = String::new();
                    raw.decode_key(&mut name, &mut ());
                    headers.push((start, name));
                }
            }
            EventKind::StdTableClose | EventKind::ArrayTableClose | EventKind::Newline => {
                open = None;
            }
            _ => {}
        }
    }
    headers
}

/// The top-level table of the last header starting at or before `offset`,
/// so a syntax error breaks only the section it is in. `None` before any
/// header.
fn table_at(headers: &[(usize, String)], offset: usize) -> Option<String> {
    headers
        .iter()
        .rev()
        .find(|(start, _)| *start <= offset)
        .map(|(_, name)| name.clone())
}

/// One setting read from its value, or why the value is wrong. `None` for a
/// key the section does not have.
type Setting = Option<Result<(), String>>;

/// `[ui]` and `[keys]`, which take no settings yet.
#[derive(Default)]
struct Empty;

fn read_nothing(_: &mut Empty, _: &str, _: &Spanned<DeValue>) -> Setting {
    None
}

#[derive(Default)]
struct ThemeSection {
    /// The name asked for and the byte offset of its value.
    name: Option<(String, usize)>,
}

fn read_theme(theme: &mut ThemeSection, key: &str, value: &Spanned<DeValue>) -> Setting {
    if key != "name" {
        return None;
    }
    Some(match value.get_ref().as_str() {
        Some(name) if !name.trim().is_empty() => {
            theme.name = Some((name.to_owned(), value.span().start));
            Ok(())
        }
        _ => Err("`theme.name` must be a theme name such as \"Nord\"".to_owned()),
    })
}

#[derive(Default)]
struct SyncSection {
    idle_timeout: Option<Duration>,
}

fn read_sync(sync: &mut SyncSection, key: &str, value: &Spanned<DeValue>) -> Setting {
    const SECONDS: RangeInclusive<u64> = 0..=86_400;
    Some(match key {
        "idle-timeout" => integer(value, "sync.idle-timeout", SECONDS)
            .map(|seconds| sync.idle_timeout = Some(Duration::from_secs(seconds))),
        _ => return None,
    })
}

fn integer(value: &Spanned<DeValue>, key: &str, range: RangeInclusive<u64>) -> Result<u64, String> {
    value
        .get_ref()
        .as_integer()
        .and_then(|integer| u64::from_str_radix(integer.as_str(), integer.radix()).ok())
        .filter(|number| range.contains(number))
        .ok_or_else(|| {
            format!(
                "`{key}` must be a whole number of seconds from {} to {}",
                range.start(),
                range.end()
            )
        })
}

/// A theme name as a slug: lower case, spaces as hyphens, so
/// `rose-pine` finds "Rose Pine".
fn slug(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '_'], "-")
}

struct Reader<'a> {
    text: &'a str,
    /// Each warning with the byte offset it points at, for sorting.
    warnings: Vec<(usize, String)>,
}

impl Reader<'_> {
    /// Reads the settings of table `[name]` with `read`. Any bad value
    /// leaves the whole section at its defaults; unknown keys are only
    /// reported.
    fn section<T: Default>(
        &mut self,
        name: &str,
        value: &Spanned<DeValue>,
        read: fn(&mut T, &str, &Spanned<DeValue>) -> Setting,
    ) -> T {
        let DeValue::Table(entries) = value.get_ref() else {
            self.warn_at(
                value,
                format!("`{name}` must be a table, as in `[{name}]`; [{name}] uses its defaults"),
            );
            return T::default();
        };
        let mut section = T::default();
        let mut broken = false;
        for (key, value) in entries {
            let key_str: &str = key.get_ref();
            match read(&mut section, key_str, value) {
                Some(Ok(())) => {}
                Some(Err(message)) => {
                    self.warn_at(value, format!("{message}; [{name}] uses its defaults"));
                    broken = true;
                }
                None => {
                    self.warn_at(
                        key,
                        format!("unknown key `{key_str}` in [{name}] is ignored"),
                    );
                }
            }
        }
        if broken { T::default() } else { section }
    }

    /// The name of the theme `wanted` names, set at byte `offset`: a
    /// built-in or a user theme, matched by name or slug. `None`, with a
    /// warning, when there is no such theme.
    fn theme(&mut self, wanted: &str, offset: usize, user_themes: &[Theme]) -> Option<String> {
        let all: Vec<String> = Theme::builtin()
            .into_iter()
            .chain(user_themes.iter().cloned())
            .map(|theme| theme.name)
            .collect();
        if let Some(found) = all.iter().find(|name| slug(name) == slug(wanted)) {
            return Some(found.clone());
        }
        self.warn(
            Some(offset),
            format!(
                "no theme `{wanted}`: it is not built in and no file in themes/ has that name; \
                 [theme] uses its defaults. Themes: {}",
                all.join(", ")
            ),
        );
        None
    }

    fn warn_at<T>(&mut self, spanned: &Spanned<T>, message: String) {
        self.warn(Some(spanned.span().start), message);
    }

    fn warn(&mut self, offset: Option<usize>, message: String) {
        let Some(offset) = offset else {
            self.warnings.push((0, format!("{FILE}: {message}")));
            return;
        };
        let before = self.text.get(..offset).unwrap_or(self.text);
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
        let column = before[line_start..].chars().count() + 1;
        self.warnings
            .push((offset, format!("{FILE}:{line}:{column}: {message}")));
    }
}

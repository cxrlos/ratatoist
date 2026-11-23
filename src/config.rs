//! `config.toml` in the config folder. ratatoist never writes this file;
//! `docs/config.md` documents every key.
//!
//! ```toml
//! [ui]
//! icons = "unicode"          # or "nerd", for a patched Nerd Font
//! wide-layout-columns = 120  # from this width on, the detail shows beside the list
//!
//! [keys]
//! preset = "standard"        # or "vim", the default
//!
//! [keys.global]
//! quit = "Q"                 # replace: Q is now quit's only key
//! show-cheatsheet = { append = ["<F1>"] }
//!
//! [keys.list]
//! move-down = { prepend = ["n"] }   # n first, j still works
//! noop = ["gg", "<C-d>"]            # these keys do nothing, and leave every hint
//!
//! [theme]
//! name = "nord"              # a built-in, or the name inside a themes/*.json file
//!
//! [sync]
//! idle-timeout = 300         # seconds without a key press before the session is idle
//!
//! [updates]
//! check = true               # ask GitHub once a day for a newer release
//! ```
//!
//! A problem is reported as `config.toml:LINE:COL: problem`, and only the
//! section it is in falls back to its defaults, except that a bad binding
//! resets only its `[keys.CONTEXT]` table. Unknown keys and tables are
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

use crate::keys::{self, Change, Context, KeySequence, Override, Preset, Target};
use crate::ui::icons::IconSet;
use crate::ui::theme::{Theme, slug};

/// The config file's name inside the config folder.
pub const FILE: &str = "config.toml";

/// What the config file asks for. Anything missing or broken falls back to
/// its default, with a line in `warnings` naming the file, line and column.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// `[theme] name`: a built-in theme or one from `themes/`, by its name.
    pub theme: Option<String>,
    /// `[ui] icons`.
    pub icons: IconSet,
    /// `[ui] wide-layout-columns`: from this many columns on, a task list
    /// and the focused task's detail show side by side.
    pub wide_layout_columns: Option<u16>,
    /// `[sync] idle-timeout`.
    pub idle_timeout: Option<Duration>,
    /// `[keys]`: the preset and the changes to its keys.
    pub keys: Keys,
    /// `[updates]`.
    pub updates: UpdateSettings,
    /// Theme files loaded from `themes/` in the config folder.
    pub user_themes: Vec<Theme>,
    /// One line per problem found while loading, for the user to read.
    pub warnings: Vec<String>,
}

/// `[updates]`: whether ratatoist asks GitHub once a day for a newer
/// release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateSettings {
    /// `check`: true by default.
    pub check: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { check: true }
    }
}

/// `[keys]`: which preset the keys start from, and the changes to them in
/// the `[keys.CONTEXT]` tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    pub preset: Preset,
    pub overrides: Vec<Override>,
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

/// The folder [`load`] reads for `xdg` on this machine, without the warning.
pub fn dir_to_read(xdg: &Path) -> PathBuf {
    source_dir(xdg, macos_legacy_dir().as_deref()).0
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
                Theme::from_base16_json(&json).map_err(|error| match error.at {
                    Some((line, column)) => format!("{name}:{line}:{column}: {error}"),
                    None => format!("{name}: {error}"),
                })
            });
        match loaded {
            Ok(theme) => themes.push(theme),
            Err(warning) => warnings.push(format!("{warning}; the theme is skipped")),
        }
    }
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    (themes, warnings)
}

/// Reads config `text`, matching `[theme] name` against the built-in themes
/// and `user_themes`.
pub fn parse(text: &str, user_themes: &[Theme]) -> Config {
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
                "ui" => {
                    let ui = reader.section("ui", value, read_ui);
                    config.icons = ui.icons;
                    config.wide_layout_columns = ui.wide_layout_columns;
                }
                "keys" => config.keys = reader.keys(value),
                "theme" => {
                    let section = reader.section("theme", value, read_theme);
                    if let Some((wanted, offset)) = section.name {
                        config.theme = reader.theme(&wanted, offset, user_themes);
                    }
                }
                "sync" => {
                    config.idle_timeout = reader.section("sync", value, read_sync).idle_timeout;
                }
                "updates" => config.updates = reader.section("updates", value, read_updates),
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
                            "unknown {what} is ignored; ratatoist reads [ui], [keys], [theme], [sync] and [updates]"
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
const SECTIONS: [&str; 5] = ["ui", "keys", "theme", "sync", "updates"];

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

#[derive(Default)]
struct UiSection {
    icons: IconSet,
    wide_layout_columns: Option<u16>,
}

fn read_ui(ui: &mut UiSection, key: &str, value: &Spanned<DeValue>) -> Setting {
    const COLUMNS: RangeInclusive<u64> = 0..=65_535;
    Some(match key {
        "icons" => match value.get_ref().as_str().and_then(IconSet::from_name) {
            Some(icons) => {
                ui.icons = icons;
                Ok(())
            }
            None => Err("`ui.icons` must be \"unicode\" or \"nerd\"".to_owned()),
        },
        "wide-layout-columns" => {
            integer(value, "ui.wide-layout-columns", "columns", COLUMNS).map(|columns| {
                ui.wide_layout_columns = Some(u16::try_from(columns).unwrap_or(u16::MAX));
            })
        }
        _ => return None,
    })
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
        "idle-timeout" => integer(value, "sync.idle-timeout", "seconds", SECONDS)
            .map(|seconds| sync.idle_timeout = Some(Duration::from_secs(seconds))),
        _ => return None,
    })
}

fn read_updates(updates: &mut UpdateSettings, key: &str, value: &Spanned<DeValue>) -> Setting {
    if key != "check" {
        return None;
    }
    Some(match value.get_ref().as_bool() {
        Some(check) => {
            updates.check = check;
            Ok(())
        }
        None => Err("`updates.check` must be true or false".to_owned()),
    })
}

/// The whole number `value` holds, when it is in `range`; otherwise why
/// not, naming `key` and the `unit` it counts.
fn integer(
    value: &Spanned<DeValue>,
    key: &str,
    unit: &str,
    range: RangeInclusive<u64>,
) -> Result<u64, String> {
    value
        .get_ref()
        .as_integer()
        .and_then(|integer| u64::from_str_radix(integer.as_str(), integer.radix()).ok())
        .filter(|number| range.contains(number))
        .ok_or_else(|| {
            format!(
                "`{key}` must be a whole number of {unit} from {} to {}",
                range.start(),
                range.end()
            )
        })
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
        let all: Vec<String> = Theme::built_ins()
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

    /// `[keys]`. A bad `preset` resets the whole table; a bad binding resets
    /// only its `[keys.CONTEXT]` table.
    fn keys(&mut self, keys: &Spanned<DeValue>) -> Keys {
        let DeValue::Table(entries) = keys.get_ref() else {
            self.warn_at(
                keys,
                "`keys` must be a table, as in `[keys]`; [keys] uses the default keys".to_owned(),
            );
            return Keys::default();
        };
        let mut read = Keys::default();
        let mut broken = false;
        for (name, value) in entries {
            let name_str: &str = name.get_ref();
            if name_str == "preset" {
                match value.get_ref().as_str().and_then(Preset::from_name) {
                    Some(preset) => read.preset = preset,
                    None => {
                        self.warn_at(
                            value,
                            "`keys.preset` must be \"vim\" or \"standard\"; \
                             [keys] uses the default keys"
                                .to_owned(),
                        );
                        broken = true;
                    }
                }
                continue;
            }
            let Some(context) = Context::from_name(name_str) else {
                let names: Vec<&str> = Context::ALL.iter().map(|c| c.name()).collect();
                let what = match value.get_ref() {
                    DeValue::Table(_) => format!("context `[keys.{name_str}]`"),
                    _ => format!("key `{name_str}` in [keys]"),
                };
                self.warn_at(
                    name,
                    format!(
                        "unknown {what} is ignored; [keys] takes `preset` and the contexts {}",
                        names.join(", ")
                    ),
                );
                continue;
            };
            let before = self.warnings.len();
            let found = self.context(context, value);
            if self.warnings.len() == before {
                read.overrides.extend(found);
            } else {
                for (_, warning) in &mut self.warnings[before..] {
                    warning.push_str(&format!(
                        "; [keys.{}] uses the default keys",
                        context.name()
                    ));
                }
            }
        }
        if broken { Keys::default() } else { read }
    }

    /// The overrides in one `[keys.CONTEXT]` table.
    fn context(&mut self, context: Context, table: &Spanned<DeValue>) -> Vec<Override> {
        let DeValue::Table(entries) = table.get_ref() else {
            self.warn_at(table, format!("`keys.{}` must be a table", context.name()));
            return Vec::new();
        };
        let mut overrides = Vec::new();
        for (id, value) in entries {
            let id_str: &str = id.get_ref();
            let target = if id_str == "noop" {
                Target::Noop
            } else {
                match keys::by_id(id_str) {
                    Some(spec) if spec.contexts.contains(&context) => Target::Action(spec.action),
                    Some(_) => {
                        self.warn_at(
                            id,
                            format!("`{id_str}` does not apply in [keys.{}]", context.name()),
                        );
                        continue;
                    }
                    None => {
                        self.warn_at(id, format!("unknown action `{id_str}`"));
                        continue;
                    }
                }
            };
            let mut add = |change, keys| {
                overrides.push(Override {
                    context,
                    target,
                    change,
                    keys,
                });
            };
            match value.get_ref() {
                DeValue::Table(changes) if target != Target::Noop => {
                    for (change, keys) in changes {
                        let change = match change.get_ref().as_ref() {
                            "prepend" => Change::Prepend,
                            "append" => Change::Append,
                            other => {
                                self.warn_at(
                                    change,
                                    format!("`{other}` is not `prepend` or `append`"),
                                );
                                continue;
                            }
                        };
                        if let Some(keys) = self.sequences(keys) {
                            add(change, keys);
                        }
                    }
                }
                _ => {
                    if let Some(keys) = self.sequences(value) {
                        add(Change::Replace, keys);
                    }
                }
            }
        }
        overrides
    }

    /// One key or a list of keys in config notation, or `None` with a
    /// warning.
    fn sequences(&mut self, value: &Spanned<DeValue>) -> Option<Vec<KeySequence>> {
        match value.get_ref() {
            DeValue::String(_) => Some(vec![self.sequence(value)?]),
            DeValue::Array(items) => items.iter().map(|item| self.sequence(item)).collect(),
            _ => {
                self.warn_at(value, "expected a key or a list of keys".to_owned());
                None
            }
        }
    }

    fn sequence(&mut self, value: &Spanned<DeValue>) -> Option<KeySequence> {
        let Some(notation) = value.get_ref().as_str() else {
            self.warn_at(
                value,
                "expected a key such as \"j\" or \"<C-d>\"".to_owned(),
            );
            return None;
        };
        match notation.parse() {
            Ok(keys) => Some(keys),
            Err(error) => {
                self.warn_at(value, format!("{error}"));
                None
            }
        }
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
        let column = before.rsplit('\n').next().unwrap_or(before).chars().count() + 1;
        self.warnings
            .push((offset, format!("{FILE}:{line}:{column}: {message}")));
    }
}

use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One key press: a key and the modifiers held with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Key {
    /// A printable character on its own. Shifted letters are just the
    /// uppercase character, as `G`.
    pub const fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    /// A character with Ctrl held, written `<C-d>`.
    pub const fn ctrl(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    /// A key that is not a character, such as `Esc` or `Enter`.
    pub const fn named(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }

    /// A key that is not a character with Shift held, written `<S-Tab>`.
    pub const fn shift(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::SHIFT)
    }

    const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// The character this key types into a text field: a character without
    /// Ctrl or Alt.
    pub fn typed(self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if self.modifiers.is_empty() => Some(c),
            _ => None,
        }
    }
}

impl From<KeyEvent> for Key {
    fn from(event: KeyEvent) -> Self {
        let mut modifiers = event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT);
        let code = match event.code {
            // Terminals report Shift on characters inconsistently; the
            // character itself already says whether it was shifted.
            KeyCode::Char(_) => event.code,
            // Shift+Tab arrives as its own key; config writes it `<S-Tab>`.
            KeyCode::BackTab => {
                modifiers |= KeyModifiers::SHIFT;
                KeyCode::Tab
            }
            code => {
                modifiers |= event.modifiers & KeyModifiers::SHIFT;
                code
            }
        };
        Self::new(code, modifiers)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (modifier, prefix) in [
            (KeyModifiers::CONTROL, "C-"),
            (KeyModifiers::ALT, "A-"),
            (KeyModifiers::SHIFT, "S-"),
        ] {
            if self.modifiers.contains(modifier) {
                f.write_str(prefix)?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "F{n}"),
            code => match NAMES.iter().find(|(_, named)| *named == code) {
                Some((name, _)) => f.write_str(name),
                None => write!(f, "{code:?}"),
            },
        }
    }
}

/// Keys written by name inside `<...>`, first name is the one displayed.
const NAMES: &[(&str, KeyCode)] = &[
    ("Esc", KeyCode::Esc),
    ("Enter", KeyCode::Enter),
    ("CR", KeyCode::Enter),
    ("Return", KeyCode::Enter),
    ("Tab", KeyCode::Tab),
    ("Backspace", KeyCode::Backspace),
    ("BS", KeyCode::Backspace),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PageUp", KeyCode::PageUp),
    ("PageDown", KeyCode::PageDown),
    ("Delete", KeyCode::Delete),
    ("Del", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Space", KeyCode::Char(' ')),
    ("lt", KeyCode::Char('<')),
];

/// The keys pressed one after another for a binding: one key, or a chord
/// such as `gg`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeySequence(Vec<Key>);

impl KeySequence {
    pub fn keys(&self) -> &[Key] {
        &self.0
    }
}

impl From<&[Key]> for KeySequence {
    fn from(keys: &[Key]) -> Self {
        Self(keys.to_vec())
    }
}

impl fmt::Display for KeySequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = self.0.iter().map(Key::to_string).collect();
        let separator = if names.iter().all(|name| name.chars().count() == 1) {
            ""
        } else {
            " "
        };
        f.write_str(&names.join(separator))
    }
}

/// Why a key in the config could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseKeyError(String);

impl fmt::Display for ParseKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseKeyError {}

/// Reads vim notation: plain characters stand for themselves (`gg`, `G`),
/// and `<...>` names a key with optional `C-`, `A-` or `S-` modifiers
/// (`<C-d>`, `<Esc>`, `<S-Tab>`, `<F1>`, `<lt>` for `<`).
impl FromStr for KeySequence {
    type Err = ParseKeyError;

    fn from_str(notation: &str) -> Result<Self, Self::Err> {
        let mut keys = Vec::new();
        let mut rest = notation;
        while let Some(c) = rest.chars().next() {
            if c == '<' {
                let Some(end) = rest.find('>') else {
                    return Err(ParseKeyError(format!(
                        "`{notation}` has a `<` with no closing `>` (write `<lt>` for `<`)"
                    )));
                };
                keys.push(named(&rest[1..end], notation)?);
                rest = &rest[end + 1..];
            } else {
                keys.push(Key::char(c));
                rest = &rest[c.len_utf8()..];
            }
        }
        if keys.is_empty() {
            return Err(ParseKeyError("a key cannot be empty".to_owned()));
        }
        Ok(Self(keys))
    }
}

fn named(inner: &str, notation: &str) -> Result<Key, ParseKeyError> {
    let mut modifiers = KeyModifiers::NONE;
    let mut name = inner;
    while let Some((prefix, rest)) = name.split_once('-')
        && !rest.is_empty()
    {
        modifiers |= match prefix.to_ascii_uppercase().as_str() {
            "C" => KeyModifiers::CONTROL,
            "A" | "M" => KeyModifiers::ALT,
            "S" => KeyModifiers::SHIFT,
            _ => break,
        };
        name = rest;
    }
    let mut chars = name.chars();
    let code = match (chars.next(), chars.next()) {
        (Some(c), None) if modifiers.contains(KeyModifiers::SHIFT) => {
            return Err(ParseKeyError(format!(
                "`{notation}`: write the shifted character itself instead of `S-{c}`"
            )));
        }
        (Some(c), None) => KeyCode::Char(c),
        _ => NAMES
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|&(_, code)| code)
            .or_else(|| function_key(name))
            .ok_or_else(|| ParseKeyError(format!("`<{inner}>` is not a key")))?,
    };
    Ok(Key::new(code, modifiers))
}

fn function_key(name: &str) -> Option<KeyCode> {
    let number = name.strip_prefix(['F', 'f'])?.parse().ok()?;
    (1..=12).contains(&number).then_some(KeyCode::F(number))
}

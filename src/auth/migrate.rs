//! The one-time move of a plaintext `api_token` out of `config.toml`. It
//! removes that one line, keeping every other byte; besides it, ratatoist
//! writes only `[theme] name` there (`crate::config_write`).

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::ApiToken;
use crate::config::FILE;
use crate::config_write::replace;

/// An `api_token` in `config.toml` that cannot be moved into the keyring.
#[derive(Debug)]
pub(super) struct Stuck {
    pub(super) path: String,
    pub(super) why: &'static str,
}

/// A `config.toml` holding a top-level `api_token`.
pub(super) struct Legacy {
    path: PathBuf,
    text: String,
    token: ApiToken,
    /// The bytes of the line or lines the key and its value are on.
    lines: Range<usize>,
}

impl Legacy {
    /// The `api_token` in `config_dir/config.toml`, if the file holds one.
    /// `Ok(None)` when there is no file or no key; [`Stuck`] when the key is
    /// there but cannot be moved, because the file does not parse or the
    /// value is not a valid token.
    ///
    /// A symlinked `config.toml` (a dotfiles checkout, say) resolves to its
    /// target, so the rewrite edits the real file and leaves the link alone.
    pub(super) fn find(config_dir: &Path) -> Result<Option<Self>, Stuck> {
        let Ok(path) = fs::canonicalize(config_dir.join(FILE)) else {
            return Ok(None);
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(None);
        };
        let stuck = |why| Stuck {
            path: path.display().to_string(),
            why,
        };
        let document = match toml_edit::Document::parse(text.as_str()) {
            Ok(document) => document,
            Err(_) if mentions_api_token(&text) => return Err(stuck("the file does not parse")),
            Err(_) => return Ok(None),
        };
        let Some((key, item)) = document.as_table().get_key_value("api_token") else {
            return Ok(None);
        };
        let Some(token) = item.as_str().and_then(|raw| ApiToken::new(raw).ok()) else {
            return Err(stuck("its value is not a valid API token"));
        };
        let (Some(key), Some(item)) = (key.span(), item.span()) else {
            return Err(stuck("its position in the file is unknown"));
        };
        let lines = line_start(&text, key.start)..line_end(&text, item.end);
        Ok(Some(Self {
            path,
            text,
            token,
            lines,
        }))
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn token(&self) -> &ApiToken {
        &self.token
    }

    /// Rewrites the file without the `api_token` line, atomically and with
    /// the same mode (see [`replace`]). Returns the path on success, or why
    /// it failed.
    pub(super) fn remove(self) -> Result<String, String> {
        let shown = self.path.display().to_string();
        let mut kept = String::with_capacity(self.text.len());
        // `lines` starts and ends on line boundaries; see `line_start`.
        kept.push_str(&self.text[..self.lines.start]);
        kept.push_str(&self.text[self.lines.end..]);
        replace(&self.path, &kept)
            .map(|()| shown.clone())
            .map_err(|error| format!("could not remove api_token from {shown} ({error})"))
    }
}

/// Whether a line of a file that does not parse looks like `api_token = ...`.
fn mentions_api_token(text: &str) -> bool {
    text.lines().any(|line| {
        line.trim_start()
            .strip_prefix("api_token")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

// The byte slices here and in `remove` are safe: `offset` comes from a TOML
// span, which starts and ends on a token, and the cuts land on a `\n` or the
// text's ends, all character boundaries.
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Just past the newline ending the line `offset` is on, or the end of the
/// text.
fn line_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |newline| offset + newline + 1)
}

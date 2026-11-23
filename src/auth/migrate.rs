//! The one-time move of a plaintext `api_token` out of `config.toml`. This
//! is the only code that writes `config.toml`, and it removes that one line,
//! keeping every other byte.

use std::fs;
use std::ops::Range;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::ApiToken;
use crate::config::FILE;

/// A `config.toml` holding a top-level `api_token`.
pub(super) struct Legacy {
    path: PathBuf,
    text: String,
    token: ApiToken,
    /// The bytes of the line or lines the key and its value are on.
    lines: Range<usize>,
}

impl Legacy {
    /// The `api_token` in `config_dir/config.toml`, if the file parses and
    /// holds a valid one. A file that does not parse is left to the config
    /// loader to report.
    pub(super) fn find(config_dir: &Path) -> Option<Self> {
        let path = config_dir.join(FILE);
        let text = fs::read_to_string(&path).ok()?;
        let document = toml_edit::Document::parse(text.as_str()).ok()?;
        let (key, item) = document.as_table().get_key_value("api_token")?;
        let token = ApiToken::new(item.as_str()?).ok()?;
        let start = key.span()?.start;
        let end = item.span()?.end;
        let lines = line_start(&text, start)..line_end(&text, end);
        Some(Self {
            path,
            text,
            token,
            lines,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn token(&self) -> &ApiToken {
        &self.token
    }

    /// Rewrites the file without the `api_token` line, through a temporary
    /// file in the same folder with the same mode, renamed over it. Returns
    /// the path on success, or why it failed.
    pub(super) fn remove(self) -> Result<String, String> {
        let shown = self.path.display().to_string();
        let mut kept = String::with_capacity(self.text.len());
        kept.push_str(&self.text[..self.lines.start]);
        kept.push_str(&self.text[self.lines.end..]);
        let temporary = self.path.with_extension("toml.ratatoist-tmp");
        let mode = fs::metadata(&self.path)
            .map(|metadata| metadata.permissions().mode())
            .unwrap_or(0o600);
        let written = fs::write(&temporary, kept)
            .and_then(|()| fs::set_permissions(&temporary, fs::Permissions::from_mode(mode)))
            .and_then(|()| fs::rename(&temporary, &self.path));
        match written {
            Ok(()) => Ok(shown),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(format!("could not remove api_token from {shown} ({error})"))
            }
        }
    }
}

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

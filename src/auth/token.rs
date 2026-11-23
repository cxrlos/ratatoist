use std::fmt;

/// A Todoist API token. It prints as `<redacted>` in `Debug` and `Display`, so
/// it is safe in logs and error chains, and it deliberately has no
/// `Serialize`. Only [`ApiToken::expose`] reveals it, for the `Authorization`
/// header and the keyring.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiToken(String);

impl ApiToken {
    /// Trims surrounding whitespace (a piped token usually ends in a
    /// newline). Fails when nothing is left or the token holds a character
    /// that cannot go in an HTTP header.
    pub fn new(raw: &str) -> Result<Self, InvalidApiToken> {
        let token = raw.trim();
        if token.is_empty() {
            return Err(InvalidApiToken::Empty);
        }
        if !token.chars().all(|c| c.is_ascii_graphic()) {
            return Err(InvalidApiToken::BadCharacter);
        }
        Ok(Self(token.to_owned()))
    }

    /// The token itself. Never log or print the result.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiToken(<redacted>)")
    }
}

impl fmt::Display for ApiToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidApiToken {
    #[error("the API token is empty")]
    Empty,
    #[error(
        "the API token holds a space or a non-ASCII character; paste it exactly as Todoist shows it"
    )]
    BadCharacter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_redact() {
        let token = ApiToken::new("  0123456789abcdef\n").unwrap();
        assert_eq!(format!("{token:?}"), "ApiToken(<redacted>)");
        assert_eq!(format!("{token}"), "<redacted>");
        assert_eq!(token.expose(), "0123456789abcdef");
    }

    #[test]
    fn rejects_empty_and_spaced() {
        assert_eq!(ApiToken::new(" \n"), Err(InvalidApiToken::Empty));
        assert_eq!(ApiToken::new("ab cd"), Err(InvalidApiToken::BadCharacter));
    }
}

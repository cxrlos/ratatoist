//! The Todoist API token: where it comes from, where it is kept, and the
//! `ratatoist auth` commands.
//!
//! The token comes from `TODOIST_API_TOKEN` when that is set, otherwise from
//! the OS keyring entry with service [`SERVICE`] and account [`ACCOUNT`].
//! There is no plaintext fallback: an `api_token` left in `config.toml` by an
//! older version is moved into the keyring once ([`resolve_from`]) and never
//! used from the file.

pub mod keyring;
mod migrate;
pub mod token;

use std::fmt;
use std::io::{self, Write};
use std::path::Path;

pub use keyring::{Keyring, KeyringError, MemoryKeyring, OsKeyring};
pub use token::{ApiToken, InvalidApiToken};

use crate::cli::AuthCommand;
use crate::todoist::{ClientError, HttpClient, TodoistClient};

/// The keyring service of ratatoist's entry.
pub const SERVICE: &str = "ratatoist";

/// The keyring account of ratatoist's entry; there is only ever one.
pub const ACCOUNT: &str = "api-token";

/// The environment variable that overrides the keyring.
pub const TOKEN_VAR: &str = "TODOIST_API_TOKEN";

/// Where a user finds their API token.
pub const TOKEN_URL: &str = "https://app.todoist.com/app/settings/integrations/developer";

/// Where a resolved token came from, for `ratatoist auth status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Env,
    Keyring,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Env => f.write_str(TOKEN_VAR),
            Self::Keyring => write!(f, "the OS keyring (service {SERVICE})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub token: ApiToken,
    pub source: Source,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("No Todoist token: run \"ratatoist auth login\", or set {TOKEN_VAR}.")]
    NoToken,
    #[error(
        "Todoist rejected this token: it is wrong or revoked. Nothing was stored. \
         Copy the API token from {TOKEN_URL} and try again"
    )]
    LoginRejected,
    #[error("{}", rejected(.0))]
    Rejected(Source),
    #[error(transparent)]
    Malformed(InvalidApiToken),
    #[error("{TOKEN_VAR} does not hold a valid API token: {0}")]
    MalformedEnv(InvalidApiToken),
    #[error(transparent)]
    Keyring(#[from] KeyringError),
    #[error(
        "could not move the api_token in {path} into the OS keyring, so the file is unchanged: {source}"
    )]
    Migration { path: String, source: KeyringError },
    #[error(
        "No Todoist token: {path} has an api_token, but {why}, so it was not moved into the OS keyring. \
         Fix or delete that line, then run \"ratatoist auth login\", or set {TOKEN_VAR}."
    )]
    Unmovable { path: String, why: &'static str },
    #[error("reading the API token")]
    ReadToken(#[source] io::Error),
    #[error("setting up the Todoist client: {0}")]
    Connect(String),
    #[error("checking the token with Todoist")]
    Client(#[source] ClientError),
}

/// The token to use: `TODOIST_API_TOKEN`, then the OS keyring, after moving
/// a legacy `api_token` from `config.toml` into the keyring. What the move
/// did is printed to stderr.
pub fn resolve() -> anyhow::Result<Resolved> {
    let dir = crate::config::dir_to_read(&crate::paths::config_dir()?);
    let env = std::env::var(TOKEN_VAR).ok();
    Ok(resolve_from(env, &OsKeyring, &dir, &mut io::stderr())?)
}

/// [`resolve`] with its inputs given: `env` is the value of
/// `TODOIST_API_TOKEN` (blank counts as unset), `config_dir` holds
/// `config.toml`, and one line per thing the migration did goes to `notes`.
///
/// The migration runs only when the keyring is consulted, so a machine
/// that uses `TODOIST_API_TOKEN` and has no keyring is never asked for one.
/// With `api_token` in the file and an empty keyring, the token is stored and
/// the line removed; when the keyring already holds a token, that one wins
/// and the line is removed too. If the keyring is unavailable the file is
/// left alone and the error is returned.
pub fn resolve_from(
    env: Option<String>,
    keyring: &impl Keyring,
    config_dir: &Path,
    notes: &mut impl Write,
) -> Result<Resolved, AuthError> {
    if let Some(raw) = env.filter(|raw| !raw.trim().is_empty()) {
        return Ok(Resolved {
            token: ApiToken::new(&raw).map_err(AuthError::MalformedEnv)?,
            source: Source::Env,
        });
    }
    let (legacy, stuck) = match migrate::Legacy::find(config_dir) {
        Ok(legacy) => (legacy, None),
        Err(stuck) => (None, Some(stuck)),
    };
    let stored = match (keyring.get(), &legacy) {
        (Ok(stored), _) => stored,
        (Err(source), Some(legacy)) => {
            return Err(AuthError::Migration {
                path: legacy.path().display().to_string(),
                source,
            });
        }
        (Err(error), None) => return Err(error.into()),
    };
    let token = match (stored, legacy) {
        (Some(token), None) => token,
        (None, None) => {
            return Err(
                stuck.map_or(AuthError::NoToken, |stuck| AuthError::Unmovable {
                    path: stuck.path,
                    why: stuck.why,
                }),
            );
        }
        (Some(token), Some(legacy)) => {
            note(
                notes,
                legacy.remove(),
                "the OS keyring already holds a token",
            );
            token
        }
        (None, Some(legacy)) => {
            let token = legacy.token().clone();
            keyring.set(&token).map_err(|source| AuthError::Migration {
                path: legacy.path().display().to_string(),
                source,
            })?;
            note(
                notes,
                legacy.remove(),
                "moved the token into the OS keyring",
            );
            token
        }
    };
    Ok(Resolved {
        token,
        source: Source::Keyring,
    })
}

/// Writes the one line saying what the migration did. A failed write to
/// stderr has nowhere to be reported, so it is dropped.
fn note(notes: &mut impl Write, removed: Result<String, String>, what: &str) {
    let line = match removed {
        Ok(path) => format!("ratatoist {what} and removed api_token from {path}."),
        Err(problem) => format!("ratatoist {what}, but {problem}; delete that line by hand."),
    };
    let _ = writeln!(notes, "{line}");
}

/// Reads a token piped to `ratatoist auth login --stdin`: the first line.
pub fn read_token(input: impl io::BufRead) -> Result<ApiToken, AuthError> {
    let mut lines = input.lines();
    let raw = lines
        .next()
        .transpose()
        .map_err(AuthError::ReadToken)?
        .unwrap_or_default();
    ApiToken::new(&raw).map_err(AuthError::Malformed)
}

/// Asks for a token on the terminal without echoing it.
pub fn prompt_token() -> Result<ApiToken, AuthError> {
    let raw = rpassword::prompt_password("Todoist API token: ").map_err(AuthError::ReadToken)?;
    ApiToken::new(&raw).map_err(AuthError::Malformed)
}

/// `ratatoist auth login`: checks `token` with Todoist, then stores it in the
/// keyring. `env_set` says whether `TODOIST_API_TOKEN` is set, which wins
/// over what is stored. Returns what to print.
pub async fn login<K, C>(
    keyring: &K,
    token: ApiToken,
    env_set: bool,
    connect: impl FnOnce(&ApiToken) -> anyhow::Result<C>,
) -> Result<String, AuthError>
where
    K: Keyring,
    C: TodoistClient,
{
    let user = connect(&token)
        .map_err(|error| AuthError::Connect(format!("{error:#}")))?
        .user()
        .await
        .map_err(|error| match error {
            ClientError::Unauthorized => AuthError::LoginRejected,
            other => AuthError::Client(other),
        })?;
    keyring.set(&token)?;
    let mut output = format!(
        "Logged in as {}.\nThe token is in {}.",
        account(&user),
        Source::Keyring
    );
    if env_set {
        output.push_str(&format!(
            "\n{TOKEN_VAR} is set, so ratatoist uses it instead of the keyring."
        ));
    }
    Ok(output)
}

/// `ratatoist auth status`: where the token in use comes from and whose
/// account it is. Never prints the token.
pub async fn status<C: TodoistClient>(
    resolved: &Resolved,
    connect: impl FnOnce(&ApiToken) -> anyhow::Result<C>,
) -> Result<String, AuthError> {
    let user = connect(&resolved.token)
        .map_err(|error| AuthError::Connect(format!("{error:#}")))?
        .user()
        .await
        .map_err(|error| match error {
            ClientError::Unauthorized => AuthError::Rejected(resolved.source),
            other => AuthError::Client(other),
        })?;
    Ok(format!(
        "Token source: {}\nAccount:      {}",
        resolved.source,
        account(&user)
    ))
}

/// `ratatoist auth logout`: deletes ratatoist's keyring entry.
/// `TODOIST_API_TOKEN` is left alone.
pub fn logout(keyring: &impl Keyring, env_set: bool) -> Result<String, AuthError> {
    let mut output = if keyring.delete()? {
        format!("Removed the token from {}.", Source::Keyring)
    } else {
        format!("No token was stored in {}.", Source::Keyring)
    };
    if env_set {
        output.push_str(&format!(
            "\n{TOKEN_VAR} is still set, so ratatoist keeps using it."
        ));
    }
    Ok(output)
}

/// Runs `ratatoist auth login | status | logout` against the real keyring
/// and Todoist.
pub async fn run(command: AuthCommand) -> anyhow::Result<()> {
    let env_set = std::env::var(TOKEN_VAR).is_ok_and(|raw| !raw.trim().is_empty());
    let output = match command {
        AuthCommand::Login { stdin } => {
            let token = if stdin {
                read_token(io::stdin().lock())?
            } else {
                prompt_token()?
            };
            login(&OsKeyring, token, env_set, HttpClient::new).await?
        }
        AuthCommand::Status => status(&resolve()?, HttpClient::new).await?,
        AuthCommand::Logout => logout(&OsKeyring, env_set)?,
    };
    println!("{output}");
    Ok(())
}

/// The account's name and email, as far as Todoist gave them.
fn account(user: &crate::todoist::model::UserInfo) -> String {
    match (user.full_name.as_deref(), user.email.as_deref()) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (Some(name), None) => name.to_owned(),
        (None, Some(email)) => email.to_owned(),
        (None, None) => format!("user {}", user.id),
    }
}

/// Why the token from `source` failed and how to replace it. A token from
/// `TODOIST_API_TOKEN` wins over the keyring, so logging in again would not
/// help.
fn rejected(source: &Source) -> String {
    let fix = match source {
        Source::Env => {
            format!("put a new one in {TOKEN_VAR}, or unset {TOKEN_VAR} to use the OS keyring")
        }
        Source::Keyring => "run \"ratatoist auth login\"".to_owned(),
    };
    format!(
        "Todoist rejected the token from {source}: it is wrong or revoked. \
         Copy the API token from {TOKEN_URL}, then {fix}"
    )
}

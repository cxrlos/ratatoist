// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use futures_util::Stream;
use ratatoist::auth::keyring::{Keyring, KeyringError, MemoryKeyring};
use ratatoist::auth::{self, ApiToken, AuthError, Resolved, Source};
use ratatoist::cli::AuthCommand;
use ratatoist::todoist::model::{Comment, Task, UserInfo};
use ratatoist::todoist::sync::{SyncRequest, SyncResponse};
use ratatoist::todoist::{
    ClientConfig, ClientError, CompletedQuery, HttpClient, LiveMessage, TodoistClient,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER_SECRET: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn token(raw: &str) -> ApiToken {
    ApiToken::new(raw).unwrap()
}

/// A config folder for one test, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-auth-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn with_config(test: &str, text: &str) -> Self {
        let dir = Self::new(test);
        fs::write(dir.config(), text).unwrap();
        dir
    }

    fn config(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    fn read_config(&self) -> String {
        fs::read_to_string(self.config()).unwrap()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A keyring that cannot be reached, as on a headless box with no Secret
/// Service running.
struct UnavailableKeyring;

fn unavailable() -> KeyringError {
    KeyringError::new("org.freedesktop.secrets was not provided by any .service files")
}

impl Keyring for UnavailableKeyring {
    fn get(&self) -> Result<Option<ApiToken>, KeyringError> {
        Err(unavailable())
    }

    fn set(&self, _: &ApiToken) -> Result<(), KeyringError> {
        Err(unavailable())
    }

    fn delete(&self) -> Result<bool, KeyringError> {
        Err(unavailable())
    }
}

fn resolve(
    env: Option<&str>,
    keyring: &impl Keyring,
    dir: &Path,
) -> (Result<Resolved, AuthError>, String) {
    let mut notes = Vec::new();
    let resolved = auth::resolve_from(env.map(str::to_owned), keyring, dir, &mut notes);
    (resolved, String::from_utf8(notes).unwrap())
}

/// A client whose `user()` answers with `user`; nothing else is called.
#[derive(Clone)]
struct StubClient {
    user: Result<UserInfo, ClientError>,
}

impl StubClient {
    fn ada() -> Self {
        Self {
            user: Ok(UserInfo {
                id: "1".to_owned(),
                full_name: Some("Ada Lovelace".to_owned()),
                email: Some("ada@example.com".to_owned()),
                websocket_url: None,
            }),
        }
    }

    fn rejecting() -> Self {
        Self {
            user: Err(ClientError::Unauthorized),
        }
    }

    fn connect(self) -> impl FnOnce(&ApiToken) -> anyhow::Result<Self> {
        move |_| Ok(self)
    }
}

impl TodoistClient for StubClient {
    async fn sync(&self, _: SyncRequest) -> Result<SyncResponse, ClientError> {
        Err(ClientError::Other("not used".to_owned()))
    }

    async fn user(&self) -> Result<UserInfo, ClientError> {
        self.user.clone()
    }

    async fn comments(&self, _: &str) -> Result<Vec<Comment>, ClientError> {
        Err(ClientError::Other("not used".to_owned()))
    }

    async fn completed_tasks(&self, _: CompletedQuery) -> Result<Vec<Task>, ClientError> {
        Err(ClientError::Other("not used".to_owned()))
    }

    fn live_events(&self, _: &str) -> impl Stream<Item = LiveMessage> + Send + 'static {
        futures_util::stream::empty()
    }
}

// Resolution

#[test]
fn the_environment_wins_over_the_keyring() {
    let dir = TempDir::new("envwins");
    let keyring = MemoryKeyring::holding(token(OTHER_SECRET));

    let (resolved, _) = resolve(Some(SECRET), &keyring, &dir.0);

    let resolved = resolved.unwrap();
    assert_eq!(resolved.source, Source::Env);
    assert_eq!(resolved.token.expose(), SECRET);
}

#[test]
fn a_blank_environment_variable_counts_as_unset() {
    let dir = TempDir::new("blankenv");
    let keyring = MemoryKeyring::holding(token(SECRET));

    let (resolved, _) = resolve(Some("  \n"), &keyring, &dir.0);

    assert_eq!(resolved.unwrap().source, Source::Keyring);
}

#[test]
fn the_keyring_is_used_without_the_environment() {
    let dir = TempDir::new("keyring");
    let keyring = MemoryKeyring::holding(token(SECRET));

    let (resolved, notes) = resolve(None, &keyring, &dir.0);

    let resolved = resolved.unwrap();
    assert_eq!(resolved.source, Source::Keyring);
    assert_eq!(resolved.token.expose(), SECRET);
    assert_eq!(notes, "");
}

#[test]
fn with_no_token_anywhere_the_error_says_how_to_add_one() {
    let dir = TempDir::new("none");

    let (resolved, _) = resolve(None, &MemoryKeyring::default(), &dir.0);

    assert_eq!(
        resolved.unwrap_err().to_string(),
        "No Todoist token: run \"ratatoist auth login\", or set TODOIST_API_TOKEN."
    );
}

#[test]
fn an_unavailable_keyring_names_the_environment_variable() {
    let dir = TempDir::new("unavailable");

    let (resolved, _) = resolve(None, &UnavailableKeyring, &dir.0);

    let message = resolved.unwrap_err().to_string();
    assert!(message.contains("keyring is unavailable"), "{message}");
    assert!(message.contains("TODOIST_API_TOKEN"), "{message}");
}

#[test]
fn the_environment_never_touches_an_unavailable_keyring() {
    let dir = TempDir::new("envnokeyring");

    let (resolved, _) = resolve(Some(SECRET), &UnavailableKeyring, &dir.0);

    assert_eq!(resolved.unwrap().source, Source::Env);
}

#[test]
fn a_resolved_token_never_prints() {
    let dir = TempDir::new("redact");
    let (resolved, _) = resolve(Some(SECRET), &MemoryKeyring::default(), &dir.0);

    let resolved = resolved.unwrap();
    assert!(!format!("{resolved:?}").contains(SECRET));
    assert_eq!(format!("{}", resolved.token), "<redacted>");
    assert_eq!(format!("{:?}", resolved.token), "ApiToken(<redacted>)");
}

// Migration

const LEGACY: &str = "# my settings\napi_token = \"0123456789abcdef0123456789abcdef01234567\"\n\n[sync]\nidle-timeout = 60 # a minute\n";
const MIGRATED: &str = "# my settings\n\n[sync]\nidle-timeout = 60 # a minute\n";

#[test]
fn the_config_token_moves_into_an_empty_keyring() {
    let dir = TempDir::with_config("migrate", LEGACY);
    let keyring = MemoryKeyring::default();

    let (resolved, notes) = resolve(None, &keyring, &dir.0);

    let resolved = resolved.unwrap();
    assert_eq!(resolved.source, Source::Keyring);
    assert_eq!(resolved.token.expose(), SECRET);
    assert_eq!(keyring.stored().unwrap().expose(), SECRET);
    assert_eq!(dir.read_config(), MIGRATED);
    assert_eq!(notes.lines().count(), 1, "{notes}");
    assert!(notes.contains("OS keyring"), "{notes}");
    assert!(!notes.contains(SECRET));
}

#[test]
fn migration_keeps_the_file_mode() {
    let dir = TempDir::with_config("migratemode", LEGACY);
    fs::set_permissions(dir.config(), fs::Permissions::from_mode(0o600)).unwrap();

    let (resolved, _) = resolve(None, &MemoryKeyring::default(), &dir.0);

    assert!(resolved.is_ok());
    let mode = fs::metadata(dir.config()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn migration_runs_once() {
    let dir = TempDir::with_config("once", LEGACY);
    let keyring = MemoryKeyring::default();
    let _ = resolve(None, &keyring, &dir.0);

    let (resolved, notes) = resolve(None, &keyring, &dir.0);

    assert_eq!(resolved.unwrap().source, Source::Keyring);
    assert_eq!(notes, "");
    assert_eq!(dir.read_config(), MIGRATED);
}

#[test]
fn an_unavailable_keyring_leaves_the_config_alone() {
    let dir = TempDir::with_config("migratefail", LEGACY);

    let (resolved, notes) = resolve(None, &UnavailableKeyring, &dir.0);

    let message = resolved.unwrap_err().to_string();
    assert!(message.contains("api_token"), "{message}");
    assert!(message.contains("TODOIST_API_TOKEN"), "{message}");
    assert!(!message.contains(SECRET));
    assert_eq!(notes, "");
    assert_eq!(dir.read_config(), LEGACY);
}

#[test]
fn a_token_already_in_the_keyring_wins_and_the_config_copy_goes() {
    let dir = TempDir::with_config("keyringwins", LEGACY);
    let keyring = MemoryKeyring::holding(token(OTHER_SECRET));

    let (resolved, notes) = resolve(None, &keyring, &dir.0);

    assert_eq!(resolved.unwrap().token.expose(), OTHER_SECRET);
    assert_eq!(keyring.stored().unwrap().expose(), OTHER_SECRET);
    assert_eq!(dir.read_config(), MIGRATED);
    assert_eq!(notes.lines().count(), 1, "{notes}");
}

#[test]
fn the_environment_skips_migration() {
    let dir = TempDir::with_config("envskips", LEGACY);
    let keyring = MemoryKeyring::default();

    let (resolved, notes) = resolve(Some(OTHER_SECRET), &keyring, &dir.0);

    assert_eq!(resolved.unwrap().source, Source::Env);
    assert_eq!(keyring.stored(), None);
    assert_eq!(notes, "");
    assert_eq!(dir.read_config(), LEGACY);
}

#[test]
fn a_read_only_config_folder_keeps_the_token_moved_and_says_to_delete_the_line() {
    let dir = TempDir::with_config("migratereadonly", LEGACY);
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o555)).unwrap();
    let keyring = MemoryKeyring::default();

    let (resolved, notes) = resolve(None, &keyring, &dir.0);

    assert_eq!(resolved.unwrap().source, Source::Keyring);
    assert_eq!(keyring.stored().unwrap().expose(), SECRET);
    assert_eq!(dir.read_config(), LEGACY);
    assert!(notes.contains("by hand"), "{notes}");
}

#[test]
fn a_config_that_does_not_parse_is_left_alone() {
    let text = "api_token = \"0123456789abcdef0123456789abcdef01234567\"\n[sync\n";
    let dir = TempDir::with_config("migratebroken", text);

    let (resolved, _) = resolve(None, &MemoryKeyring::default(), &dir.0);

    let message = resolved.unwrap_err().to_string();
    assert!(message.contains("does not parse"), "{message}");
    assert!(message.contains("api_token"), "{message}");
    assert!(message.contains("ratatoist auth login"), "{message}");
    assert!(!message.contains(SECRET));
    assert_eq!(dir.read_config(), text);
}

#[test]
fn an_invalid_config_token_is_reported_not_ignored() {
    let text = "api_token = \"not a token\"\n";
    let dir = TempDir::with_config("migrateinvalid", text);

    let (resolved, _) = resolve(None, &MemoryKeyring::default(), &dir.0);

    let message = resolved.unwrap_err().to_string();
    assert!(message.contains("api_token"), "{message}");
    assert!(message.contains("not a valid API token"), "{message}");
    assert!(!message.contains("not a token"), "{message}");
    assert_eq!(dir.read_config(), text);
}

#[test]
fn a_keyring_token_still_wins_over_a_config_token_that_cannot_move() {
    let text = "api_token = \"0123456789abcdef0123456789abcdef01234567\"\n[sync\n";
    let dir = TempDir::with_config("migratebrokenkeyring", text);

    let (resolved, notes) = resolve(None, &MemoryKeyring::holding(token(OTHER_SECRET)), &dir.0);

    assert_eq!(resolved.unwrap().token.expose(), OTHER_SECRET);
    assert_eq!(notes, "");
    assert_eq!(dir.read_config(), text);
}

#[test]
fn migration_edits_the_target_of_a_symlinked_config_and_keeps_the_link() {
    let dir = TempDir::new("migratesymlink");
    let dotfiles = dir.0.join("dotfiles");
    let config = dir.0.join("config");
    fs::create_dir_all(&dotfiles).unwrap();
    fs::create_dir_all(&config).unwrap();
    let target = dotfiles.join("config.toml");
    fs::write(&target, LEGACY).unwrap();
    std::os::unix::fs::symlink(&target, config.join("config.toml")).unwrap();
    let keyring = MemoryKeyring::default();

    let (resolved, notes) = resolve(None, &keyring, &config);

    assert_eq!(resolved.unwrap().source, Source::Keyring);
    assert_eq!(keyring.stored().unwrap().expose(), SECRET);
    let link = fs::symlink_metadata(config.join("config.toml")).unwrap();
    assert!(link.file_type().is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), MIGRATED);
    assert_eq!(fs::read_dir(&dotfiles).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&config).unwrap().count(), 1);
    assert!(notes.contains("dotfiles"), "{notes}");
}

// Commands

#[tokio::test]
async fn login_checks_the_token_then_stores_it() {
    let keyring = MemoryKeyring::default();

    let output = auth::login(&keyring, token(SECRET), false, StubClient::ada().connect())
        .await
        .unwrap();

    assert_eq!(keyring.stored().unwrap().expose(), SECRET);
    assert!(output.contains("Ada Lovelace"), "{output}");
    assert!(output.contains("ada@example.com"), "{output}");
    assert!(!output.contains(SECRET));
    assert!(!output.contains("TODOIST_API_TOKEN is set"));
}

#[tokio::test]
async fn login_rejects_a_token_todoist_refuses_and_stores_nothing() {
    let keyring = MemoryKeyring::default();

    let error = auth::login(
        &keyring,
        token(SECRET),
        false,
        StubClient::rejecting().connect(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, AuthError::LoginRejected), "{error}");
    assert!(error.to_string().contains("Nothing was stored"));
    assert_eq!(keyring.stored(), None);
}

/// The same through the real client: a 401 from the user route.
#[tokio::test]
async fn login_rejects_a_real_401_and_stores_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Forbidden"))
        .expect(1)
        .mount(&server)
        .await;
    let keyring = MemoryKeyring::default();
    let config = ClientConfig {
        base_url: server.uri(),
        ..ClientConfig::default()
    };

    let error = auth::login(&keyring, token(SECRET), false, |token| {
        HttpClient::with_config(token, config)
    })
    .await
    .unwrap_err();

    assert!(matches!(error, AuthError::LoginRejected), "{error}");
    assert_eq!(keyring.stored(), None);
}

#[tokio::test]
async fn login_says_when_the_environment_still_wins() {
    let output = auth::login(
        &MemoryKeyring::default(),
        token(SECRET),
        true,
        StubClient::ada().connect(),
    )
    .await
    .unwrap();

    assert!(output.contains("TODOIST_API_TOKEN is set"), "{output}");
}

#[tokio::test]
async fn status_names_the_source_and_account_never_the_token() {
    let resolved = Resolved {
        token: token(SECRET),
        source: Source::Env,
    };

    let output = auth::status(&resolved, StubClient::ada().connect())
        .await
        .unwrap();

    assert!(output.contains("TODOIST_API_TOKEN"), "{output}");
    assert!(output.contains("Ada Lovelace"), "{output}");
    assert!(output.contains("ada@example.com"), "{output}");
    assert!(!output.contains(SECRET));
}

#[tokio::test]
async fn status_names_the_keyring_source() {
    let resolved = Resolved {
        token: token(SECRET),
        source: Source::Keyring,
    };

    let output = auth::status(&resolved, StubClient::ada().connect())
        .await
        .unwrap();

    assert!(output.contains("OS keyring"), "{output}");
}

#[tokio::test]
async fn status_reports_a_rejected_token_with_its_fix() {
    let resolved = Resolved {
        token: token(SECRET),
        source: Source::Keyring,
    };

    let error = auth::status(&resolved, StubClient::rejecting().connect())
        .await
        .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("rejected"), "{message}");
    assert!(message.contains("ratatoist auth login"), "{message}");
}

#[test]
fn logout_removes_the_entry() {
    let keyring = MemoryKeyring::holding(token(SECRET));

    let first = auth::logout(&keyring, false).unwrap();
    let second = auth::logout(&keyring, false).unwrap();

    assert_eq!(keyring.stored(), None);
    assert!(first.starts_with("Removed"), "{first}");
    assert!(second.starts_with("No token"), "{second}");
}

#[test]
fn logout_says_when_the_environment_is_still_set() {
    let output = auth::logout(&MemoryKeyring::default(), true).unwrap();

    assert!(
        output.contains("TODOIST_API_TOKEN is still set"),
        "{output}"
    );
}

#[test]
fn a_piped_token_is_trimmed_and_an_empty_one_refused() {
    let read = auth::read_token(format!("{SECRET}\n").as_bytes()).unwrap();
    assert_eq!(read.expose(), SECRET);

    let error = auth::read_token(&b"\n"[..]).unwrap_err();
    assert!(error.to_string().contains("empty"), "{error}");
}

// What `ratatoist auth` prints, through `run_with` as the binary runs it

/// What one `ratatoist auth` command wrote, and its error as `main` prints it.
struct Printed {
    out: String,
    notes: String,
    error: Option<String>,
}

impl Printed {
    fn assert_no_token(&self) {
        for text in [&self.out, &self.notes] {
            assert!(!text.contains(SECRET), "{text}");
        }
        if let Some(error) = &self.error {
            assert!(!error.contains(SECRET), "{error}");
        }
    }
}

/// A Todoist whose `/user` answers with `status`, the fixture body on a 200.
async fn todoist(status: u16) -> MockServer {
    let server = MockServer::start().await;
    let body = if status == 200 {
        fs::read_to_string("tests/fixtures/todoist/HYPOTHETICAL_user.json").unwrap()
    } else {
        "Forbidden".to_owned()
    };
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .expect(1)
        .mount(&server)
        .await;
    server
}

async fn run_auth(
    command: AuthCommand,
    keyring: &MemoryKeyring,
    env: Option<&str>,
    input: &str,
    server: &MockServer,
    test: &str,
) -> Printed {
    let dir = TempDir::new(test);
    let config_dir = dir.0.clone();
    let config = ClientConfig {
        base_url: server.uri(),
        ..ClientConfig::default()
    };
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let result = auth::run_with(
        command,
        keyring,
        env.map(str::to_owned),
        move || Ok(config_dir),
        |token| HttpClient::with_config(token, config),
        auth::Console {
            input: std::io::Cursor::new(input.to_owned()),
            out: &mut out,
            notes: &mut notes,
        },
    )
    .await;
    Printed {
        out: String::from_utf8(out).unwrap(),
        notes: String::from_utf8(notes).unwrap(),
        error: result.err().map(|error| format!("{error:#}")),
    }
}

#[tokio::test]
async fn auth_login_stdin_stores_the_token_and_never_prints_it() {
    let server = todoist(200).await;
    let keyring = MemoryKeyring::default();

    let printed = run_auth(
        AuthCommand::Login { stdin: true },
        &keyring,
        None,
        &format!("{SECRET}\n"),
        &server,
        "runlogin",
    )
    .await;

    assert_eq!(printed.error, None);
    assert!(printed.out.contains("Wren Example"), "{}", printed.out);
    assert_eq!(keyring.stored().unwrap().expose(), SECRET);
    printed.assert_no_token();
}

#[tokio::test]
async fn auth_login_stdin_rejected_stores_nothing_and_never_prints_the_token() {
    let server = todoist(401).await;
    let keyring = MemoryKeyring::default();

    let printed = run_auth(
        AuthCommand::Login { stdin: true },
        &keyring,
        None,
        &format!("{SECRET}\n"),
        &server,
        "runloginrejected",
    )
    .await;

    let error = printed.error.as_deref().unwrap();
    assert!(error.contains("Nothing was stored"), "{error}");
    assert_eq!(keyring.stored(), None);
    printed.assert_no_token();
}

#[tokio::test]
async fn auth_status_never_prints_the_token_from_either_source() {
    for (source, status) in [
        ("keyring", 200),
        ("keyring", 401),
        ("env", 200),
        ("env", 401),
    ] {
        let server = todoist(status).await;
        let (keyring, env) = match source {
            "keyring" => (MemoryKeyring::holding(token(SECRET)), None),
            _ => (MemoryKeyring::default(), Some(SECRET)),
        };

        let printed = run_auth(
            AuthCommand::Status,
            &keyring,
            env,
            "",
            &server,
            &format!("runstatus{source}{status}"),
        )
        .await;

        assert_eq!(printed.error.is_none(), status == 200, "{source} {status}");
        if status == 200 {
            assert!(printed.out.contains("Wren Example"), "{}", printed.out);
        }
        printed.assert_no_token();
    }
}

#[tokio::test]
async fn auth_logout_empties_the_keyring_and_never_prints_the_token() {
    let server = MockServer::start().await;
    let keyring = MemoryKeyring::holding(token(SECRET));

    let printed = run_auth(
        AuthCommand::Logout,
        &keyring,
        None,
        "",
        &server,
        "runlogout",
    )
    .await;

    assert_eq!(printed.error, None);
    assert!(printed.out.starts_with("Removed"), "{}", printed.out);
    assert_eq!(keyring.stored(), None);
    printed.assert_no_token();
}

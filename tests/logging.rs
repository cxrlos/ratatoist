//! Where logs go and what they hold: a daily file under the directory main
//! passes, at the level in `RATATOIST_LOG`, never the terminal and never the
//! token.

// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use ratatoist::auth::ApiToken;
use ratatoist::logging;
use ratatoist::todoist::HttpClient;
use tracing_subscriber::EnvFilter;

/// Not a real token: it must never reach a log line.
const FAKE_TOKEN: &str = "0123456789abcdef_fake_token_never_in_a_log";
/// Set to the log directory to make [`logs_when_asked`] start logging.
const LOG_DIR: &str = "RATATOIST_TEST_LOG_DIR";
/// Set to make [`handshakes_when_asked`] open a WebSocket.
const HANDSHAKE: &str = "RATATOIST_TEST_HANDSHAKE";
const INFO_LINE: &str = "an info line from the logging test";
const DEBUG_LINE: &str = "a debug line from the logging test";

/// A fresh directory under the system temp directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("ratatoist-logging-{}-{n}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The names of the files in `dir`, sorted.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Everything logged to the files in `dir`.
fn logged(dir: &Path) -> String {
    file_names(dir)
        .iter()
        .map(|name| fs::read_to_string(dir.join(name)).unwrap())
        .collect()
}

#[test]
fn the_token_never_reaches_a_trace_log() {
    let dir = TempDir::new();
    let token = ApiToken::new(FAKE_TOKEN).unwrap();
    let (writer, guard) = tracing_appender::non_blocking(logging::appender(&dir.0).unwrap());

    tracing::subscriber::with_default(logging::subscriber(writer, EnvFilter::new("trace")), || {
        let client = HttpClient::new(&token).unwrap();
        tracing::trace!(?token, %token, "sending a request");
        drop(client);
    });
    drop(guard);

    let logged = logged(&dir.0);
    assert!(logged.contains("sending a request"), "{logged}");
    assert!(logged.contains("<redacted>"), "{logged}");
    assert!(!logged.contains(FAKE_TOKEN), "{logged}");
}

/// The WebSocket URL carries the token as a query parameter, and at trace
/// tungstenite logs the whole upgrade request. tungstenite logs through the
/// `log` crate, which reaches the file only through the bridge [`logging::init`]
/// installs, so this runs [`handshakes_when_asked`] in a child process.
#[test]
fn the_token_in_a_websocket_url_never_reaches_a_trace_log() {
    let dir = TempDir::new();

    log_in_a_child(
        "handshakes_when_asked",
        &dir.0,
        &[("RATATOIST_LOG", "trace"), (HANDSHAKE, "1")],
    );

    let logged = logged(&dir.0);
    assert!(logged.contains("tungstenite::"), "{logged}");
    assert!(!logged.contains(FAKE_TOKEN), "{logged}");
}

#[test]
fn logs_roll_daily_and_keep_seven_files() {
    let dir = TempDir::new();
    for day in 1..=10 {
        fs::write(
            dir.0.join(format!("ratatoist.2026-10-{day:02}.log")),
            "old\n",
        )
        .unwrap();
    }

    let appender = logging::appender(&dir.0).unwrap();
    drop(appender);

    let names = file_names(&dir.0);
    assert_eq!(names.len(), 7, "{names:?}");
    let today = chrono::Utc::now()
        .format("ratatoist.%Y-%m-%d.log")
        .to_string();
    assert!(names.contains(&today), "{names:?}");
}

/// Passes at once unless [`LOG_DIR`] is set, when it starts logging there
/// as main does and logs one line at info and one at debug. The tests below
/// run it from this same test binary, each in its own process, because a
/// process can start logging only once.
#[test]
fn logs_when_asked() {
    let Some(dir) = std::env::var_os(LOG_DIR) else {
        return;
    };
    let guard = logging::init(Path::new(&dir)).unwrap();
    tracing::info!(target: "ratatoist::logging_test", "{INFO_LINE}");
    tracing::debug!(target: "ratatoist::logging_test", "{DEBUG_LINE}");
    drop(guard);
}

/// Passes at once unless [`HANDSHAKE`] and [`LOG_DIR`] are set, when it
/// starts logging there as main does and opens a WebSocket to a local
/// listener with the fake token in the URL, as the app's refresh does.
#[test]
fn handshakes_when_asked() {
    let (Some(dir), Some(_)) = (std::env::var_os(LOG_DIR), std::env::var_os(HANDSHAKE)) else {
        return;
    };
    let guard = logging::init(Path::new(&dir)).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "ws://{}/updates?token={FAKE_TOKEN}",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });
        let (client, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        let server = server.await.unwrap();
        drop((client, server));
    });
    drop(guard);
}

/// Runs the test `name` (one of the `*_when_asked` tests) in a child process
/// logging to `dir`, with only the environment variables `env` set besides
/// PATH.
fn log_in_a_child(name: &str, dir: &Path, env: &[(&str, &str)]) -> Output {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env(LOG_DIR, dir)
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    output
}

#[test]
fn logs_go_to_a_file_in_the_log_dir_at_info_by_default_and_never_to_the_terminal() {
    let dir = TempDir::new();
    let log_dir = dir.0.join("state").join("ratatoist");

    let output = log_in_a_child("logs_when_asked", &log_dir, &[]);

    let names = file_names(&log_dir);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].starts_with("ratatoist.") && names[0].ends_with(".log"),
        "{names:?}"
    );
    let logged = logged(&log_dir);
    assert!(logged.contains(INFO_LINE), "{logged}");
    assert!(!logged.contains(DEBUG_LINE), "{logged}");
    let terminal = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!terminal.contains(INFO_LINE), "{terminal}");
}

#[test]
fn ratatoist_log_raises_the_level() {
    let dir = TempDir::new();

    log_in_a_child("logs_when_asked", &dir.0, &[("RATATOIST_LOG", "debug")]);

    let logged = logged(&dir.0);
    assert!(logged.contains(DEBUG_LINE), "{logged}");
}

#[test]
fn rust_log_is_ignored() {
    let dir = TempDir::new();

    log_in_a_child("logs_when_asked", &dir.0, &[("RUST_LOG", "trace")]);

    let logged = logged(&dir.0);
    assert!(logged.contains(INFO_LINE), "{logged}");
    assert!(!logged.contains(DEBUG_LINE), "{logged}");
}

#[test]
fn an_invalid_ratatoist_log_is_reported_and_info_is_used() {
    let dir = TempDir::new();

    log_in_a_child(
        "logs_when_asked",
        &dir.0,
        &[("RATATOIST_LOG", "ratatoist=loud")],
    );

    let logged = logged(&dir.0);
    assert!(logged.contains("ignoring RATATOIST_LOG"), "{logged}");
    assert!(logged.contains(INFO_LINE), "{logged}");
    assert!(!logged.contains(DEBUG_LINE), "{logged}");
}

#[test]
fn the_debug_flag_is_rejected() {
    let home = TempDir::new();

    let output = Command::new(env!("CARGO_BIN_EXE_ratatoist"))
        .arg("--debug")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &home.0)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument '--debug'"), "{stderr}");
}

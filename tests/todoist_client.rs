//! The real client against a local fake server, the fixtures it parses, and
//! the fake client the app and `--demo` use.

// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use jiff::civil::date;
use ratatoist::auth::ApiToken;
use ratatoist::todoist::fake::CommandMatch;
use ratatoist::todoist::model::{CompletedTasksResponse, Paginated, UserInfo};
use ratatoist::todoist::sync::{SyncCommand, SyncCommandError, SyncRequest, SyncResponse};
use ratatoist::todoist::{
    ClientConfig, ClientError, CompletedQuery, CompletedWindow, FakeClient, HttpClient,
    LiveMessage, TodoistClient,
};
use serde_json::{Value, json};
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Not a Todoist token; nothing here reaches Todoist.
const TOKEN: &str = "fake-test-token-not-a-real-todoist-token";

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/todoist");

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Port 1 is privileged, so no test can listen there and a connection to it
/// is refused at once.
const UNREACHABLE: &str = "http://127.0.0.1:1";

fn tiny_config(base_url: String) -> ClientConfig {
    ClientConfig {
        base_url,
        connect_timeout: Duration::from_millis(300),
        request_timeout: Duration::from_secs(2),
        retry_unit: Duration::from_millis(10),
    }
}

fn config_for(server: &MockServer) -> ClientConfig {
    tiny_config(server.uri())
}

fn client_with(config: ClientConfig) -> HttpClient {
    HttpClient::with_config(&ApiToken::new(TOKEN).unwrap(), config).unwrap()
}

fn client_for(server: &MockServer) -> HttpClient {
    client_with(config_for(server))
}

fn command(kind: &str, uuid: &str) -> SyncCommand {
    SyncCommand {
        r#type: kind.to_owned(),
        temp_id: None,
        uuid: uuid.to_owned(),
        args: json!({ "id": "6Jf8VQXxpwv56VT1" }),
    }
}

fn write_request(commands: Vec<SyncCommand>) -> SyncRequest {
    SyncRequest {
        sync_token: "hypothetical-sync-token-0002".to_owned(),
        resource_types: vec![],
        commands,
    }
}

#[tokio::test]
async fn sends_the_token_as_a_bearer_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_user.json")))
        .expect(1)
        .mount(&server)
        .await;

    let user = client_for(&server).user().await.unwrap();

    assert_eq!(user.id, "1000001");
    assert_eq!(
        user.websocket_url.as_deref(),
        Some("wss://ws.example.com/ws?token=hypothetical")
    );
}

#[tokio::test]
async fn sync_posts_the_commands_as_json_and_reads_each_status() {
    let server = MockServer::start().await;
    let mut add = command("item_add", "e5f6a7b8-0000-4000-8000-000000000002");
    add.temp_id = Some("a1b2c3d4-0000-4000-8000-000000000001".to_owned());
    add.args = json!({ "content": "Buy compost", "project_id": "6Jf8VQXxpwv56VQ8" });
    let close = command("item_close", "6f3a9c4e-1b2d-4e5f-8a9b-0c1d2e3f4a5b");
    Mock::given(method("POST"))
        .and(path("/sync"))
        .and(body_json(json!({
            "sync_token": "hypothetical-sync-token-0002",
            "resource_types": [],
            "commands": [
                {
                    "type": "item_add",
                    "temp_id": "a1b2c3d4-0000-4000-8000-000000000001",
                    "uuid": "e5f6a7b8-0000-4000-8000-000000000002",
                    "args": { "content": "Buy compost", "project_id": "6Jf8VQXxpwv56VQ8" }
                },
                {
                    "type": "item_close",
                    "uuid": "6f3a9c4e-1b2d-4e5f-8a9b-0c1d2e3f4a5b",
                    "args": { "id": "6Jf8VQXxpwv56VT1" }
                }
            ]
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_command_results.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let response = client_for(&server)
        .sync(write_request(vec![add, close]))
        .await
        .unwrap();

    assert_eq!(response.sync_status.len(), 2);
    assert!(!response.sync_status["e5f6a7b8-0000-4000-8000-000000000002"].is_err());
    assert_eq!(
        response.sync_status["6f3a9c4e-1b2d-4e5f-8a9b-0c1d2e3f4a5b"].error_message(),
        Some("Item not found")
    );
    // Todoist maps the command's temp_id, not its uuid, to the new id.
    assert_eq!(
        response.temp_id_mapping["a1b2c3d4-0000-4000-8000-000000000001"],
        "6Jf8VQXxpwv56VT9"
    );
}

#[tokio::test]
async fn comments_follow_the_cursor_to_the_last_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/comments"))
        .and(query_param("task_id", "6Jf8VQXxpwv56VT2"))
        .and(query_param_is_missing("cursor"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_comments_page_1.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/comments"))
        .and(query_param("task_id", "6Jf8VQXxpwv56VT2"))
        .and(query_param("cursor", "hypothetical-cursor-2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_comments_page_2.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let comments = client_for(&server)
        .comments("6Jf8VQXxpwv56VT2")
        .await
        .unwrap();

    let ids: Vec<&str> = comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["6Jf8VQXxpwv56VN1", "6Jf8VQXxpwv56VN2"]);
}

#[tokio::test]
async fn paging_stops_after_fifty_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "id": "c1", "content": "again" }],
            "next_cursor": "always-more"
        })))
        .expect(50)
        .mount(&server)
        .await;

    let comments = client_for(&server).comments("t1").await.unwrap();

    assert_eq!(comments.len(), 50);
}

fn window() -> CompletedWindow {
    CompletedWindow::latest(date(2026, 11, 23))
}

fn query(project_id: Option<&str>) -> CompletedQuery {
    CompletedQuery {
        project_id: project_id.map(str::to_owned),
        window: window(),
    }
}

#[tokio::test]
async fn completed_tasks_load_one_window_by_completion_date() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tasks/completed/by_completion_date"))
        .and(query_param("since", "2026-09-02T00:00:00Z"))
        .and(query_param("until", "2026-11-25T00:00:00Z"))
        .and(query_param("project_id", "6Jf8VQXxpwv56VQ8"))
        .and(query_param_is_missing("annotate_items"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_completed_tasks.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let tasks = client_for(&server)
        .completed_tasks(query(Some("6Jf8VQXxpwv56VQ8")))
        .await
        .unwrap();

    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].priority, 4);
    assert_eq!(tasks[1].content, "Sharpen the shears");
    assert!(tasks.iter().all(|task| task.checked));
}

#[tokio::test]
async fn an_older_window_ends_where_the_newer_one_starts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tasks/completed/by_completion_date"))
        .and(query_param("since", "2026-06-10T00:00:00Z"))
        .and(query_param("until", "2026-09-02T00:00:00Z"))
        .and(query_param_is_missing("project_id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let older = CompletedQuery {
        project_id: None,
        window: window().before(),
    };
    let tasks = client_for(&server).completed_tasks(older).await.unwrap();

    assert!(tasks.is_empty());
}

#[tokio::test]
async fn completed_tasks_follow_the_cursor_with_the_same_window() {
    // Todoist's cursors are `[0-9a-zA-Z_-]`; these characters prove the
    // cursor is encoded, not pasted into the URL.
    let cursor = "a+b/c=d&e";
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tasks/completed/by_completion_date"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{ "id": "t1", "content": "first page" }],
            "next_cursor": cursor,
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/completed/by_completion_date"))
        .and(query_param("cursor", cursor))
        .and(query_param("since", "2026-09-02T00:00:00Z"))
        .and(query_param("until", "2026-11-25T00:00:00Z"))
        .and(query_param("project_id", "p 1&x=y"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{ "id": "t2", "content": "last page" }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let tasks = client_for(&server)
        .completed_tasks(query(Some("p 1&x=y")))
        .await
        .unwrap();

    let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["t1", "t2"]);
}

#[tokio::test]
async fn comments_encode_the_task_id_and_cursor() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/comments"))
        .and(query_param("task_id", "t+1/2=3"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "id": "c1" }],
            "next_cursor": "x+y/z=",
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/comments"))
        .and(query_param("task_id", "t+1/2=3"))
        .and(query_param("cursor", "x+y/z="))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "id": "c2" }],
            "next_cursor": null,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let comments = client_for(&server).comments("t+1/2=3").await.unwrap();

    assert_eq!(comments.len(), 2);
}

#[tokio::test]
async fn a_slow_response_times_out_as_a_network_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("HYPOTHETICAL_user.json"))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let client = client_with(ClientConfig {
        request_timeout: Duration::from_millis(200),
        ..config_for(&server)
    });

    let started = Instant::now();
    let error = client.user().await.unwrap_err();

    assert!(matches!(error, ClientError::Network(_)), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn an_unreachable_server_is_a_network_error() {
    let client = client_with(tiny_config(UNREACHABLE.to_owned()));

    let error = client.user().await.unwrap_err();

    assert!(matches!(error, ClientError::Network(_)), "{error:?}");
}

#[tokio::test]
async fn a_429_waits_for_retry_after_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "5"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sync"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_command_results.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let started = Instant::now();
    let response = client_for(&server).sync(write_request(vec![])).await;

    assert!(response.is_ok(), "{response:?}");
    // Five units of 10 ms, plus at most two units of jitter.
    let waited = started.elapsed();
    assert!(waited >= Duration::from_millis(50), "{waited:?}");
}

#[tokio::test]
async fn a_huge_retry_after_is_capped_at_sixty_units() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", u64::MAX.to_string()))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_user.json")))
        .expect(1)
        .mount(&server)
        .await;
    let client = client_for(&server);

    let started = Instant::now();
    let user = tokio::time::timeout(Duration::from_secs(3), client.user()).await;

    assert!(matches!(user, Ok(Ok(_))), "{user:?}");
    // Sixty units of 10 ms, jitter included.
    let waited = started.elapsed();
    assert!(waited >= Duration::from_millis(600), "{waited:?}");
}

#[tokio::test]
async fn a_429_that_never_clears_is_rate_limited_with_the_body_wait() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "error": "Too many requests",
            "error_code": 35,
            "error_extra": { "retry_after": 3 }
        })))
        .expect(4)
        .mount(&server)
        .await;

    let error = client_for(&server)
        .sync(write_request(vec![]))
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ClientError::RateLimited {
            retry_after: Some(Duration::from_secs(3))
        }
    );
}

/// Collects everything a `tracing` subscriber writes.
#[derive(Clone, Default)]
struct CapturedLogs(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLogs {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl CapturedLogs {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// The process-wide trace-level log of this test binary. A scoped subscriber
/// races tracing's global callsite cache against tests running in parallel,
/// so this one is global and also sees every other test's calls.
fn captured_logs() -> &'static CapturedLogs {
    static LOGS: std::sync::OnceLock<CapturedLogs> = std::sync::OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = CapturedLogs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
        logs
    })
}

#[tokio::test]
async fn no_client_call_logs_the_token_even_at_trace_level() {
    let logs = captured_logs();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_user.json")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/comments"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_comments_page_2.json")),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/completed/by_completion_date"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sync"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sync"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_command_results.json")),
        )
        .mount(&server)
        .await;
    let client = client_for(&server);

    client.user().await.unwrap();
    client.comments("6Jf8VQXxpwv56VT2").await.unwrap();
    let completed = client.completed_tasks(query(None)).await;
    let synced = client
        .sync(write_request(vec![command("item_close", "6f3a9c4e")]))
        .await;

    assert_eq!(completed.unwrap_err(), ClientError::Unauthorized);
    assert!(synced.is_ok(), "{synced:?}");
    let text = logs.text();
    assert!(text.contains("rate limited"), "nothing captured: {text}");
    assert!(text.contains("sync complete"), "nothing captured: {text}");
    assert!(!text.contains(TOKEN), "the token reached the log:\n{text}");
}

#[tokio::test]
async fn a_401_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;

    let error = client_for(&server)
        .sync(write_request(vec![]))
        .await
        .unwrap_err();

    assert_eq!(error, ClientError::Unauthorized);
    assert!(!error.to_string().contains(TOKEN));
}

#[tokio::test]
async fn a_401_whose_body_is_cut_off_is_still_unauthorized() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request).await;
        // Promises 100 bytes of body, sends 5, then hangs up.
        stream
            .write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 100\r\n\r\nshort")
            .await
            .unwrap();
    });
    let client = client_with(tiny_config(format!("http://{address}")));

    let error = client.user().await.unwrap_err();

    assert_eq!(error, ClientError::Unauthorized);
}

#[tokio::test]
async fn another_status_is_an_http_error_with_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable\n"))
        .mount(&server)
        .await;

    let error = client_for(&server).user().await.unwrap_err();

    assert_eq!(
        error,
        ClientError::Http {
            status: 503,
            message: "Service Unavailable".to_owned()
        }
    );
}

#[tokio::test]
async fn a_body_that_is_not_the_expected_json_is_a_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>maintenance</html>"))
        .mount(&server)
        .await;

    let error = client_for(&server).user().await.unwrap_err();

    assert!(matches!(error, ClientError::Decode(_)), "{error:?}");
}

#[test]
fn every_fixture_parses_into_the_model() {
    let mut names: Vec<String> = std::fs::read_dir(FIXTURES)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();

    for name in &names {
        let value = fixture(name);
        let parsed = match name.as_str() {
            "HYPOTHETICAL_full_sync.json"
            | "HYPOTHETICAL_incremental_sync.json"
            | "HYPOTHETICAL_command_results.json" => {
                serde_json::from_value::<SyncResponse>(value).map(drop)
            }
            "HYPOTHETICAL_comments_page_1.json" | "HYPOTHETICAL_comments_page_2.json" => {
                serde_json::from_value::<Paginated<ratatoist::todoist::model::Comment>>(value)
                    .map(drop)
            }
            "HYPOTHETICAL_completed_tasks.json" => {
                serde_json::from_value::<CompletedTasksResponse>(value).map(drop)
            }
            "HYPOTHETICAL_user.json" => serde_json::from_value::<UserInfo>(value).map(drop),
            "HYPOTHETICAL_rate_limited.json" | "HYPOTHETICAL_server_error.json" => {
                serde_json::from_value::<SyncCommandError>(value).map(drop)
            }
            other => panic!("{other} has no model type in this test"),
        };
        assert!(parsed.is_ok(), "{name}: {parsed:?}");
    }
    assert_eq!(names.len(), 9, "{names:?}");
    let limited = fixture("HYPOTHETICAL_rate_limited.json");
    assert_eq!(limited["error_extra"]["retry_after"].as_u64(), Some(30));

    let full: SyncResponse =
        serde_json::from_value(fixture("HYPOTHETICAL_full_sync.json")).unwrap();
    assert!(full.full_sync);
    assert_eq!(full.items.unwrap().len(), 3);
    assert_eq!(full.folders.unwrap()[0].workspace_id, "9001");
}

#[tokio::test]
async fn the_fake_serves_its_fixtures() {
    let fake = FakeClient::new()
        .with_full_sync(fixture("HYPOTHETICAL_full_sync.json"))
        .with_incremental_sync(fixture("HYPOTHETICAL_incremental_sync.json"))
        .with_user(fixture("HYPOTHETICAL_user.json"))
        .with_comments(fixture("HYPOTHETICAL_comments_page_1.json"))
        .with_completed_tasks(fixture("HYPOTHETICAL_completed_tasks.json"));

    let full = fake
        .sync(SyncRequest {
            sync_token: "*".to_owned(),
            resource_types: vec!["all".to_owned()],
            commands: vec![],
        })
        .await
        .unwrap();
    let delta = fake.sync(write_request(vec![])).await.unwrap();

    assert!(full.full_sync);
    assert_eq!(full.projects.unwrap().len(), 3);
    assert!(!delta.full_sync);
    assert_eq!(fake.user().await.unwrap().id, "1000001");
    assert_eq!(fake.comments("6Jf8VQXxpwv56VT2").await.unwrap().len(), 1);
    let completed = fake.completed_tasks(query(None)).await.unwrap();
    assert_eq!(completed.len(), 2);
}

#[tokio::test]
async fn the_fake_fails_a_chosen_command_and_records_every_command() {
    let fake = FakeClient::new();
    fake.fail_command(
        CommandMatch::Uuid("u2".to_owned()),
        SyncCommandError {
            error_code: 15,
            error: "Invalid temporary id".to_owned(),
        },
    );
    fake.fail_command(
        CommandMatch::Type("project_delete".to_owned()),
        SyncCommandError {
            error_code: 403,
            error: "Access denied".to_owned(),
        },
    );
    let app_side = fake.clone();

    let response = app_side
        .sync(write_request(vec![
            command("item_close", "u1"),
            command("item_update", "u2"),
            command("project_delete", "u3"),
        ]))
        .await
        .unwrap();

    assert!(!response.sync_status["u1"].is_err());
    assert_eq!(
        response.sync_status["u2"].error_message(),
        Some("Invalid temporary id")
    );
    assert_eq!(
        response.sync_status["u3"].error_message(),
        Some("Access denied")
    );
    let uuids: Vec<String> = fake.commands().into_iter().map(|c| c.uuid).collect();
    assert_eq!(uuids, ["u1", "u2", "u3"]);
}

#[tokio::test]
async fn the_fake_records_commands_even_when_its_sync_body_is_malformed() {
    let fake = FakeClient::new().with_incremental_sync(json!({ "full_sync": "not a bool" }));

    let response = fake
        .sync(write_request(vec![command("item_close", "u1")]))
        .await;

    assert!(
        matches!(response, Err(ClientError::Decode(_))),
        "{response:?}"
    );
    let uuids: Vec<String> = fake.commands().into_iter().map(|c| c.uuid).collect();
    assert_eq!(uuids, ["u1"]);
}

#[tokio::test]
async fn the_fake_with_nothing_configured_is_an_empty_account() {
    let fake = FakeClient::default();

    let response = fake
        .sync(write_request(vec![command("item_close", "u1")]))
        .await;

    assert!(!response.unwrap().sync_status["u1"].is_err());
    assert!(fake.comments("t1").await.unwrap().is_empty());
}

// The handshake callback returns tungstenite's error type, not ours.
#[allow(clippy::result_large_err)]
#[tokio::test]
async fn live_events_report_the_connection_and_each_message() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut origin = None;
        let mut socket = tokio_tungstenite::accept_hdr_async(
            stream,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                origin = request
                    .headers()
                    .get("origin")
                    .map(|v| v.to_str().unwrap().to_owned());
                Ok(response)
            },
        )
        .await
        .unwrap();
        socket
            .send(tokio_tungstenite::tungstenite::Message::text(
                r#"{"type":"sync_needed"}"#,
            ))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
        origin
    });
    let client = client_with(tiny_config(UNREACHABLE.to_owned()));

    let messages: Vec<LiveMessage> = tokio::time::timeout(
        Duration::from_secs(5),
        client
            .live_events(&format!("ws://{address}/ws"))
            .take(4)
            .collect(),
    )
    .await
    .unwrap();

    // The close frame is a message too, so it also reads as a change.
    assert_eq!(
        messages,
        [
            LiveMessage::Connected,
            LiveMessage::Changed,
            LiveMessage::Changed,
            LiveMessage::Disconnected
        ]
    );
    assert_eq!(
        server.await.unwrap().as_deref(),
        Some("https://app.todoist.com")
    );
}

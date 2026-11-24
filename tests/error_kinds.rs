//! Each kind of failure, mapped from what Todoist answers, through the real
//! client against a local server.

#![cfg(test)]

use std::path::Path;
use std::time::Duration;

use jiff::civil::date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Zoned};
use ratatoist::app::{ErrorKind, Failure};
use ratatoist::auth::ApiToken;
use ratatoist::todoist::sync::{SyncCommandResult, SyncRequest};
use ratatoist::todoist::{ClientConfig, ClientError, HttpClient, TodoistClient};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Not a Todoist token; nothing here reaches Todoist.
const TOKEN: &str = "fake-test-token-not-a-real-todoist-token";

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/todoist");

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn now() -> Zoned {
    date(2026, 11, 23)
        .at(9, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
}

fn client(base_url: String, request_timeout: Duration) -> HttpClient {
    let config = ClientConfig {
        base_url,
        connect_timeout: Duration::from_millis(300),
        request_timeout,
        retry_unit: Duration::from_millis(10),
    };
    HttpClient::with_config(&ApiToken::new(TOKEN).unwrap(), config).unwrap()
}

fn read() -> SyncRequest {
    SyncRequest {
        sync_token: "*".to_owned(),
        resource_types: vec!["items".to_owned()],
        commands: vec![],
    }
}

/// The error a sync gets from a server that answers with `response`.
async fn sync_error(response: ResponseTemplate) -> ClientError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sync"))
        .respond_with(response)
        .mount(&server)
        .await;
    client(server.uri(), Duration::from_secs(2))
        .sync(read())
        .await
        .unwrap_err()
}

#[tokio::test]
async fn a_401_is_a_rejected_token() {
    let error = sync_error(ResponseTemplate::new(401)).await;

    assert_eq!(ErrorKind::of(&error, &now()), ErrorKind::TokenRejected);
}

#[tokio::test]
async fn a_429_is_rate_limited_until_its_retry_after() {
    let body = fixture("HYPOTHETICAL_rate_limited.json");
    let error = sync_error(ResponseTemplate::new(429).set_body_json(body)).await;

    let kind = ErrorKind::of(&error, &now());

    assert_eq!(
        kind,
        ErrorKind::RateLimited {
            until: Some(now() + SignedDuration::from_secs(30))
        }
    );
    assert_eq!(
        kind.summary(),
        "Todoist's rate limit is used up until 09:00"
    );
}

#[tokio::test]
async fn a_500_is_a_server_error() {
    let body = fixture("HYPOTHETICAL_server_error.json");
    let error = sync_error(ResponseTemplate::new(500).set_body_json(body)).await;

    assert_eq!(ErrorKind::of(&error, &now()), ErrorKind::Server);
}

#[tokio::test]
async fn a_request_that_times_out_is_offline() {
    let slow = ResponseTemplate::new(200)
        .set_body_json(fixture("HYPOTHETICAL_incremental_sync.json"))
        .set_delay(Duration::from_secs(5));
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(slow)
        .mount(&server)
        .await;

    let error = client(server.uri(), Duration::from_millis(200))
        .sync(read())
        .await
        .unwrap_err();

    assert_eq!(ErrorKind::of(&error, &now()), ErrorKind::Offline);
}

#[tokio::test]
async fn a_refused_connection_is_offline() {
    let error = client("http://127.0.0.1:1".to_owned(), Duration::from_secs(2))
        .sync(read())
        .await
        .unwrap_err();

    assert_eq!(ErrorKind::of(&error, &now()), ErrorKind::Offline);
}

#[tokio::test]
async fn a_sync_status_error_is_a_validation_failure_with_todoist_s_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("HYPOTHETICAL_command_results.json")),
        )
        .mount(&server)
        .await;
    let response = client(server.uri(), Duration::from_secs(2))
        .sync(read())
        .await
        .unwrap();

    let failures: Vec<Failure> = response
        .sync_status
        .values()
        .filter_map(|status| match status {
            SyncCommandResult::Err(error) => Some(Failure::from_command(
                "complete \"Buy oat milk\"",
                error,
                &now(),
            )),
            SyncCommandResult::Ok(_) => None,
        })
        .collect();

    let [failure] = failures.as_slice() else {
        panic!("one command fails in the fixture");
    };
    assert_eq!(failure.kind, ErrorKind::Validation);
    assert_eq!(failure.message, "Item not found");
    assert_eq!(failure.action, "complete \"Buy oat milk\"");
    assert_eq!(failure.at, now());
}

#[test]
fn every_kind_has_a_name_a_summary_and_a_fix() {
    for kind in [
        ErrorKind::Offline,
        ErrorKind::RateLimited { until: None },
        ErrorKind::TokenRejected,
        ErrorKind::Validation,
        ErrorKind::Server,
        ErrorKind::Other,
    ] {
        assert!(!kind.name().is_empty());
        assert!(!kind.summary().is_empty());
        assert!(kind.fix().ends_with('.'));
    }
}

#[test]
fn other_client_errors_map_by_what_they_say() {
    let at = &now();
    let http = |status| ClientError::Http {
        status,
        message: String::new(),
    };

    assert_eq!(ErrorKind::of(&http(403), at), ErrorKind::TokenRejected);
    assert_eq!(ErrorKind::of(&http(400), at), ErrorKind::Validation);
    assert_eq!(ErrorKind::of(&http(503), at), ErrorKind::Server);
    assert_eq!(
        ErrorKind::of(&ClientError::Decode("eof".to_owned()), at),
        ErrorKind::Other
    );
    assert_eq!(
        ErrorKind::of(&ClientError::RateLimited { retry_after: None }, at),
        ErrorKind::RateLimited { until: None }
    );
}

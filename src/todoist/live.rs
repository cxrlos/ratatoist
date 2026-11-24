//! Live updates: Todoist's undocumented WebSocket at the user's
//! `websocket_url`. Every text or binary message means something changed;
//! pings and pongs keep the connection open and say nothing. ratatoist
//! pings the server every 30 s, and a socket that sends no frame at all for
//! 75 s counts as disconnected, so a connection that died without a close
//! (a suspend, a network change) is noticed. The stream reconnects after
//! each disconnect and never ends on its own.

use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt, stream};
use tokio::net::TcpStream;
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async_tls_with_config};
use tracing::debug;

use super::LiveMessage;

/// The web app's origin, which the socket has always been opened with;
/// whether Todoist needs it is unverified.
const ORIGIN: &str = "https://app.todoist.com";
/// The wait after a failed connection, before jitter; it doubles up to the
/// cap.
const FIRST_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A clean disconnect reconnects after this, without growing the backoff.
const AFTER_DISCONNECT: Duration = Duration::from_secs(1);
/// How often ratatoist pings the server.
const PING_EVERY: Duration = Duration::from_secs(30);
/// A socket that sends no frame for this long, pongs included, is dead.
const DEAD_AFTER: Duration = Duration::from_secs(75);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Live {
    url: String,
    socket: Option<Socket>,
    wait: Option<Duration>,
    backoff: Duration,
    last_frame: Instant,
    next_ping: Instant,
}

pub(super) fn events(url: String) -> impl Stream<Item = LiveMessage> + Send + 'static {
    let live = Live {
        url,
        socket: None,
        wait: None,
        backoff: FIRST_BACKOFF,
        last_frame: Instant::now(),
        next_ping: Instant::now(),
    };
    stream::unfold(live, |mut live| async move {
        loop {
            if let Some(socket) = live.socket.as_mut() {
                let dead_at = live.last_frame + DEAD_AFTER;
                let read = tokio::select! {
                    read = socket.next() => Some(read),
                    () = sleep_until(dead_at.min(live.next_ping)) => None,
                };
                match read {
                    Some(Some(Ok(message))) => {
                        live.last_frame = Instant::now();
                        match signal(&message) {
                            Signal::Changed => return Some((LiveMessage::Changed, live)),
                            Signal::KeepAlive => continue,
                            Signal::Closed => {}
                        }
                    }
                    Some(Some(Err(error))) => debug!(error = %error, "websocket read failed"),
                    Some(None) => {}
                    None if Instant::now() >= dead_at => {
                        debug!("websocket silent for {DEAD_AFTER:?}, reconnecting");
                    }
                    None => {
                        live.next_ping = Instant::now() + PING_EVERY;
                        let ping = socket.send(Message::Ping(Default::default()));
                        match tokio::time::timeout_at(dead_at, ping).await {
                            Ok(Ok(())) => continue,
                            Ok(Err(error)) => debug!(error = %error, "websocket ping failed"),
                            Err(_) => debug!("websocket ping stuck, reconnecting"),
                        }
                    }
                }
                live.socket = None;
                live.wait = Some(AFTER_DISCONNECT);
                return Some((LiveMessage::Disconnected, live));
            }
            if let Some(wait) = live.wait.take() {
                tokio::time::sleep(wait).await;
            }
            match connect(&live.url).await {
                Ok(socket) => {
                    live.socket = Some(socket);
                    live.last_frame = Instant::now();
                    live.next_ping = live.last_frame + PING_EVERY;
                    live.backoff = FIRST_BACKOFF;
                    return Some((LiveMessage::Connected, live));
                }
                Err(error) => {
                    debug!(error = %error, "websocket connection failed, retrying");
                    live.wait = Some(jittered(live.backoff));
                    live.backoff = (live.backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    })
}

/// What a message from the socket means to ratatoist.
#[derive(Debug, PartialEq, Eq)]
enum Signal {
    Changed,
    KeepAlive,
    Closed,
}

/// Todoist's message bodies are undocumented, so any text or binary message
/// counts as a change; only the frames the protocol itself sends to keep the
/// connection open count as nothing.
fn signal(message: &Message) -> Signal {
    match message {
        Message::Text(_) | Message::Binary(_) => Signal::Changed,
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => Signal::KeepAlive,
        Message::Close(_) => Signal::Closed,
    }
}

/// `backoff` shortened by up to half at random, so clients that lost the
/// connection together do not all come back at the same moment.
fn jittered(backoff: Duration) -> Duration {
    backoff.mul_f64(rand::random_range(0.5..=1.0))
}

async fn connect(url: &str) -> anyhow::Result<Socket> {
    let mut request = url.into_client_request()?;
    request.headers_mut().insert("Origin", ORIGIN.parse()?);
    let (socket, _) = connect_async_tls_with_config(request, None, false, None).await?;
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use tokio_tungstenite::tungstenite::Bytes;

    use super::*;

    #[test]
    fn keep_alives_signal_nothing_and_any_other_message_a_change() {
        assert_eq!(signal(&Message::Ping(Bytes::new())), Signal::KeepAlive);
        assert_eq!(signal(&Message::Pong(Bytes::new())), Signal::KeepAlive);
        assert_eq!(
            signal(&Message::text(r#"{"type":"sync_needed"}"#)),
            Signal::Changed
        );
        assert_eq!(signal(&Message::text("anything")), Signal::Changed);
        assert_eq!(signal(&Message::binary(vec![1])), Signal::Changed);
        assert_eq!(signal(&Message::Close(None)), Signal::Closed);
    }

    #[test]
    fn the_backoff_is_jittered_within_half_of_itself() {
        let waits: Vec<_> = (0..64).map(|_| jittered(MAX_BACKOFF)).collect();

        assert!(
            waits
                .iter()
                .all(|wait| *wait >= MAX_BACKOFF / 2 && *wait <= MAX_BACKOFF)
        );
        assert!(waits.iter().any(|wait| *wait != waits[0]), "{waits:?}");
    }
}

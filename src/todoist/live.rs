//! Live updates: Todoist's undocumented WebSocket at the user's
//! `websocket_url`. Every message means something changed; the stream
//! reconnects after each disconnect and never ends on its own.

use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async_tls_with_config};
use tracing::debug;

use super::LiveMessage;

/// Todoist refuses the upgrade without a web-app origin.
const ORIGIN: &str = "https://app.todoist.com";
/// The wait after a failed connection; it doubles up to the cap.
const FIRST_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A clean disconnect reconnects after this, without growing the backoff.
const AFTER_DISCONNECT: Duration = Duration::from_secs(1);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Live {
    url: String,
    socket: Option<Socket>,
    wait: Option<Duration>,
    backoff: Duration,
}

pub(super) fn events(url: String) -> impl Stream<Item = LiveMessage> + Send + 'static {
    let live = Live {
        url,
        socket: None,
        wait: None,
        backoff: FIRST_BACKOFF,
    };
    stream::unfold(live, |mut live| async move {
        loop {
            if let Some(socket) = live.socket.as_mut() {
                if socket.next().await.is_some() {
                    return Some((LiveMessage::Changed, live));
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
                    live.backoff = FIRST_BACKOFF;
                    return Some((LiveMessage::Connected, live));
                }
                Err(error) => {
                    debug!(error = %error, "websocket connection failed, retrying");
                    live.wait = Some(live.backoff);
                    live.backoff = (live.backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    })
}

async fn connect(url: &str) -> anyhow::Result<Socket> {
    let mut request = url.into_client_request()?;
    request.headers_mut().insert("Origin", ORIGIN.parse()?);
    let (socket, _) = connect_async_tls_with_config(request, None, false, None).await?;
    Ok(socket)
}

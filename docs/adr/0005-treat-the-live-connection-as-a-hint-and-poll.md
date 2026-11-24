# Treat the live connection as a hint and poll while it is down

The live-updates WebSocket tells ratatoist that something changed on Todoist's side, and nothing more: each message that is not a keep-alive schedules one incremental sync with the sync token, two seconds after the first message of a burst, so a stream of changes costs one sync every two seconds at most. Correctness comes from the sync token, not the socket: while the connection is down or has not come up, ratatoist polls with an incremental sync every 60 seconds, so a dropped or silent connection delays changes rather than losing them. The socket is undocumented in Todoist's API v1 (only `websocket_url` on the user appears), so it can change or vanish without notice, and polling keeps ratatoist correct if it does.

## Considered options

- **The WebSocket as the source of changes**: rejected. Its messages carry no data, only that something changed, and a message missed while the connection is down is gone; only the sync token tells Todoist what I have.
- **A sync per message**: rejected. Every frame used to start a sync, so a burst of changes, or pings, stormed the API's rate limit.
- **Polling only**: rejected. A minute's delay on every change made elsewhere is worse than the socket's seconds when it works.

## Consequences

Both the debounced syncs and polling pause while the session is idle (`[sync] idle-timeout`); the socket stays open, and the first key after idle syncs once. A reconnect after a drop syncs once to catch up, since the socket never announces what changed while it was down. ratatoist pings the socket every 30 seconds and counts 75 seconds without any frame as a drop, so a connection that died silently (a laptop's sleep, a network change) falls back to polling instead of showing `live` forever. A connection that closes soon after opening backs off like a failed one, growing from 5 s to 60 s with jitter, instead of reconnecting every second. The sync indicator says `live`, `polling` or `idle` with the time since the last sync, or `offline` or `sync failing` as an alert. The connection still sends the web app's `Origin` header, which is kept because whether Todoist needs it is unverified.

# Security

ratatoist holds a Todoist API token that can read and change everything in your Todoist account, so I treat anything that could leak or misuse it as a security issue.

## Reporting

Report vulnerabilities privately through [GitHub security advisories](https://github.com/cxrlos/ratatoist/security/advisories/new), not as a public issue. I aim to reply within a week.

## Supported versions

Only the latest released version gets fixes.

## How ratatoist handles your token

- It reads the token from the `TODOIST_API_TOKEN` environment variable, or else from the OS keyring: the Keychain on macOS, the Secret Service (GNOME Keyring, KWallet) on Linux, under service `ratatoist`. There is no plaintext fallback: when the keyring is unavailable, ratatoist says so and points at `TODOIST_API_TOKEN`.
- `ratatoist auth login` asks for the token without echoing it, or reads it from standard input with `--stdin`. It checks the token with Todoist before storing it and stores nothing Todoist rejects.
- `ratatoist auth status` shows where the token comes from and whose account it is, never the token. `ratatoist auth logout` deletes the keyring entry.
- Versions before 0.6.0 kept the token as `api_token` in `config.toml`. On the first start after upgrading, ratatoist moves it into the keyring and removes that one line from the file, leaving every other byte as it was. Nothing reads `api_token` as a token after that.
- In memory the token sits in a type that prints as `<redacted>`, so it never reaches the log, an error message or a panic. It never writes the token to the log.
- It sends the token only to `api.todoist.com`.

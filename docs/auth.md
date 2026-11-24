# Logging in

ratatoist needs your Todoist API token, which you copy from Settings, Integrations, Developer at <https://app.todoist.com/app/settings/integrations/developer>. The token reaches everything in your account, so ratatoist keeps it in the OS keyring and never in a file.

## Where the token comes from

ratatoist looks in two places, in this order, and uses the first it finds:

1. `TODOIST_API_TOKEN`, when it is set and not blank. This is for containers, CI and machines without a keyring.
2. The OS keyring entry with service `ratatoist` and account `api-token`.

With neither, ratatoist prints this and exits with status 1:

```text
No Todoist token: run "ratatoist auth login", or set TODOIST_API_TOKEN.
```

There is no plaintext fallback. When the keyring cannot be reached, ratatoist says why and suggests `TODOIST_API_TOKEN`.

## Commands

| Command | What it does |
| --- | --- |
| `ratatoist auth login` | Asks for the token on the terminal without echoing it, checks it with Todoist, then stores it in the keyring. |
| `ratatoist auth login --stdin` | The same, reading the token from the first line of standard input, such as `pass show todoist \| ratatoist auth login --stdin`. Surrounding whitespace is trimmed. |
| `ratatoist auth status` | Prints where the token in use comes from and the name and email of its account. It never prints the token. |
| `ratatoist auth logout` | Deletes the keyring entry. `TODOIST_API_TOKEN` is left alone, and the command says so when it is set. |

A token Todoist rejects is never stored. When `TODOIST_API_TOKEN` is set, `login` still stores the token and says that the variable wins over it.

## Moving an old token out of config.toml

Versions before 0.6.0 kept the token as `api_token` in `config.toml`. When ratatoist starts without `TODOIST_API_TOKEN` and finds that key:

- If the keyring has no entry, it stores the token there, removes the `api_token` line from `config.toml` and prints one line saying so.
- If the keyring already holds a token, that token wins, and the `api_token` line is removed the same way.
- If the keyring is unavailable, the file stays as it is and ratatoist prints the error with the `TODOIST_API_TOKEN` hint.
- If `config.toml` does not parse, or `api_token` does not hold a valid token, the file stays as it is. With no token in the keyring either, ratatoist says the line could not be moved and why, instead of only saying there is no token.

Only the `api_token` line goes; every other byte of the file, comments included, stays, and the file keeps its mode. When `config.toml` is a symlink (into a dotfiles repo, say), ratatoist edits the file it points to and leaves the link in place, so the token leaves the copy you might commit. If that file was ever committed, the token is still in the repo's history: I recommend issuing a new token in Todoist and logging in again. If the file cannot be rewritten (a read-only folder, say), the token is still in the keyring and ratatoist asks you to delete the line by hand. Besides picking a theme, which sets `[theme] name` ([config.md](config.md)), this is the only time ratatoist writes `config.toml`, and nothing reads `api_token` as a token afterwards.

## The keyring on each system

- **macOS**: the login Keychain, as a generic password. macOS may ask whether ratatoist may use it; "Always Allow" stops the question.
- **Linux**: the Secret Service over D-Bus, which GNOME Keyring and KWallet provide. A headless box or a container usually has none running; set `TODOIST_API_TOKEN` there instead.

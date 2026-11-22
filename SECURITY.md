# Security

ratatoist holds a Todoist API token that can read and change everything in your Todoist account, so I treat anything that could leak or misuse it as a security issue.

## Reporting

Report vulnerabilities privately through [GitHub security advisories](https://github.com/cxrlos/ratatoist/security/advisories/new), not as a public issue. I aim to reply within a week.

## Supported versions

Only the latest released version gets fixes.

## How ratatoist handles your token

- It reads the token from the `TODOIST_API_TOKEN` environment variable, or else from `api_token` in `config.toml` in its config directory (`$XDG_CONFIG_HOME/ratatoist/`, usually `~/.config/ratatoist/`).
- When it finds neither, a setup screen asks for the token, checks it with Todoist and saves it to that `config.toml` with mode 0600. `ratatoist --new-user` runs the same screen and keeps the token for that session only.
- The setup screen shows the token as you paste it, so clear the screen before you share or record it.
- On start it warns in the log when other users can read or write `config.toml`. It never writes the token to the log.
- It sends the token only to `api.todoist.com`.

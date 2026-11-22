# Security

ratatoist holds a Todoist API token that can read and change everything in your Todoist account, so I treat anything that could leak or misuse it as a security issue.

## Reporting

Report vulnerabilities privately through [GitHub security advisories](https://github.com/cxrlos/ratatoist/security/advisories/new), not as a public issue. I aim to reply within a week.

## Supported versions

Only the latest released version gets fixes.

## How ratatoist handles your token

- It reads the token from the `TODOIST_API_TOKEN` environment variable, or from your OS keyring (stored there by `ratatoist auth login`).
- It never writes the token to the config file or to logs, and never echoes it on screen.
- It sends the token only to `api.todoist.com`.

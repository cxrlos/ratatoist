# Contributing

Thanks for helping. ratatoist is a terminal client for Todoist; this page covers what you need to get a change merged.

## Scope

ratatoist supports Linux and macOS, and one Todoist account at a time. Issues labelled `good first issue` are a good place to start.

## Setup

- Rust 1.88 or newer
- [`cargo-insta`](https://insta.rs) for snapshot tests
- [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for dependency checks
- [`gitleaks`](https://github.com/gitleaks/gitleaks) for the secret scan

`cargo run -- --demo` starts ratatoist on a fictional account, so you don't need a Todoist account to work on the UI. When your change touches how ratatoist talks to Todoist, also try it against a second account, as described in [Testing against a free Todoist account](#testing-against-a-free-todoist-account).

## Testing against a free Todoist account

The test suite never calls the Todoist API, so a change to the Todoist client also needs a run against the real thing. I do that in a second, free Todoist account made for it, never in the account I plan my life in: ratatoist can complete, edit and move tasks, and a token reaches everything in its account.

1. Create the account. Sign up at [todoist.com](https://todoist.com) with a second email address; the free plan is enough.
2. Copy its API token from Settings, Integrations, Developer ([app.todoist.com/app/settings/integrations/developer](https://app.todoist.com/app/settings/integrations/developer)).
3. Hand the token to ratatoist in one of two ways:
   - For one shell session only, keep it in `TODOIST_API_TOKEN`. `read -rs` keeps it out of your shell history and off the screen; paste the token and press Enter:

     ```sh
     read -rs TODOIST_API_TOKEN && export TODOIST_API_TOKEN
     ```

     Closing the shell forgets it. While it is set it wins over any token in the keyring.
   - Or store it in the OS keyring with `ratatoist auth login`, which prompts for it without echo. `ratatoist auth logout` removes it again.
4. Load a handful of sample tasks in the web app: two or three projects, one with sections, a few labels, a subtask, a recurring task, a task due today and one overdue.

I run ratatoist from my working tree with `cargo run --`, so `ratatoist auth status` below is `cargo run -- auth status`, and `ratatoist` is `cargo run`.

### Manual checks

Run these in order and compare with what each step expects:

1. Log in. Unset `TODOIST_API_TOKEN` first (`unset TODOIST_API_TOKEN`), run `ratatoist auth login` and paste the token. It reports the account it stored the token for.
2. Check the token. `ratatoist auth status` names where the token comes from (the keyring) and the account, without printing the token.
3. Load your tasks. `ratatoist` shows the sample projects and their tasks.
4. Make a write round trip. Complete a task. It shows as completed in ratatoist and in the web app. Uncomplete it; both show it open again.
5. Watch live updates. In the web app, rename a task. ratatoist shows the change within a few seconds, without a key press.
6. Log out. `ratatoist auth logout` says it removed the token from the OS keyring, and `ratatoist auth status` then reports no token.

When you are done, reset the token in the same settings page, or delete the account.

Tokens never go in issues, PRs, logs, screenshots or recordings. Check a terminal screenshot before you post it, and if a token leaks anyway, reset it at once.

## Workflow

1. Open an issue, or comment on an existing one, before starting larger work. Questions and loose ideas go to [Discussions](https://github.com/cxrlos/ratatoist/discussions).
2. Branch off `main` as `<type>/<issue>-<slug>`, where `<issue>` is the issue number and `<slug>` a few words of its title.
3. Before opening a PR, run the checks listed under Commands in [AGENTS.md](AGENTS.md). CI runs the same ones.
4. When a snapshot changes, review it with `cargo insta review` and commit the accepted `.snap` files.

## Pull requests

- The PR title becomes the commit on `main`, so it must be a [Conventional Commit](https://www.conventionalcommits.org): `feat: ...`, `fix: ...`, `feat!: ...` for breaking changes. The changelog and version bump are generated from it.
- PRs are squash-merged.
- Fill in the template, including the exact commands you ran.

## Words and decisions

[GLOSSARY.md](GLOSSARY.md) defines the terms ratatoist uses; please use them in code and issues. Recorded decisions live in [docs/adr](docs/adr).

## AI-assisted contributions

Welcome. Agents read [AGENTS.md](AGENTS.md). You are responsible for what you submit, and you should be able to explain every change.

## Security

Never paste an API token into an issue, PR or log. Report vulnerabilities as described in [SECURITY.md](SECURITY.md).

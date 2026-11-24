# Write only the picked theme to config.toml

ratatoist writes exactly one key of `config.toml`, `[theme] name`, when I pick a theme in the palette's theme picker. Until now `config.toml` was mine alone: ratatoist read it and never wrote it, apart from the one-time removal of an old `api_token` line. But a theme picked in the app lasted only until quit, and keeping it meant opening the file and typing the line the notice named. I want a pick to persist without a settings screen, and the theme is the one setting I choose by looking at it rather than by knowing its value. The write is narrow so the file stays mine: it goes through `toml_edit`, which changes the one value and keeps every comment, the order and the layout, and creates the file or the `[theme]` table with just that key when they are missing. It is atomic (a temp file with the file's mode, renamed over it), so a crash never leaves half a file. A symlinked `config.toml` is followed and its target changed, as the token migration does, so a dotfiles checkout keeps its link. A file that does not parse is not written at all, since `toml_edit` cannot promise to keep what it cannot read: the pick lasts the session and a notice says why. `--demo` has no config folder and writes nothing, and under `NO_COLOR` the only theme is the colourless one, so a pick there writes nothing either.

## Considered options

- **Keep the session-only pick and the notice naming the line**: rejected. It asks me to do by hand what the app knows exactly, every time I change my mind.
- **A separate state file for UI choices**: rejected. That is what `ui_settings.json` was before 0.7.0, and two files setting the same value needed precedence rules and a notice to explain them. One file is the one place the theme lives.
- **Serializing `Config` back to `config.toml`**: rejected. It would drop my comments and reorder keys, and every setting would become something ratatoist may rewrite.
- **A settings screen that writes any key**: out of scope. Each new writable key widens what ratatoist may change in a file I edit by hand; it needs its own decision.

## Consequences

`config.toml` now has two writers, me and ratatoist, for one key. If I edit the file while ratatoist runs and then pick a theme, the write is based on what is on disk at that moment, so my edit survives; an editor holding an unsaved buffer can still overwrite the pick. A key other than `[theme] name` is never written, and adding one needs a new ADR. The write runs on the runtime's one writer, off the loop and in order, and a quit waits for it; a failed write goes to the messages log and the session keeps the theme. ratatoist still never rewrites the file to fix a problem it reports.

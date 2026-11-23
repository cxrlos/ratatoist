# Configuration

ratatoist reads `config.toml` from `$XDG_CONFIG_HOME/ratatoist/`, which is `~/.config/ratatoist/config.toml` by default, on macOS too. The file is optional: without it every setting has its default. ratatoist never writes it. The one exception is the one-time move of an old `api_token` line into the OS keyring, described in [auth.md](auth.md).

```toml
[ui]
# no settings yet

[keys]
# no settings yet

[theme]
name = "Nord"

[sync]
idle-timeout = 300
```

## `[theme]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `name` | The name of a built-in theme, or the `name` inside a file in `themes/`. Case, spaces and hyphens do not matter, so `"rose-pine"` finds "Rose Pine". | Rose Pine | The colours ratatoist draws with. |

The built-in themes are Catppuccin Mocha, Dracula, Gruvbox Dark, Material Dark, Monokai, Nord, One Dark, Rose Pine, Solarized Dark and Tokyo Night.

## `[sync]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `idle-timeout` | Whole seconds, 0 to 86400. 0 turns idle detection off. | 300 | How long without a key press before the session counts as idle. |

## `config.toml` and the Settings pane

The Settings pane saves the theme and idle timeout you pick there to `ui_settings.json` in the same folder. At startup a value set in `config.toml` wins over the saved one; `ui_settings.json` only fills in what `config.toml` leaves out, or a `[theme] name` that matches no theme. So if I set `[theme] name` in `config.toml`, a theme picked in the Settings pane lasts only until the next start.

## `[ui]` and `[keys]`

Both tables are accepted and take no settings yet. A key inside them is reported as unknown.

## Theme files

Every `*.json` file in `themes/` beside `config.toml` is read as a Base16 scheme: a `name` and the colours `base00` to `base0F` as six-digit hex, with or without `#`. Each one becomes a theme you can pick by its `name`. A file that cannot be read or parsed is reported and skipped; the others still load.

## When something is wrong

Problems print to stderr before the TUI starts, one line each, naming the file, line and column:

```text
config.toml:5:16: `sync.idle-timeout` must be a whole number of seconds from 0 to 86400; [sync] uses its defaults
```

- A syntax error resets only the section it is in; the other sections still apply. A syntax error before the first `[section]` header resets the whole file to its defaults.
- A bad value resets its whole section to its defaults.
- An unknown key or table is reported and ignored.
- A `[theme] name` that matches no theme is reported, and the theme saved by the Settings pane, or else the default, is used.
- A theme file that fails to load is reported as `themes/FILE.json:LINE:COL: problem; the theme is skipped`.

ratatoist still starts in every case, and the file is never rewritten.

`api_token` at the top of the file is the token setting of versions before 0.6.0. It is not reported as unknown, and the config loader never reads its value.

## The old macOS folder

Before 0.6.0, ratatoist kept its files on macOS in `~/Library/Application Support/ratatoist/`. If `~/.config/ratatoist/` does not exist and that folder does, ratatoist reads the old folder and prints the command that moves it:

```bash
mkdir -p ~/.config && mv ~/Library/Application\ Support/ratatoist ~/.config/ratatoist
```

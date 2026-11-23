# Configuration

ratatoist reads `config.toml` from `$XDG_CONFIG_HOME/ratatoist/`, which is `~/.config/ratatoist/config.toml` by default, on macOS too. The file is optional: without it every setting has its default. ratatoist never writes it. The one exception is the one-time move of an old `api_token` line into the OS keyring, described in [auth.md](auth.md).

```toml
[ui]
# no settings yet

[keys]
preset = "vim"

[keys.list]
move-down = { prepend = ["n"] }

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

## `[keys]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `preset` | `"vim"` or `"standard"` | `"vim"` | The keys everything starts from. `standard` moves with the arrows, Home/End and PgUp/PgDn, switches panes with Tab and Shift-Tab, adds a task with Ctrl-a and completes one with Ctrl-x; every other action keeps its vim key. |

Each context has its own table that changes the preset's keys there: `[keys.global]`, `[keys.list]`, `[keys.detail]`, `[keys.browse]`, `[keys.projects]` and `[keys.overlay]`. [keymap.md](keymap.md) lists every action by its id, with its keys in both presets and the contexts it applies in. Keys use vim notation: a plain character stands for itself (`j`, `G`, `gg` for a chord), and anything else goes inside `<...>`, as `<C-d>`, `<Esc>`, `<S-Tab>`, `<F1>`, `<Space>` and `<lt>` for `<`.

```toml
[keys.global]
quit = "Q"                              # replace: Q is now quit's only key
choose-theme = ","                      # an action with no default key
show-cheatsheet = { append = ["<F1>"] }  # F1 as well, after ?

[keys.list]
move-down = { prepend = ["n"] }         # n first, j still works
noop = ["gg", "<C-d>"]                  # these keys do nothing, and leave every hint
```

- A string or a list replaces the action's keys in that context.
- `{ prepend = [...] }` adds keys ahead of the defaults: they win where a key is bound twice, and they are the ones hints show first.
- `{ append = [...] }` adds keys after the defaults, which win where a key is bound twice.
- `noop` binds keys to nothing, which also hides them from every hint.

An unknown action, an action that does not apply in that context, or a key that is not valid notation is reported, and only that context's table falls back to the preset's keys. An unknown context is reported and ignored. A `preset` that is neither `vim` nor `standard` resets the whole `[keys]` table.

## `[ui]`

The table is accepted and takes no settings yet. A key inside it is reported as unknown.

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

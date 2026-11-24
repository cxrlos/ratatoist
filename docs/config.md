# Configuration

ratatoist reads `config.toml` from `$XDG_CONFIG_HOME/ratatoist/`, which is `~/.config/ratatoist/config.toml` by default, on macOS too. The file is optional: without it every setting has its default. ratatoist never writes it. The one exception is the one-time move of an old `api_token` line into the OS keyring, described in [auth.md](auth.md).

```toml
[ui]
icons = "unicode"
wide-layout-columns = 120

[keys]
preset = "vim"

[keys.list]
move-down = { prepend = ["n"] }

[theme]
name = "nord"

[sync]
idle-timeout = 300

[updates]
check = true
```

## `[ui]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `icons` | `"unicode"` or `"nerd"` | `"unicode"` | The glyphs for task state, priority, recurring, comments, folds, project kinds, the focused row, failed writes and the Overdue and Today markers. `"nerd"` needs a patched Nerd Font. |
| `wide-layout-columns` | 0 to 65535 | `120` | From this many columns on, a task list and the focused task's detail sit side by side and the pane keys (`h` and `l` in the vim preset) move between them. When my terminal is narrower, the list fills the body, Enter opens the detail and Esc goes back. |

The Unicode set, which any terminal font has:

| Meaning | Glyph |
| --- | --- |
| Open, completed task | `○` `●` |
| Priority p1 to p4 | `▮▮▮` `▮▮▯` `▮▯▯` `···` |
| Recurring | `↻` |
| Inbox, project, favourite | `▣` `#` `★` |
| Folder open, closed | `▾` `▸` |
| Subtasks shown, folded | `▿` `▹` |
| Comment count | `✉` |
| Focused row, failed write | `›` `✘` |
| Overdue, today | `▲` `◆` |

The Nerd set uses the Font Awesome glyphs of the same meaning, one cell each. Within a set every priority and every task state has its own shape, so none of them relies on colour.

## `[theme]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `name` | The name of a built-in theme, or the `name` inside a file in `themes/`. Case, spaces and hyphens do not matter, so `"rose-pine"` finds "Rose Pine". | `terminal` | The colours ratatoist draws with. |

The built-in themes are `terminal`, `catppuccin-mocha`, `dracula`, `gruvbox-dark`, `material-dark`, `monokai`, `nord`, `one-dark`, `rose-pine`, `solarized-dark` and `tokyo-night`. `terminal`, the default, draws with my terminal's own colours and paints no background. Every other theme paints its background behind the whole screen, since its colours are only legible on it.

Every theme draws with a few roles: text, muted for what is inactive or secondary, accent for what I can act on now (bold in titles and key hints), error, the border, and the focused row, which is reversed. Priorities p1 to p3 take the theme's red, orange and blue and p4 is muted; a due date is in the error colour when overdue, the accent today, text in the next six days and muted after. Muted text is lightened toward the text colour until it reaches a 3:1 contrast with the background, so it stays readable in every theme.

When the `NO_COLOR` environment variable is set to anything but an empty string, ratatoist draws with no colour at all, whatever `[theme] name` says. Bold still marks titles and key hints, and reverse video still marks the focused row. ratatoist reads `NO_COLOR` once, at startup.

## `[sync]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `idle-timeout` | Whole seconds, 0 to 86400. 0 turns idle detection off. | 300 | How long without a key press before the session counts as idle. |

ratatoist keeps what it last synced in `cache.json` in `$XDG_CACHE_HOME/ratatoist/` (`~/.cache/ratatoist/` by default), readable by my user alone. It holds my tasks, projects, labels and comments, the Todoist user they belong to and the sync token, never the API token. A start paints my tasks from it at once and then asks Todoist only for what changed. ratatoist writes it about two seconds after a sync changes something, and on quit. A file it cannot read, from an older version or of another Todoist user is thrown away for a full sync, so deleting it is always safe. `R` starts over from a full sync and replaces the file too. `--demo` and the Not logged in screen neither read nor write it. Versions before 0.8.0 kept only the sync token, in `sync_state.json` in the config folder; ratatoist deletes that file after its first sync.

## `[updates]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `check` | `true` or `false` | `true` | Whether ratatoist asks GitHub once a day for a newer release. |

With the check on, ratatoist asks GitHub's latest release of `cxrlos/ratatoist` in the background after the first frame, at most once a day, and keeps the answer in `release.json` in `$XDG_CACHE_HOME/ratatoist/` (`~/.cache/ratatoist/` by default). The request carries no token and no account data, only `ratatoist/VERSION` as its User-Agent, and it is the only request ratatoist makes to anything other than Todoist. When a newer stable release is out, the tab row shows a muted `vX.Y.Z available` left of the sync state while every tab's full title fits beside it, and the cheatsheet says how to upgrade: `brew upgrade ratatoist` for Homebrew, the README's install section for `cargo install`, `cargo binstall` and the shell installer. AUR and Nix installs show no notice, since their package manager owns updates. The palette's "What's new" opens the release page in my browser, and "Skip this release" hides the notice until a newer version is out. A failed check (offline, rate limited, a bad answer) only goes to the log.

Setting `RATATOIST_NO_UPDATE_CHECK` to anything but an empty string turns the check off too, whatever `check` says. `ratatoist --demo` and the `auth` commands never check.

## `ui_settings.json`

I set the theme and the idle timeout in `config.toml` and nowhere else. Versions before 0.7.0 saved them to `ui_settings.json` in the same folder. ratatoist still reads that file on every start, and never writes, changes or deletes it: a theme or idle timeout that `config.toml` does not set comes from the file, and a notice names the `config.toml` lines that keep it, such as `[theme] name = "nord"` and `[sync] idle-timeout = 120`. The notice comes back on each start until `config.toml` sets both values or I delete the file.

## `[keys]`

| Key | Values | Default | Effect |
| --- | --- | --- | --- |
| `preset` | `"vim"` or `"standard"` | `"vim"` | The keys I start from. `standard` moves with the arrows, Home/End and PgUp/PgDn, switches panes with Tab and Shift-Tab, adds a task with Ctrl-a and completes one with Ctrl-x; every other action keeps its vim key. |

Each context has its own table where I change the preset's keys: `[keys.global]`, `[keys.list]`, `[keys.detail]`, `[keys.browse]`, `[keys.projects]` and `[keys.overlay]`. [keymap.md](keymap.md) lists every action by its id, with its keys in both presets and the contexts it applies in. Keys use vim notation: a plain character stands for itself (`j`, `G`, `gg` for a chord), and anything else goes inside `<...>`, as `<C-d>`, `<Esc>`, `<S-Tab>`, `<F1>`, `<Space>` and `<lt>` for `<`.

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
- `{ prepend = [...] }` adds my keys ahead of the defaults: mine win where a key is bound twice, and hints show them first.
- `{ append = [...] }` adds my keys after the defaults, which win where a key is bound twice.
- `noop` binds keys to nothing, which also hides them from every hint.

If I name an unknown action, an action that does not apply in that context, or a key that is not valid notation, ratatoist reports it and only that context's table falls back to the preset's keys. An unknown context is reported and ignored. A `preset` that is neither `vim` nor `standard` resets the whole `[keys]` table.

Some actions have no default key and wait for the palette. `choose-theme` is one: to pick a theme from the keyboard I bind it, as in the example above. The idle timeout has no key at all; I set it with `[sync] idle-timeout`.

## Theme files

Every `*.json` file in `themes/` beside `config.toml` is read as a Base16 scheme: a `name` and the colours `base00` to `base0F` as `#rgb` or `#rrggbb` hex, the `#` optional. Each one becomes a theme I can pick by its `name`. The roles come from `base00` (background), `base03` (muted and borders), `base05` (text), `base08` (error and p1), `base09` (p2), `base0C` (accent) and `base0D` (p3); project dots also use `base0B` and `base0E`. A file that cannot be read or parsed is reported and skipped; the others still load.

## When something is wrong

Problems print to stderr before the TUI starts, one line each, naming the file, line and column:

```text
config.toml:5:16: `sync.idle-timeout` must be a whole number of seconds from 0 to 86400; [sync] uses its defaults
```

- A syntax error resets only the section it is in; the other sections still apply. A syntax error before the first `[section]` header resets the whole file to its defaults.
- A bad value resets its whole section to its defaults.
- An unknown key or table is reported and ignored.
- A `[theme] name` that matches no theme is reported, and the theme `ui_settings.json` names is used if that file names one, otherwise the default theme.
- A theme file that fails to load is reported as `themes/FILE.json:LINE:COL: problem; the theme is skipped`, or `themes/FILE.json: baseNN: problem; the theme is skipped` for a colour that is not one.

ratatoist still starts in every case, and the file is never rewritten.

`api_token` at the top of the file is the token setting of versions before 0.6.0. It is not reported as unknown, and the config loader never reads its value.

## The old macOS folder

Before 0.6.0, ratatoist kept its files on macOS in `~/Library/Application Support/ratatoist/`. If `~/.config/ratatoist/` does not exist and that folder does, ratatoist reads the old folder and prints the command that moves it:

```bash
mkdir -p ~/.config && mv ~/Library/Application\ Support/ratatoist ~/.config/ratatoist
```

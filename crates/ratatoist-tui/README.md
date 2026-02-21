# ratatoist-tui

Terminal user interface for Todoist. Installs as the `ratatoist` binary.

## Installation

```sh
cargo install --path .
```

Or from the workspace root:

```sh
cargo install --path crates/ratatoist-tui
```

## Configuration

Create `~/.config/ratatoist/config.toml`:

```toml
api_token = "your-todoist-api-token"
```

```sh
chmod 600 ~/.config/ratatoist/config.toml
```

Alternatively, set `TODOIST_API_TOKEN` as an environment variable.

## Key bindings

### Vim mode (default)

| Key | Context | Action |
|-----|---------|--------|
| `j` / `k` | Any list | Move down / up |
| `h` / `l` | Panes | Switch left / right |
| `g` / `G` | Any list | Jump to top / bottom |
| `Enter` | Projects | Focus tasks pane |
| `Enter` | Tasks | Open task detail |
| `Space` | Tasks | Toggle fold (expand/collapse subtasks) |
| `za` | Tasks | Toggle fold at cursor |
| `zR` / `zM` | Tasks | Open / close all folds |
| `x` | Tasks/Detail | Complete / uncomplete task |
| `a` | Tasks | Add new task (multi-field form) |
| `o` | Tasks | Cycle sort mode |
| `s` | Projects | Star / unstar project |
| `i` / `Enter` | Detail | Edit selected field |
| `p` | Detail | Open priority picker |
| `c` | Detail | Add comment |
| `j` / `k` | Detail | Navigate fields |
| `,` | Any | Toggle settings pane |
| `?` | Any | Show keybinding cheatsheet |
| `q` | Any | Quit |
| `Ctrl-c` | Any | Force quit |

### Standard mode

| Key | Action |
|-----|--------|
| `Up` / `Down` | Move selection |
| `Left` / `Right` | Switch pane |
| `Home` / `End` | Jump to top / bottom |
| `Tab` / `Shift-Tab` | Next / previous pane |
| `Enter` | Open / edit |
| `Esc` | Go back |
| `Ctrl-a` | Add task |
| `Ctrl-x` | Complete task |
| `q` | Quit |

Switch between modes via the settings pane (`,`).

## Flags

```
ratatoist            # start normally
ratatoist --debug    # enable debug logging to ~/.config/ratatoist/logs/
ratatoist --version  # print version
ratatoist --help     # print help
```

# Records docs/demo.gif with VHS (https://github.com/charmbracelet/vhs).
#
# ratatoist --demo runs on a fictional account, so this needs no Todoist
# account and no network. It runs target/release/ratatoist and does not
# build it, so build first, from the repository root:
#
#   cargo build --release --locked
#   vhs docs/demo.tape
#
# The config dir points at a directory that does not exist, so the recording
# uses the default config (and the terminal theme below) whatever my own
# config.toml says. It is set on the ratatoist command, not with Env: Env
# also reaches the browser VHS records with, which then fails to start.
# The detail sits beside the list from 120 columns; the size below gives
# 127 columns and 33 rows, so the first frames show the detail on the right.

Output docs/demo.gif

Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 640
Set Padding 16
Set Framerate 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 120ms

Hide
Type "clear && XDG_CONFIG_HOME=/nonexistent/ratatoist-demo target/release/ratatoist --demo"
Enter
Sleep 1.5s
Show

# Today, with the detail following the focused task.
Type "2"
Sleep 2s
Type "jj"
Sleep 1s
Type "j"
Sleep 1.2s
Type "k"
Sleep 1s

# The priority picker, then back out.
Type "p"
Sleep 1.5s
Escape
Sleep 600ms

# The palette finds actions, tasks and projects.
Type ":"
Sleep 600ms
Type "plumb"
Sleep 1.5s
Escape
Sleep 600ms

# Projects: open one, then step back.
Type "4"
Sleep 1.2s
Type "j"
Sleep 600ms
Enter
Sleep 1.8s
Escape
Sleep 800ms

# Labels.
Type "5"
Sleep 1.5s

# Every key of the current screen.
Type "2"
Sleep 600ms
Type "?"
Sleep 2.2s
Escape
Sleep 600ms

Hide
Type "q"
Sleep 300ms

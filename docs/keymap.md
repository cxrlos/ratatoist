# Keymap

<!-- Generated from the action registry in src/keys/registry.rs. Do not edit by hand: run `UPDATE_KEYMAP=1 cargo test --test keymap_reference`. -->

These are ratatoist's default keys, in the `vim` preset and in the `standard` preset that `[keys] preset = "standard"` picks. Keys are written in the vim notation `config.toml` uses, and each table heading names the config table that rebinds its actions; see [Configuration](config.md#keys).

The projects pane takes the Projects keys, then the Projects and Labels keys. Everywhere keys apply under all of them, and an overlay takes only the Overlays keys.

While the add-task form or a prompt has a text field, a character typed without Ctrl or Alt goes into the field instead, and so do `<Backspace>`, `<Delete>`, `<Left>`, `<Right>`, `<Home>`, `<End>`, `<C-w>` (delete the word before the cursor) and pasted text. Esc still closes it without sending anything, and Enter submits.

## Everywhere (`[keys.global]`)

| Action | Keys (vim) | Keys (standard) | Description |
| --- | --- | --- | --- |
| `show-inbox` | `1` | `1` | Show the Inbox |
| `show-today` | `2` | `2` | Show Today |
| `show-upcoming` | `3` | `3` | Show Upcoming |
| `show-projects` | `4` | `4` | Show Projects |
| `show-labels` | `5` | `5` | Show Labels |
| `resync` | `R` | `R` | Sync everything again |
| `show-cheatsheet` | `?` | `?` | Show every key for this context |
| `show-palette` | `:`, `<C-k>` | `<C-k>` | Find an action, task or project |
| `show-messages` | `!` | `!` | Show the messages log |
| `quit` | `q`, `<C-c>` | `q`, `<C-c>` | Quit ratatoist |
| `choose-theme` |  |  | Choose a theme for this session |
| `skip-release` |  |  | Skip this release |
| `release-notes` |  |  | What's new in the latest release |

## Task list (`[keys.list]`)

| Action | Keys (vim) | Keys (standard) | Description |
| --- | --- | --- | --- |
| `move-down` | `j`, `<Down>` | `<Down>` | Move down |
| `move-up` | `k`, `<Up>` | `<Up>` | Move up |
| `move-to-top` | `gg` | `<Home>` | Move to the top |
| `move-to-bottom` | `G` | `<End>` | Move to the bottom |
| `half-page-down` | `<C-d>` | `<PageDown>` | Move down half a page |
| `half-page-up` | `<C-u>` | `<PageUp>` | Move up half a page |
| `focus-left` | `h` | `<S-Tab>` | Focus the pane on the left |
| `focus-right` | `l` | `<Tab>` | Focus the pane on the right |
| `open` | `<Enter>` | `<Enter>` | Open the focused task |
| `close` | `<Esc>` | `<Esc>` | Close the top overlay or step back |
| `toggle-complete` | `x` | `<C-x>` | Complete the task, or reopen it |
| `add-task` | `a` | `<C-a>` | Add a task |
| `edit-priority` | `p` | `p` | Change the priority |
| `edit-due-date` | `d` | `d` | Set or clear the due date |
| `edit-content` | `e` | `e` | Edit the content |
| `edit-description` | `E` | `E` | Edit the description |
| `comment` | `c` | `c` | Comment on the task |
| `toggle-fold` | `za`, `<Space>` | `<Space>` | Fold or unfold the focused row |
| `open-all-folds` | `zR` | `zR` | Unfold every row |
| `close-all-folds` | `zM` | `zM` | Fold every row |
| `filter` | `f` | `f` | Pick what the list shows |
| `sort` | `vs` | `vs` | Pick how the list is sorted |
| `load-older-completed` |  |  | Load older completed tasks |

## Task detail (`[keys.detail]`)

| Action | Keys (vim) | Keys (standard) | Description |
| --- | --- | --- | --- |
| `move-down` | `j`, `<Down>` | `<Down>` | Move down |
| `move-up` | `k`, `<Up>` | `<Up>` | Move up |
| `move-to-top` | `gg` | `<Home>` | Move to the top |
| `move-to-bottom` | `G` | `<End>` | Move to the bottom |
| `half-page-down` | `<C-d>` | `<PageDown>` | Move down half a page |
| `half-page-up` | `<C-u>` | `<PageUp>` | Move up half a page |
| `line-down` | `<C-e>` | `<C-e>` | Scroll the detail down one row |
| `line-up` | `<C-y>` | `<C-y>` | Scroll the detail up one row |
| `focus-left` | `h` | `<S-Tab>` | Focus the pane on the left |
| `focus-right` | `l` | `<Tab>` | Focus the pane on the right |
| `close` | `<Esc>` | `<Esc>` | Close the top overlay or step back |
| `toggle-complete` | `x` | `<C-x>` | Complete the task, or reopen it |
| `add-task` | `a` | `<C-a>` | Add a task |
| `edit-priority` | `p` | `p` | Change the priority |
| `edit-due-date` | `d` | `d` | Set or clear the due date |
| `edit-content` | `e` | `e` | Edit the content |
| `edit-description` | `E` | `E` | Edit the description |
| `comment` | `c` | `c` | Comment on the task |

## Projects and Labels (`[keys.browse]`)

| Action | Keys (vim) | Keys (standard) | Description |
| --- | --- | --- | --- |
| `move-down` | `j`, `<Down>` | `<Down>` | Move down |
| `move-up` | `k`, `<Up>` | `<Up>` | Move up |
| `move-to-top` | `gg` | `<Home>` | Move to the top |
| `move-to-bottom` | `G` | `<End>` | Move to the bottom |
| `half-page-down` | `<C-d>` | `<PageDown>` | Move down half a page |
| `half-page-up` | `<C-u>` | `<PageUp>` | Move up half a page |
| `open-list` | `<Enter>` | `<Enter>` | Open the focused project's or label's tasks |
| `close` | `<Esc>` | `<Esc>` | Close the top overlay or step back |
| `toggle-fold` | `za`, `<Space>` | `<Space>` | Fold or unfold the focused row |

## Projects (`[keys.projects]`)

| Action | Keys (vim) | Keys (standard) | Description |
| --- | --- | --- | --- |
| `toggle-favourite` | `s` | `s` | Mark or unmark the project as a favourite |

## Overlays (`[keys.overlay]`)

| Action | Keys (vim) | Keys (standard) | Description |
| --- | --- | --- | --- |
| `scroll-down` | `j`, `<Down>`, `<C-n>` | `<Down>`, `<C-n>` | Scroll down or move the highlight down |
| `scroll-up` | `k`, `<Up>`, `<C-p>` | `<Up>`, `<C-p>` | Scroll up or move the highlight up |
| `accept` | `<Enter>` | `<Enter>` | Choose the highlighted entry or submit |
| `pick-1` | `1` | `1` | Pick entry 1 of the list |
| `pick-2` | `2` | `2` | Pick entry 2 of the list |
| `pick-3` | `3` | `3` | Pick entry 3 of the list |
| `pick-4` | `4` | `4` | Pick entry 4 of the list |
| `pick-5` | `5` | `5` | Pick entry 5 of the list |
| `pick-6` | `6` | `6` | Pick entry 6 of the list |
| `pick-7` | `7` | `7` | Pick entry 7 of the list |
| `pick-8` | `8` | `8` | Pick entry 8 of the list |
| `pick-9` | `9` | `9` | Pick entry 9 of the list |
| `confirm` | `y` | `y` | Confirm the question asked |
| `decline` | `n` | `n` | Cancel the question asked |
| `next-field` | `<Tab>` | `<Tab>` | Move to the next field |
| `previous-field` | `<S-Tab>` | `<S-Tab>` | Move to the previous field |
| `close` | `<Esc>` | `<Esc>` | Close the top overlay or step back |

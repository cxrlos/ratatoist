# One crate named ratatoist

ratatoist is one package, `ratatoist`, with a library target (`src/lib.rs`, so tests import `ratatoist::...`) and one binary, `ratatoist`. It replaces the workspace of `ratatoist-core` (the Todoist client, config and logging), `ratatoist-tui` (the binary) and `ratatoist-nvim` (a four-line stub). Nothing outside this repo depends on `ratatoist-core`, the Neovim plugin is out of scope, and the split made me keep two release flows, two sets of version numbers and twice the deny and MSRV config for one program. The nvim stub is deleted.

The version continues from the binary crate: the first release of `ratatoist` is 0.6.0, after `ratatoist-tui` 0.5.0 (0.4.0 is burned on crates.io). The final releases of `ratatoist-core` and `ratatoist-tui` point to `ratatoist`, so anyone who installed them finds the new crate.

## Considered options

- **Keep the workspace**: rejected. Once the nvim stub is gone, core has no consumer but the binary, and every tool (release, deny, MSRV, dist) needs workspace config for a split nobody uses.
- **Start the new crate at 0.1.0**: rejected. The binary has shipped up to 0.5.0, and users would see the version go backwards.

## Consequences

`cargo install ratatoist` is the one install command. The Todoist client lives in `src/todoist/` and is not published as its own library; if a second consumer appears (a Neovim plugin, a shared crate), splitting it out again is a new decision. The `ratatoist-core` and `ratatoist-tui` names stay reserved on crates.io and only ever receive the release that points here.

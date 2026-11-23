//! Checks that a panic restores the terminal before its message prints. Run
//! `cargo run --example force_panic`: the message should appear on the normal
//! screen, and `stty -a` afterwards should show `icanon` and `echo`.
//! `tests/pty.rs` checks the same from a pseudo-terminal.

use std::io;

use ratatoist::terminal::Guard;

fn main() -> io::Result<()> {
    let _terminal = Guard::enter()?;
    panic!("forced panic to check that the terminal is restored");
}

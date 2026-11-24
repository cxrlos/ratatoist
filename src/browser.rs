//! Opening URLs in the user's browser.

use std::process::{Command, Stdio};

use anyhow::Context;

#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(not(target_os = "macos"))]
const OPENER: &str = "xdg-open";

/// Starts `xdg-open` (Linux) or `open` (macOS) on `url` and returns without
/// waiting for it.
pub fn open(url: &str) -> anyhow::Result<()> {
    open_with(OPENER, url)
}

/// Runs `program url` detached from the terminal: its standard streams are
/// null so nothing it prints lands on the TUI.
fn open_with(program: &str, url: &str) -> anyhow::Result<()> {
    let mut child = Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not open the browser with {program}"))?;
    // Waiting on a thread reaps the opener so it does not linger as a zombie.
    let program = program.to_owned();
    std::thread::spawn(move || match child.wait() {
        Ok(status) if !status.success() => tracing::warn!(%status, "{program} failed"),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "waiting for {program}"),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_opener_is_an_error_that_names_it() {
        let error = open_with("ratatoist-no-such-opener", "https://example.com").unwrap_err();

        let message = format!("{error:#}");
        assert!(message.starts_with("could not open the browser with ratatoist-no-such-opener: "));
    }

    #[test]
    fn an_opener_that_starts_is_not_waited_for() {
        let started = std::time::Instant::now();

        // `sleep 3` stands in for an opener that keeps running.
        open_with("sleep", "3").unwrap();

        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}

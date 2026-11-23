//! The one owner of raw mode and the alternate screen.

use std::future::Future;
use std::io;
use std::panic;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use tokio::signal::unix::{SignalKind, signal};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK: Once = Once::new();

/// Holds the terminal in raw mode on the alternate screen. Dropping it, or a
/// panic anywhere while it lives, puts the terminal back.
#[derive(Debug)]
pub struct Guard(());

impl Guard {
    pub fn enter() -> io::Result<Self> {
        PANIC_HOOK.call_once(restore_before_panic_message);
        // From here on, an early return drops the guard and restores.
        let guard = Self(());
        take_over()?;
        Ok(guard)
    }
}

/// Raw mode, the alternate screen and a hidden cursor.
fn take_over() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    ACTIVE.store(true, Ordering::SeqCst);
    execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

/// Leaves raw mode and the alternate screen. Safe to call more than once:
/// only the first call after [`take_over`] writes anything.
fn restore() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Err(error) = terminal::disable_raw_mode() {
        tracing::warn!(%error, "could not leave raw mode");
    }
    if let Err(error) = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show) {
        tracing::warn!(%error, "could not leave the alternate screen");
    }
}

/// Restores the terminal before the panic message prints, so the message
/// lands on the main screen where it stays readable.
fn restore_before_panic_message() {
    let print_panic = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore();
        tracing::error!("{info}");
        print_panic(info);
    }));
}

/// Resolves on SIGINT, SIGTERM or SIGHUP, so the runtime can return and the
/// guard can restore the terminal instead of the process dying in raw mode.
/// Raw mode turns Ctrl-C into a key press, so SIGINT only comes from outside,
/// as with `kill -INT`.
pub fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
            _ = hangup.recv() => {}
        }
    })
}

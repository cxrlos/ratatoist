use std::path::Path;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;

/// Starts logging to a daily file under `log_dir`. Keep the guard until the
/// program ends, so the last lines are flushed.
pub fn init(log_dir: &Path) -> Result<WorkerGuard> {
    std::fs::create_dir_all(log_dir).context("failed to create log directory")?;

    let file_appender = tracing_appender::rolling::daily(log_dir, "ratatoist.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ratatoist=info,warn"));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_timer(UtcTime::rfc_3339())
        .with_target(true)
        .with_thread_ids(false)
        .with_ansi(false)
        .json();

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "ratatoist starting");

    Ok(guard)
}

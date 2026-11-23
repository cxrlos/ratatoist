//! File logging. Nothing is ever logged to the terminal, which belongs to
//! the UI.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use tracing::Subscriber;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::{FilterExt, filter_fn};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;

const LEVEL_VAR: &str = "RATATOIST_LOG";
const DEFAULT_LEVEL: &str = "ratatoist=info";
/// Days of logs kept, today's included.
const KEPT_FILES: usize = 7;

/// Starts logging to `ratatoist.<date>.log` under `log_dir` at the level in
/// `RATATOIST_LOG` (an `EnvFilter` string), `ratatoist=info` when unset.
/// `RUST_LOG` is ignored. Keep the guard until the program ends, so the last
/// lines are flushed.
pub fn init(log_dir: &Path) -> Result<WorkerGuard> {
    let (writer, guard) = tracing_appender::non_blocking(appender(log_dir)?);

    let (level, invalid_level) = match std::env::var(LEVEL_VAR) {
        Ok(value) => match EnvFilter::try_new(&value) {
            Ok(level) => (level, None),
            Err(error) => (EnvFilter::new(DEFAULT_LEVEL), Some(error)),
        },
        Err(_) => (EnvFilter::new(DEFAULT_LEVEL), None),
    };
    subscriber(writer, level)
        .try_init()
        .map_err(|error| anyhow!(error))
        .context("failed to start logging")?;
    if let Some(error) = invalid_level {
        tracing::warn!(%error, "ignoring {LEVEL_VAR}, logging at {DEFAULT_LEVEL}");
    }

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "ratatoist starting");
    Ok(guard)
}

/// A file under `log_dir` that rolls over daily, keeping the newest seven.
/// Creates `log_dir` if needed.
pub fn appender(log_dir: &Path) -> Result<RollingFileAppender> {
    std::fs::create_dir_all(log_dir)
        .with_context(|| format!("failed to create {}", log_dir.display()))?;
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ratatoist")
        .filename_suffix("log")
        .max_log_files(KEPT_FILES)
        .build(log_dir)
        .with_context(|| format!("failed to open a log file in {}", log_dir.display()))
}

/// The subscriber [`init`] installs: JSON lines to `writer`, at `level`.
/// tungstenite's handshake is never logged, whatever `level` asks for: at
/// trace it writes the whole upgrade request, URL and headers included.
pub fn subscriber<W>(writer: W, level: EnvFilter) -> impl Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let no_handshake =
        filter_fn(|metadata| !metadata.target().starts_with("tungstenite::handshake"));
    tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_timer(UtcTime::rfc_3339())
            .with_target(true)
            .with_ansi(false)
            .json()
            .with_filter(level.and(no_handshake)),
    )
}

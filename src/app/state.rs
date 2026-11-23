use chrono::{DateTime, Local, NaiveDate};

/// Everything the screen shows, and nothing else: no client, channel,
/// terminal or file handle. Rendering reads only this.
#[derive(Debug, Clone)]
pub struct State {
    /// Set by the runtime from the clock; fixed in tests.
    pub today: NaiveDate,
    pub now: DateTime<Local>,
    /// Treat the session as idle from the start.
    pub idle_forcer: bool,
}

impl State {
    pub fn new(now: DateTime<Local>) -> Self {
        Self {
            today: now.date_naive(),
            now,
            idle_forcer: false,
        }
    }
}

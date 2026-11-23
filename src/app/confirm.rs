//! Questions asked before something that discards unsent changes.

/// What a confirmation runs when the user says yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmed {
    /// Sync everything from scratch, dropping the commands not sent yet.
    Resync,
    /// Quit with commands not sent yet.
    Quit,
}

/// A question on screen: a title, the thing it is about, what saying yes
/// costs, and the verb the yes key shows (`Enter/y re-sync`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub title: String,
    pub subject: String,
    pub consequences: Vec<String>,
    pub verb: &'static str,
    pub confirmed: Confirmed,
}

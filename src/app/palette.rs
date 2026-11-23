//! The palette: find an action, a loaded task or a project by typing.

/// The open palette: what has been typed, and the highlighted row among the
/// matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

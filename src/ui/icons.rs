//! Glyphs, from the icon set the user picks with `ui.icons`. Within a set
//! every glyph of one column differs in shape, so nothing relies on colour
//! alone; the theme colours them.

use crate::todoist::model::Priority;

/// Which glyphs ratatoist draws with.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum IconSet {
    /// Plain Unicode that any terminal font has.
    #[default]
    Unicode,
    /// Glyphs from a patched Nerd Font.
    Nerd,
}

impl IconSet {
    /// Every icon set, the default first.
    pub const ALL: [IconSet; 2] = [IconSet::Unicode, IconSet::Nerd];

    /// Its value for `ui.icons` in `config.toml`.
    pub fn name(self) -> &'static str {
        match self {
            IconSet::Unicode => "unicode",
            IconSet::Nerd => "nerd",
        }
    }

    /// The icon set whose `ui.icons` value is `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|set| set.name() == name)
    }

    /// Every glyph of the set.
    pub fn icons(self) -> &'static Icons {
        match self {
            IconSet::Unicode => &UNICODE,
            IconSet::Nerd => &NERD,
        }
    }
}

/// The glyphs of one set, by what they mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icons {
    /// The check of an open task.
    pub open: &'static str,
    /// The check of a completed task.
    pub completed: &'static str,
    /// p1 to p4, always drawn so rows align.
    pub priorities: [&'static str; 4],
    pub recurring: &'static str,
    pub inbox: &'static str,
    pub project: &'static str,
    pub favourite: &'static str,
    pub folder_open: &'static str,
    pub folder_closed: &'static str,
    /// Before a task whose subtasks show, and one whose subtasks are folded.
    pub fold_open: &'static str,
    pub fold_closed: &'static str,
    /// Before a task's comment count.
    pub comments: &'static str,
    /// The marker of the focused row.
    pub focus: &'static str,
    /// The marker of a row whose write Todoist refused.
    pub failed: &'static str,
    /// The Overdue and Today group headers.
    pub overdue: &'static str,
    pub today: &'static str,
}

impl Icons {
    /// The check of a task, by whether it is completed.
    pub fn check(&self, completed: bool) -> &'static str {
        if completed { self.completed } else { self.open }
    }

    pub fn priority(&self, priority: Priority) -> &'static str {
        match priority {
            Priority::P1 => self.priorities[0],
            Priority::P2 => self.priorities[1],
            Priority::P3 => self.priorities[2],
            Priority::P4 => self.priorities[3],
        }
    }

    pub fn folder(&self, open: bool) -> &'static str {
        if open {
            self.folder_open
        } else {
            self.folder_closed
        }
    }

    pub fn fold(&self, open: bool) -> &'static str {
        if open {
            self.fold_open
        } else {
            self.fold_closed
        }
    }
}

const UNICODE: Icons = Icons {
    open: "○",
    completed: "●",
    priorities: ["▮▮▮", "▮▮▯", "▮▯▯", "···"],
    recurring: "↻",
    inbox: "▣",
    project: "#",
    favourite: "★",
    folder_open: "▾",
    folder_closed: "▸",
    fold_open: "▿",
    fold_closed: "▹",
    comments: "✉",
    focus: "▸",
    failed: "✘",
    overdue: "▲",
    today: "◆",
};

/// The Nerd Font set. Until its glyphs are chosen it repeats the Unicode
/// ones.
const NERD: Icons = UNICODE;

//! The `?` overlay: the keys of the context below it in panes by purpose,
//! flowing into as many columns as the width allows, or one scrolling
//! column on a narrow terminal.

use super::text;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::style;
use super::theme::Theme;
use crate::app::State;
use crate::keys::{self, ACTIONS, ActionSpec, Group};

const MAX_COLUMNS: usize = 4;
/// How wide a column grows to fill the popup.
const MAX_COLUMN_WIDTH: usize = 36;
/// Before each line of entries, under the pane's title.
const INDENT: &str = " ";
/// Between an entry's keys and its label.
const KEY_GAP: usize = 2;
/// Between the entries of a line, when a pane packs several per line.
const ENTRY_GAP: &str = "   ";
const DIVIDER: &str = " │ ";

/// Draws inside `area`'s border, scrolled down by `scroll` rows (no further
/// than its last row).
pub fn render(state: &State, scroll: u16, frame: &mut Frame, area: Rect) {
    let lines = lines(state, area, |spec| spec.availability(state).can_run());
    let popup = popup(area, &lines);
    let scroll = scroll.min(hidden_rows(popup, &lines));
    let edge = usize::from(popup.width.saturating_sub(2));
    let block = super::popup_block(state)
        .title(" Keys ")
        .title_bottom(text::fit_line(super::close_hint(state), edge));
    let lines: Vec<Line> = lines
        .into_iter()
        .map(|line| text::fit_line(line, edge))
        .collect();
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        popup,
    );
}

/// How many rows of the cheatsheet do not fit in `area`.
pub fn max_scroll(state: &State, area: Rect) -> u16 {
    // Availability only styles the lines, and the scroll actions' own
    // availability asks for this size, so it is left out here.
    let lines = lines(state, area, |_| true);
    hidden_rows(popup(area, &lines), &lines)
}

fn hidden_rows(popup: Rect, lines: &[Line]) -> u16 {
    u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_sub(popup.height.saturating_sub(2))
}

/// The box `lines` need inside its border, centered inside `area`'s border
/// and shrunk to fit.
fn popup(area: Rect, lines: &[Line]) -> Rect {
    let width = lines.iter().map(Line::width).max().unwrap_or(0) + 2;
    let height = lines.len() + 2;
    super::centered(
        area.inner(Margin::new(1, 1)),
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(height).unwrap_or(u16::MAX),
    )
}

/// One line of a pane: the keys of each of its actions with whether that
/// action can run now, and the label they share.
struct Entry {
    actions: Vec<(Vec<String>, bool)>,
    label: &'static str,
}

impl Entry {
    /// What the entry shows for each bound action, with whether it can run
    /// now: its keys joined by commas, as `:,C-k`. The caller puts a slash
    /// between actions, as `j/k`.
    fn keys(&self) -> Vec<(String, bool)> {
        self.actions
            .iter()
            .filter(|(keys, _)| !keys.is_empty())
            .map(|(keys, available)| (keys.join(","), *available))
            .collect()
    }

    fn keys_width(&self) -> usize {
        let keys = self.keys();
        let separators = keys.len().saturating_sub(1);
        keys.iter()
            .map(|(keys, _)| text::width(keys))
            .sum::<usize>()
            + separators
    }
}

struct Pane {
    group: Group,
    title: &'static str,
    entries: Vec<Entry>,
}

/// The panes of the contexts below the overlay, in [`Group`] order. Each
/// action is listed once, in the most specific context that has it, and
/// actions of one pane with the same label share an entry.
fn panes(state: &State, available: impl Fn(&ActionSpec) -> bool) -> Vec<Pane> {
    let contexts = keys::screen_contexts(state);
    let mut panes: Vec<Pane> = Vec::new();
    let mut listed = Vec::new();
    for &context in &contexts {
        for spec in ACTIONS.iter().filter(|s| s.contexts.contains(&context)) {
            if listed.contains(&spec.action) {
                continue;
            }
            listed.push(spec.action);
            let keys: Vec<String> = state
                .keymap
                .keys_for(spec.action, &contexts)
                .iter()
                .map(ToString::to_string)
                .collect();
            // An action only the palette runs has no line of keys to show.
            if keys.is_empty() && spec.default_keys.is_empty() {
                continue;
            }
            let action = (keys, available(spec));
            let title = spec.group.title().unwrap_or(context.title());
            let index = match panes.iter().position(|pane| pane.title == title) {
                Some(index) => index,
                None => {
                    panes.push(Pane {
                        group: spec.group,
                        title,
                        entries: Vec::new(),
                    });
                    panes.len() - 1
                }
            };
            let entries = &mut panes[index].entries;
            match entries.iter_mut().find(|entry| entry.label == spec.label) {
                Some(entry) => entry.actions.push(action),
                None => entries.push(Entry {
                    actions: vec![action],
                    label: spec.label,
                }),
            }
        }
    }
    panes.sort_by_key(|pane| pane.group);
    panes
}

/// The cheatsheet's lines for a popup inside `area`: its panes flowed into
/// the most columns that fit, each pane into the column that is shortest so
/// far, with a divider between columns.
fn lines(state: &State, area: Rect, available: impl Fn(&ActionSpec) -> bool) -> Vec<Line<'static>> {
    let theme = state.theme();
    let panes = panes(state, available);
    // The margin around the popup, its border and a space inside each side.
    let room = usize::from(area.width).saturating_sub(6);
    let natural = panes.iter().map(natural_width).max().unwrap_or(0);
    let fits = |columns: usize| columns * natural + text::width(DIVIDER) * (columns - 1) <= room;
    let columns = (1..=MAX_COLUMNS.min(panes.len()).max(1))
        .rev()
        .find(|&columns| fits(columns))
        .unwrap_or(1);
    let column_width = if columns == 1 {
        natural
    } else {
        ((room - text::width(DIVIDER) * (columns - 1)) / columns)
            .min(MAX_COLUMN_WIDTH)
            .max(natural)
    };

    let mut stacks: Vec<Vec<Vec<Span<'static>>>> = vec![Vec::new(); columns];
    for pane in &panes {
        let Some(stack) = stacks.iter_mut().min_by_key(|stack| stack.len()) else {
            break;
        };
        if !stack.is_empty() {
            stack.push(Vec::new());
        }
        stack.extend(pane_rows(pane, theme, column_width));
    }

    let height = stacks.iter().map(Vec::len).max().unwrap_or(0);
    let mut lines: Vec<Line<'static>> = (0..height)
        .map(|y| {
            let mut spans = vec![Span::raw(" ")];
            for (index, stack) in stacks.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(DIVIDER, style::border(theme)));
                }
                let row = stack.get(y).cloned().unwrap_or_default();
                let used: usize = row.iter().map(Span::width).sum();
                spans.extend(row);
                spans.push(Span::raw(" ".repeat(column_width.saturating_sub(used))));
            }
            spans.push(Span::raw(" "));
            Line::from(spans)
        })
        .collect();
    if let Some(version) = state.release.notice() {
        lines.push(Line::default());
        let line = match state.release.upgrade() {
            Some(how) => format!(" {INDENT}ratatoist v{version} is available: {how} "),
            None => format!(" {INDENT}ratatoist v{version} is available "),
        };
        lines.push(Line::styled(line, style::muted(theme)));
    }
    lines
}

/// How wide `pane` is with one entry per line.
fn natural_width(pane: &Pane) -> usize {
    text::width(pane.title).max(packed_width(&pane.entries, 1))
}

/// For each of `per_line` slots, the width of its keys and of the whole
/// slot, when the entries fill lines of `per_line` left to right.
fn slots(entries: &[Entry], per_line: usize) -> Vec<(usize, usize)> {
    (0..per_line)
        .map(|slot| {
            let in_slot = || entries.iter().skip(slot).step_by(per_line);
            let keys = in_slot().map(Entry::keys_width).max().unwrap_or(0);
            let label = in_slot()
                .map(|entry| text::width(entry.label))
                .max()
                .unwrap_or(0);
            (keys, keys + KEY_GAP + label)
        })
        .collect()
}

fn packed_width(entries: &[Entry], per_line: usize) -> usize {
    let slots = slots(entries, per_line);
    INDENT.len()
        + slots.iter().map(|(_, width)| width).sum::<usize>()
        + text::width(ENTRY_GAP) * per_line.saturating_sub(1)
}

/// A pane's title, then its entries, as many to a line as fit in `width`.
fn pane_rows(pane: &Pane, theme: &Theme, width: usize) -> Vec<Vec<Span<'static>>> {
    let per_line = (1..=pane.entries.len().max(1))
        .rev()
        .find(|&per_line| packed_width(&pane.entries, per_line) <= width)
        .unwrap_or(1);
    let slots = slots(&pane.entries, per_line);
    let mut rows = vec![vec![Span::styled(pane.title, style::accent(theme))]];
    for line in pane.entries.chunks(per_line) {
        let mut spans = vec![Span::raw(INDENT)];
        for (slot, entry) in line.iter().enumerate() {
            let (keys_width, slot_width) = slots.get(slot).copied().unwrap_or_default();
            if slot > 0 {
                spans.push(Span::raw(ENTRY_GAP));
            }
            spans.extend(entry_spans(entry, theme, keys_width));
            let last = slot + 1 == line.len();
            let used = keys_width + KEY_GAP + text::width(entry.label);
            if !last {
                spans.push(Span::raw(" ".repeat(slot_width.saturating_sub(used))));
            }
        }
        rows.push(spans);
    }
    rows
}

/// An entry's keys, padded to `keys_width`, then its label. Keys that
/// cannot run now are muted, and so is the label when none of them can.
fn entry_spans(entry: &Entry, theme: &Theme, keys_width: usize) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, (keys, available)) in entry.keys().into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("/", style::muted(theme)));
        }
        let style = if available {
            style::accent(theme)
        } else {
            style::muted(theme)
        };
        spans.push(Span::styled(keys, style));
    }
    let padding = keys_width.saturating_sub(entry.keys_width()) + KEY_GAP;
    spans.push(Span::raw(" ".repeat(padding)));
    let label_style = if entry.actions.iter().any(|(_, available)| *available) {
        Style::new()
    } else {
        style::muted(theme)
    };
    spans.push(Span::styled(entry.label, label_style));
    spans
}

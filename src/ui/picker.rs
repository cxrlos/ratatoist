//! A picker: the filter typed so far, the entries it leaves with the number
//! that picks each of the first nine, and the keys that choose and close.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph};

use super::{style, text};
use crate::app::{Action, Choice, ListFilter, Picker, State};
use crate::keys::{self, Context};

/// The width the list asks for at least, so short entries leave room for
/// the title and the keys at the bottom.
const MIN_WIDTH: usize = 34;

/// Draws `picker` centered inside `area`'s border, each entry led by its
/// number key and its glyph, if it has one.
pub fn render(state: &State, picker: &Picker, frame: &mut Frame, area: Rect) {
    let bottom = bottom_keys(state);
    render_with(
        state,
        &picker.title,
        picker,
        glyph(state),
        bottom,
        frame,
        area,
    );
}

/// The glyph of an entry: a priority's, a due bucket's or a project's, in
/// its theme colour.
fn glyph(state: &State) -> impl Fn(&Choice) -> Option<Span<'static>> + '_ {
    let theme = state.theme();
    let icons = state.icons();
    move |choice| match choice {
        Choice::Priority(priority) | Choice::Filter(ListFilter::Priority(priority)) => {
            Some(Span::styled(
                icons.priority(*priority),
                Style::new().fg(theme.priority(*priority)),
            ))
        }
        Choice::Filter(ListFilter::Overdue) => {
            Some(Span::styled(icons.overdue, theme.due_overdue()))
        }
        Choice::Filter(ListFilter::DueToday) => Some(Span::styled(icons.today, theme.due_today())),
        Choice::Project(id) => state.projects.iter().find(|p| p.id == *id).map(|project| {
            let icon = if project.is_inbox() {
                icons.inbox
            } else {
                icons.project
            };
            Span::styled(icon, theme.project_dot(&project.color))
        }),
        Choice::Filter(_) | Choice::Sort(_) | Choice::Theme(_) => None,
    }
}

/// Draws `picker` titled `title`, with `bottom` as the keys under it.
fn render_with(
    state: &State,
    title: &str,
    picker: &Picker,
    glyph: impl Fn(&Choice) -> Option<Span<'static>>,
    bottom: Line<'static>,
    frame: &mut Frame,
    area: Rect,
) {
    let theme = state.theme();
    let area = area.inner(Margin::new(1, 1));
    let number_keys: Vec<String> = (1..=9)
        .map(|n| {
            state
                .keymap
                .keys_for(Action::PickNumber(n), &[Context::Overlay])
                .first()
                .map(ToString::to_string)
                .unwrap_or_default()
        })
        .collect();
    let key_width = number_keys
        .iter()
        .map(|key| text::width(key))
        .max()
        .unwrap_or(0);

    let query = if picker.query().is_empty() {
        Line::from(vec![
            Span::styled("> ", style::accent(theme)),
            Span::styled("type to filter", style::muted(theme)),
        ])
    } else {
        Line::from(vec![
            Span::styled("> ", style::accent(theme)),
            Span::raw(picker.query().to_owned()),
        ])
    };

    let visible = picker.visible();
    let selected = picker.selected_position();
    let mut rows: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(position, (index, entry))| {
            let key = number_keys.get(position).cloned().unwrap_or_default();
            let mut spans = vec![Span::styled(
                format!("{key:<key_width$} "),
                style::accent(theme),
            )];
            if let Some(glyph) = glyph(&entry.value) {
                spans.push(glyph);
                spans.push(Span::raw(" "));
            }
            spans.push(Span::raw(entry.label.clone()));
            if picker.is_current(*index) {
                spans.push(Span::styled(" ✓", style::accent(theme)));
            }
            let line = Line::from(spans);
            if selected == Some(position) {
                line.style(theme.selection)
            } else {
                line
            }
        })
        .collect();
    let note = visible
        .is_empty()
        .then(|| Line::styled("No matches", style::muted(theme)));

    let width = rows
        .iter()
        .chain(note.iter())
        .chain([&query])
        .map(Line::width)
        .chain([text::width(title) + 2, bottom.width(), MIN_WIDTH])
        .max()
        .unwrap_or(0)
        + 4;
    let width = u16::try_from(width).unwrap_or(u16::MAX);
    let wanted = rows.len() + usize::from(note.is_some()) + 3;
    let popup = super::centered(area, width, u16::try_from(wanted).unwrap_or(u16::MAX));

    // Rows for entries once the border, the filter and any note are drawn.
    let room = usize::from(popup.height)
        .saturating_sub(3 + usize::from(note.is_some()))
        .max(1);
    let offset = selected.map_or(0, |position| (position + 1).saturating_sub(room));
    let mut lines = vec![query];
    lines.extend(rows.drain(..).skip(offset).take(room));
    lines.extend(note);

    let edge = usize::from(popup.width.saturating_sub(2));
    let block = super::popup_block(state)
        .title(text::fit(&format!(" {title} "), edge))
        .title_bottom(text::fit_line(bottom, edge));
    let lines: Vec<Line> = lines
        .into_iter()
        .map(|line| text::fit_line(line, edge.saturating_sub(2)))
        .collect();
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(block.padding(Padding::horizontal(1))),
        popup,
    );
}

/// The keys that select, choose and close, as
/// ` Down/Up select  Enter choose  Esc close `. Keys the filter takes as
/// text are left out. Choosing and closing say what the bottom bar says.
fn bottom_keys(state: &State) -> Line<'static> {
    let select: (&[Action], &str) = (&[Action::ScrollDown, Action::ScrollUp], "select");
    let choose: (&[Action], &str) = (&[Action::Accept], hint_word(Action::Accept));
    let close: (&[Action], &str) = (&[Action::Close], hint_word(Action::Close));
    keys_line(state, &[select, choose, close])
}

/// The word the bottom bar says after `action`'s key.
fn hint_word(action: Action) -> &'static str {
    keys::spec(action)
        .and_then(|spec| spec.hint)
        .map_or("", |hint| hint.word)
}

/// Each group of `items` as its keys and word, leaving out keys the text
/// field takes.
fn keys_line(state: &State, items: &[(&[Action], &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for &(actions, word) in items {
        let keys: Vec<String> = actions
            .iter()
            .filter_map(|&action| {
                state
                    .keymap
                    .keys_for(action, &[Context::Overlay])
                    .into_iter()
                    .find(|keys| keys.keys().first().is_none_or(|key| key.typed().is_none()))
                    .map(|keys| keys.to_string())
            })
            .collect();
        if keys.is_empty() {
            continue;
        }
        spans.push(Span::raw(" "));
        spans.push(Span::styled(keys.join("/"), style::accent(state.theme())));
        spans.push(Span::raw(format!(" {word} ")));
    }
    Line::from(spans)
}

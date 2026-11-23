//! Rendering: a pure function of [`State`].

pub mod components;
pub mod confirm;
pub mod dates;
mod error;
pub mod hints;
pub mod icons;
pub mod keyhints;
pub mod layout;
mod messages;
pub mod picker;
pub mod release;
pub mod style;
pub mod text;
pub mod theme;
mod upcoming;
pub mod views;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect, Size};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Tabs};

use crate::app::{Action, Modal, Pane, Screen, State};
use crate::keys;

/// The most rows the notice above the bottom bar takes.
pub const MAX_NOTICE_ROWS: usize = 3;

/// Draws the whole screen from `state`: the tab row, the current screen's
/// body, the notice, the bottom bar and the overlays over the body.
pub fn draw(frame: &mut Frame, state: &State) {
    let theme = state.theme();
    let area = frame.area();
    frame.buffer_mut().set_style(area, style::base(theme));
    let notice = notice_rows(state, area.width);
    let [tabs_area, body_area, notice_area, bar_area] = areas(area, notice.len());

    let selected = Screen::ALL
        .iter()
        .position(|&screen| screen == state.screen);
    let titles = tab_titles(state, tabs_area.width);
    let tabs_width = titles_width(&titles);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .style(style::muted(theme))
            .highlight_style(style::accent(theme)),
        tabs_area,
    );
    let indicator_width = render_sync_indicator(state, frame, tabs_area, tabs_width);
    release::render_release_notice(state, frame, tabs_area, tabs_width, indicator_width);
    render_body(state, frame, body_area);
    frame.render_widget(Paragraph::new(notice), notice_area);
    keyhints::render(frame, state, bar_area);

    for modal in &state.modals {
        match modal {
            Modal::Picker(picker) => picker::render(state, picker, frame, body_area),
            Modal::Confirm(confirm) => confirm::render(state, confirm, frame, body_area),
            Modal::Form(form) => components::task_form::render(frame, state, form),
            Modal::Input(target) => components::input_popup::render(frame, state, *target),
            Modal::Cheatsheet { .. } => {
                components::cheatsheet::render(frame, state.keymap.preset(), theme);
            }
            Modal::Messages { scroll } => messages::render(state, *scroll, frame, body_area),
            Modal::WhichKey { .. } | Modal::Palette(_) => {}
        }
    }
}

/// The current screen in `area`: the loading line or an error screen until
/// the tasks are in, then the screen's list, tree or the focused task's
/// detail.
fn render_body(state: &State, frame: &mut Frame, area: Rect) {
    let title = state.screen.title();
    if let Some(load_error) = state.load_error {
        error::render(
            load_error,
            state.token_source,
            title,
            state.theme(),
            frame,
            area,
        );
        return;
    }
    if state.loading {
        error::render_loading(state.theme(), title, frame, area);
        return;
    }
    if state.active_pane == Pane::Detail {
        render_detail(state, frame, area);
        return;
    }
    match state.screen {
        Screen::Inbox | Screen::Today => {
            layout::render_tasks_block(frame, state, area, state.list_title());
        }
        Screen::Upcoming => upcoming::render(state, frame, area),
        Screen::Projects if state.active_pane == Pane::Projects => {
            layout::render_projects_block(frame, state, area);
        }
        Screen::Projects => layout::render_tasks_block(frame, state, area, state.list_title()),
        Screen::Labels => error::render_not_built(state.theme(), title, frame, area),
    }
}

/// The focused task's detail, filling `area`, under why its last write was
/// rolled back when it was.
fn render_detail(state: &State, frame: &mut Frame, area: Rect) {
    let Some(task) = state.selected_task() else {
        return;
    };
    let reason = rollback_lines(state, area.width);
    let [reason_area, area] = split_detail(area, reason.len());
    frame.render_widget(Paragraph::new(reason), reason_area);
    views::detail::render(
        frame,
        task,
        &state.comments,
        &state.user_names,
        state.current_user_id.as_deref(),
        area,
        state.detail_scroll,
        state.detail_field,
        state.today,
        state.theme(),
        state.icons(),
    );
}

/// Every tab's title, as [`tab_title`] has it. When they do not fit in
/// `width` beside the sync indicator, the tabs of the other screens show
/// only their key, as `[2]`.
fn tab_titles(state: &State, width: u16) -> [String; Screen::ALL.len()] {
    let full = Screen::ALL.map(|screen| tab_title(state, screen, false));
    let indicator = state
        .indicator()
        .map_or(0, |indicator| text::width(&indicator.text) + 2);
    let room = usize::from(width).saturating_sub(indicator);
    if usize::from(titles_width(&full)) <= room {
        return full;
    }
    Screen::ALL.map(|screen| tab_title(state, screen, screen != state.screen))
}

/// The screen's title after the first key that shows it from here, as
/// `[1] Inbox`, or only the key when `short`, as `[1]`. The bare title if no
/// key reaches it (unbound, or shadowed by a binding of the current screen).
fn tab_title(state: &State, screen: Screen, short: bool) -> String {
    match state
        .keymap
        .keys_for(Action::ShowScreen(screen), &keys::screen_contexts(state))
        .first()
    {
        Some(key) if short => format!("[{key}]"),
        Some(key) => format!("[{key}] {}", screen.title()),
        None => screen.title().to_owned(),
    }
}

/// How wide [`Tabs`] draws `titles`: each padded by a space on both sides,
/// with a one-column divider between them.
fn titles_width(titles: &[String]) -> u16 {
    let padded: usize = titles.iter().map(|title| text::width(title) + 2).sum();
    let dividers = titles.len().saturating_sub(1);
    u16::try_from(padded + dividers).unwrap_or(u16::MAX)
}

/// The sync indicator at the right end of the tab row, muted while sync is
/// fine and accented when it is failing or the live connection is down.
/// Left out when it would touch the tabs. Returns how many columns it took.
fn render_sync_indicator(state: &State, frame: &mut Frame, area: Rect, tabs_width: u16) -> u16 {
    let Some(indicator) = state.indicator() else {
        return 0;
    };
    let text = format!("{} ", indicator.text);
    let width = u16::try_from(text::width(&text)).unwrap_or(u16::MAX);
    if tabs_width.saturating_add(width).saturating_add(1) > area.width {
        return 0;
    }
    let style = if indicator.alert {
        style::accent(state.theme())
    } else {
        style::muted(state.theme())
    };
    let [_, right] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)]).areas(area);
    frame.render_widget(Span::styled(text, style), right);
    width
}

/// The tab row, the body, the notice and the bottom bar, top to bottom. The
/// notice takes `notice_rows` rows, none when it is empty.
pub fn areas(area: Rect, notice_rows: usize) -> [Rect; 4] {
    let notice = notice_rows.min(MAX_NOTICE_ROWS);
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(u16::try_from(notice).unwrap_or(0)),
        Constraint::Length(hints::ROWS),
    ])
    .areas(area)
}

/// The body of a `viewport`-sized terminal: what the tab row, the notice
/// and the bottom bar leave.
pub fn body_area(state: &State, viewport: Size) -> Rect {
    let area = Rect::from((Position::ORIGIN, viewport));
    let [_, body_area, _, _] = areas(area, notice_rows(state, viewport.width).len());
    body_area
}

/// How many task rows fit in the body of a `viewport`-sized terminal,
/// inside its border. Page moves use it.
pub fn list_rows(state: &State, viewport: Size) -> usize {
    usize::from(body_area(state, viewport).height.saturating_sub(2))
}

/// Where the focused task's detail is drawn in a `viewport`-sized terminal:
/// the body, under why its last write was rolled back when it was.
pub fn detail_area(state: &State, viewport: Size) -> Rect {
    let body = body_area(state, viewport);
    let [_, detail] = split_detail(body, rollback_lines(state, body.width).len());
    detail
}

/// How many rows of the messages log do not fit in a `viewport`-sized
/// terminal, so scrolling stops at its last row.
pub fn messages_max_scroll(state: &State, viewport: Size) -> u16 {
    messages::max_scroll(state, body_area(state, viewport))
}

/// `area` split into `reason_rows` rows on top and the detail under them.
fn split_detail(area: Rect, reason_rows: usize) -> [Rect; 2] {
    Layout::vertical([
        Constraint::Length(u16::try_from(reason_rows).unwrap_or(0)),
        Constraint::Fill(1),
    ])
    .areas(area)
}

/// Why the focused task's last write was rolled back, in the error style,
/// in rows of `width` cells; none when it was not.
fn rollback_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let Some(failure) = state
        .selected_task()
        .and_then(|task| state.rollback_reason(&task.id))
    else {
        return Vec::new();
    };
    let reason = format!(
        "{} Could not {}: {}",
        state.icons().failed,
        failure.action,
        failure.message
    );
    notice_lines(&reason, width, style::error(state.theme()))
}

/// The notice under the body, cut into rows of `width` cells: the newest
/// failure in the error style until the next key, or else the plain notice,
/// muted.
fn notice_rows(state: &State, width: u16) -> Vec<Line<'static>> {
    let theme = state.theme();
    if state.show_failure
        && let Some(failure) = state.failures.newest()
    {
        return notice_lines(&failure.notice(), width, style::error(theme));
    }
    state
        .notice
        .as_deref()
        .map(|notice| notice_lines(notice, width, style::muted(theme)))
        .unwrap_or_default()
}

/// `paragraph` wrapped at spaces into rows of at most `room` cells; a word
/// wider than a row is cut.
pub(crate) fn wrap(paragraph: &str, room: usize) -> Vec<String> {
    let room = room.max(1);
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in paragraph.split_whitespace() {
        let joined = if row.is_empty() {
            word.to_owned()
        } else {
            format!("{row} {word}")
        };
        if text::width(&joined) <= room || row.is_empty() {
            row = joined;
        } else {
            rows.push(std::mem::replace(&mut row, word.to_owned()));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows.into_iter().map(|row| text::fit(&row, room)).collect()
}

/// `notice` wrapped at spaces into rows of `width` cells in `style`, at most
/// [`MAX_NOTICE_ROWS`]; the last ends in `…` when the rest does not fit.
fn notice_lines(notice: &str, width: u16, style: ratatui::style::Style) -> Vec<Line<'static>> {
    let room = usize::from(width.max(1));
    let mut rows = wrap(notice, room);
    let overflow = rows.len() > MAX_NOTICE_ROWS;
    rows.truncate(MAX_NOTICE_ROWS);
    let last = rows.len().saturating_sub(1);
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let row = if overflow && index == last {
                text::fit(&format!("{row} …"), room)
            } else {
                text::fit(&row, room)
            };
            Line::styled(row, style)
        })
        .collect()
}

/// The block of a popup: its border in the theme's border colour, and the
/// theme's text and background under it, since `Clear` resets them.
pub fn popup_block(state: &State) -> Block<'static> {
    Block::bordered()
        .border_style(style::border(state.theme()))
        .style(style::base(state.theme()))
}

/// A `width` by `height` box centred in `area`, shrunk to fit.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_notice_wraps_at_spaces() {
        let lines = notice_lines("one two three four", 9, ratatui::style::Style::new());

        assert_eq!(texts(&lines), ["one two", "three", "four"]);
    }

    #[test]
    fn a_notice_longer_than_three_rows_ends_in_an_ellipsis() {
        let lines = notice_lines("aa bb cc dd ee ff gg", 5, ratatui::style::Style::new());

        assert_eq!(texts(&lines), ["aa bb", "cc dd", "ee f…"]);
    }

    #[test]
    fn a_word_wider_than_the_row_is_cut() {
        let lines = notice_lines("abcdefgh", 4, ratatui::style::Style::new());

        assert_eq!(texts(&lines), ["abc…"]);
    }
}

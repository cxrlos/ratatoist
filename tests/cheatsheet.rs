//! The `?` cheatsheet: panes by purpose (Move, Edit the task, App and the
//! context's own keys) generated from the registry, flowing into columns
//! when wide and scrolling in one column when narrow.

#![cfg(test)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use jiff::Zoned;
use jiff::civil::date;
use jiff::tz::TimeZone;
use ratatoist::app::{App, Event, Modal};
use ratatoist::keys::{self, ACTIONS, Context, Group};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::sync::SyncResponse;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Size;
use ratatui::style::{Color, Modifier, Style};

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");
const STANDARD_PRESET: &str = "[keys]\npreset = \"standard\"\n";

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> Zoned {
    date(2026, 11, 22)
        .at(9, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
}

/// The app at `columns` x `rows` with `config` as `config.toml`, after the
/// full sync.
fn configured(config: &str, columns: u16, rows: u16) -> App<TestBackend> {
    let config = ratatoist::config::parse(config, &[]);
    assert_eq!(config.warnings, Vec::<String>::new());
    let state = initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    );
    let mut app = App::new(TestBackend::new(columns, rows), state).unwrap();
    let sync: Box<SyncResponse> = Box::new(serde_json::from_str(FULL_SYNC).unwrap());
    app.handle(Event::Synced(Ok(sync))).unwrap();
    app
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) {
    for event in events {
        app.handle(event).unwrap();
    }
}

fn screen(app: &App<TestBackend>) -> Vec<String> {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn text(app: &App<TestBackend>) -> String {
    screen(app).join("\n")
}

/// The foreground of the first cell of `needle` on the first row with it.
fn fg_of(app: &App<TestBackend>, needle: &str) -> Option<Color> {
    let rows = screen(app);
    let (y, line) = rows
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains(needle))?;
    let x = line[..line.find(needle)?].chars().count();
    let cell = app
        .backend()
        .buffer()
        .cell((u16::try_from(x).ok()?, u16::try_from(y).ok()?))?;
    Some(cell.fg)
}

/// The Inbox's task list at `columns` x `rows` with the cheatsheet open.
fn on_the_list(config: &str, columns: u16, rows: u16) -> App<TestBackend> {
    let mut app = configured(config, columns, rows);
    send(&mut app, [key('1'), key('?')]);
    assert_eq!(app.state().modals, [Modal::Cheatsheet { scroll: 0 }]);
    app
}

/// The panes of a one-column cheatsheet: each title with the lines under
/// it, read from inside the popup's border.
fn panes(app: &App<TestBackend>) -> Vec<(String, Vec<String>)> {
    let rows = screen(app);
    let top = rows.iter().position(|row| row.contains("┌ Keys")).unwrap();
    let left = rows[top].chars().position(|c| c == '┌').unwrap();
    let right = rows[top].chars().position(|c| c == '┐').unwrap();
    let mut panes: Vec<(String, Vec<String>)> = Vec::new();
    for row in &rows[top + 1..] {
        if row.chars().nth(left) == Some('└') {
            break;
        }
        let inside: String = row.chars().skip(left + 2).take(right - left - 2).collect();
        let inside = inside.trim_end();
        if inside.is_empty() {
            continue;
        }
        if inside.starts_with(' ') {
            if let Some((_, lines)) = panes.last_mut() {
                lines.push(inside.to_owned());
            }
        } else {
            panes.push((inside.to_owned(), Vec::new()));
        }
    }
    panes
}

#[test]
fn every_action_with_a_key_is_listed_in_its_group() {
    let list = || {
        let mut app = configured("", 60, 120);
        send(&mut app, [key('1')]);
        app
    };
    let detail = || {
        let mut app = list();
        send(&mut app, [key('j'), press(KeyCode::Enter)]);
        app
    };
    let projects = || {
        let mut app = configured("", 60, 120);
        send(&mut app, [key('4')]);
        app
    };
    for mut app in [list(), detail(), projects()] {
        let contexts = keys::screen_contexts(app.state());
        send(&mut app, [key('?')]);
        let panes = panes(&app);

        for spec in ACTIONS {
            let Some(&context) = contexts.iter().find(|c| spec.contexts.contains(c)) else {
                continue;
            };
            if app
                .state()
                .keymap
                .keys_for(spec.action, &contexts)
                .is_empty()
            {
                continue;
            }
            let keys: Vec<String> = app
                .state()
                .keymap
                .keys_for(spec.action, &contexts)
                .iter()
                .map(ToString::to_string)
                .collect();
            let title = spec.group.title().unwrap_or(context.title());
            let pane = panes.iter().find(|(name, _)| name == title);
            let listed = |line: &String| {
                let Some((shown, _)) = line.split_once(&format!("  {}", spec.label)) else {
                    return false;
                };
                let shown: Vec<&str> = shown.trim().split(['/', ',']).collect();
                keys.iter().all(|key| shown.contains(&key.as_str()))
            };
            assert!(
                pane.is_some_and(|(_, lines)| lines.iter().any(listed)),
                "{} under {title}",
                spec.id
            );
        }
    }
}

#[test]
fn the_list_shows_its_own_pane_beside_move_edit_and_app() {
    let app = on_the_list("", 120, 36);

    let text = text(&app);
    for title in [
        Group::Move.title(),
        Group::Edit.title(),
        Group::App.title(),
        Some(Context::List.title()),
    ]
    .into_iter()
    .flatten()
    {
        assert!(text.contains(&format!(" {title} ")), "{title}");
    }
}

#[test]
fn a_wide_terminal_puts_the_panes_side_by_side() {
    let app = on_the_list("", 120, 36);

    let rows = screen(&app);
    let row = rows.iter().find(|row| row.contains(" Move ")).unwrap();
    assert!(row.contains("Edit the task"), "{row}");
}

#[test]
fn pairs_share_a_line() {
    let app = on_the_list("", 120, 36);

    let text = text(&app);
    for pair in [
        "gg/G",
        "top / bottom",
        "C-d/C-u",
        "zR/zM",
        "unfold / fold all",
    ] {
        assert!(text.contains(pair), "{pair} in\n{text}");
    }
}

#[test]
fn a_narrow_terminal_scrolls_one_column_with_j_and_k() {
    let mut app = on_the_list("", 50, 24);
    let before = text(&app);
    assert!(!before.contains("quit"), "{before}");

    send(&mut app, (0..60).map(|_| key('j')));

    let Some(Modal::Cheatsheet { scroll }) = app.state().top_modal().cloned() else {
        panic!("the cheatsheet stays open");
    };
    let viewport = Size::new(50, 24);
    assert_eq!(
        scroll,
        ratatoist::ui::cheatsheet_max_scroll(app.state(), viewport)
    );
    assert!(scroll > 0);
    assert!(text(&app).contains("quit"), "{}", text(&app));

    send(&mut app, [key('k')]);
    assert_eq!(
        app.state().top_modal(),
        Some(&Modal::Cheatsheet { scroll: scroll - 1 })
    );
}

#[test]
fn scrolling_is_unavailable_when_the_cheatsheet_fits() {
    let mut app = on_the_list("", 120, 60);
    let frames = app.frames_drawn();

    send(&mut app, [key('j'), key('k')]);

    assert_eq!(app.frames_drawn(), frames);
    assert_eq!(
        ratatoist::ui::cheatsheet_max_scroll(app.state(), Size::new(120, 60)),
        0
    );
    let down = keys::spec(ratatoist::app::Action::ScrollDown).unwrap();
    assert!(!down.availability(app.state()).can_run());
}

#[test]
fn keys_that_cannot_run_now_are_muted() {
    let app = on_the_list("", 80, 24);

    let muted = Some(app.state().theme().muted);
    let accent = Some(app.state().theme().accent);
    assert_eq!(fg_of(&app, "h/l"), muted, "not wide");
    assert_eq!(fg_of(&app, "x  "), accent);
}

#[test]
fn the_cheatsheet_hides_what_is_behind_it() {
    let mut app = configured("", 120, 24);
    send(&mut app, [key('1')]);
    let behind = screen(&app);
    send(&mut app, [key('?')]);
    let over = screen(&app);

    let top = over.iter().position(|row| row.contains("┌ Keys")).unwrap();
    let bottom = over.iter().rposition(|row| row.contains('┘')).unwrap();
    let left = over[top].chars().position(|c| c == '┌').unwrap();
    let right = over[top].chars().position(|c| c == '┐').unwrap();
    let blanked = (top + 1..bottom).any(|y| {
        let before: Vec<char> = behind[y].chars().collect();
        let after: Vec<char> = over[y].chars().collect();
        (left + 1..right).any(|x| before[x] != ' ' && after[x] == ' ')
    });
    assert!(blanked, "{}", over.join("\n"));
}

#[test]
fn the_cheatsheet_resets_the_style_of_the_cells_behind_it() {
    let app = on_the_list("", 120, 36);
    let state = app.state().clone();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            let marked = Style::new().add_modifier(Modifier::REVERSED);
            frame.buffer_mut().set_style(area, marked);
            ratatoist::ui::cheatsheet::render(&state, 0, frame, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let top = (0..36)
        .find(|&y| (0..120).any(|x| buffer[(x, y)].symbol() == "┌"))
        .unwrap();
    let left = (0..120)
        .find(|&x| buffer[(x, top)].symbol() == "┌")
        .unwrap();
    let right = (0..120)
        .rfind(|&x| buffer[(x, top)].symbol() == "┐")
        .unwrap();
    let bottom = (top..36)
        .find(|&y| buffer[(left, y)].symbol() == "└")
        .unwrap();
    for y in top..=bottom {
        for x in left..=right {
            assert!(!buffer[(x, y)].modifier.contains(Modifier::REVERSED));
        }
    }
}

#[test]
fn the_cheatsheet_follows_the_standard_preset() {
    let app = on_the_list(STANDARD_PRESET, 120, 36);

    let text = text(&app);
    assert!(text.contains("C-x"), "{text}");
    assert!(!text.contains("gg/G"), "{text}");
    assert_snapshot!(text);
}

#[test]
fn the_cheatsheet_follows_overrides() {
    let app = on_the_list("[keys.list]\ntoggle-complete = \"X\"\n", 120, 36);

    let text = text(&app);
    assert!(text.contains("X    complete / reopen"), "{text}");
}

#[test]
fn cheatsheet_at_120x36() {
    assert_snapshot!(text(&on_the_list("", 120, 36)));
}

#[test]
fn cheatsheet_at_80x24() {
    assert_snapshot!(text(&on_the_list("", 80, 24)));
}

#[test]
fn cheatsheet_at_50x24() {
    assert_snapshot!(text(&on_the_list("", 50, 24)));
}

#[test]
fn cheatsheet_at_50x24_scrolled() {
    let mut app = on_the_list("", 50, 24);
    send(&mut app, (0..5).map(|_| key('j')));

    assert_snapshot!(text(&app));
}

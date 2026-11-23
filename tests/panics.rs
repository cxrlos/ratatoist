#![cfg(test)]
// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatoist::app::{App, Effect, Event, State};
use ratatoist::config;
use ratatoist::todoist::model::Comment;
use ratatoist::todoist::sync::SyncResponse;
use ratatoist::ui::theme::Theme;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

/// A theme with every colour valid; tests replace one key.
fn scheme_json(name: &str, key: &str, value: &str) -> String {
    let mut scheme = serde_json::json!({
        "name": name,
        "base00": "#101010", "base01": "#202020", "base02": "#303030", "base03": "#404040",
        "base04": "#b0b0b0", "base05": "#e0e0e0", "base06": "#f0f0f0", "base07": "#ffffff",
        "base08": "#ff5555", "base09": "#ffaa55", "base0A": "#ffff55", "base0B": "#55ff55",
        "base0C": "#55ffff", "base0D": "#5555ff", "base0E": "#ff55ff", "base0F": "#aa5500"
    });
    scheme[key] = serde_json::Value::String(value.to_owned());
    scheme.to_string()
}

fn theme(key: &str, value: &str) -> Result<Theme, String> {
    Theme::from_base16_json(&scheme_json("Test", key, value)).map_err(|error| error.to_string())
}

#[test]
fn a_three_digit_colour_doubles_each_digit() {
    let white = theme("base05", "#fff").unwrap();
    let teal = theme("base0C", "#0a8").unwrap();

    assert_eq!(white.text, Color::Rgb(255, 255, 255));
    assert_eq!(teal.accent, Color::Rgb(0x00, 0xaa, 0x88));
}

#[test]
fn the_hash_is_optional_as_in_base16_files() {
    assert_eq!(
        theme("base08", "eb6f92").unwrap().error,
        Color::Rgb(0xeb, 0x6f, 0x92)
    );
    assert_eq!(
        theme("base05", "fff").unwrap().text,
        Color::Rgb(255, 255, 255)
    );
}

#[test]
fn every_built_in_theme_loads() {
    assert_eq!(Theme::built_ins().len(), 11);
}

#[test]
fn a_six_digit_colour_parses_in_either_case() {
    assert_eq!(
        theme("base08", "#EB6f92").unwrap().error,
        Color::Rgb(0xeb, 0x6f, 0x92)
    );
}

#[test]
fn any_other_colour_is_an_error_naming_the_key() {
    for (key, value) in [
        ("base08", "#ff"),
        ("base0A", "#ffé000"),
        ("base0D", "red"),
        ("base00", "#"),
        ("base01", ""),
        ("base02", "#fffff"),
        ("base03", "#1234567"),
        ("base04", "#ggg"),
        ("base05", "##fff"),
        ("base06", "#+ff"),
    ] {
        let error = theme(key, value).unwrap_err();
        assert!(error.contains(key), "{value:?}: {error}");
    }
}

/// A fresh config folder for one test, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ratatoist-panics-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("themes")).unwrap();
        Self(dir)
    }

    fn theme(&self, file: &str, json: &str) {
        fs::write(self.0.join("themes").join(file), json).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn theme_files_with_bad_colours_load_without_panicking() {
    let dir = TempDir::new("colours");
    fs::write(dir.0.join("config.toml"), "[theme]\nname = \"Two\"\n").unwrap();
    dir.theme("short.json", &scheme_json("Short", "base05", "#fff"));
    dir.theme("two.json", &scheme_json("Two", "base08", "#ff"));
    dir.theme("accent.json", &scheme_json("Accent", "base0A", "#ffé000"));
    dir.theme("word.json", &scheme_json("Word", "base0D", "red"));

    let config = config::load(&dir.0);

    let names: Vec<&str> = config.user_themes.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Short"]);
    assert_eq!(config.user_themes[0].text, Color::Rgb(255, 255, 255));
    for (file, key) in [
        ("themes/two.json", "base08"),
        ("themes/accent.json", "base0A"),
        ("themes/word.json", "base0D"),
    ] {
        assert!(
            config
                .warnings
                .iter()
                .any(|w| w.starts_with(file) && w.contains(key) && w.contains("skipped")),
            "{file}: {:?}",
            config.warnings
        );
    }
    assert_eq!(
        config.theme, None,
        "a skipped theme falls back to the default"
    );
}

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn screen(app: &App<TestBackend>) -> String {
    let buffer = app.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Opens the detail of "Call the dentist" and answers its comment fetch
/// with `comments`.
fn detail_with_comments(comments: Vec<Comment>) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(160, 40), State::new(now())).unwrap();
    let sync: Box<SyncResponse> = Box::new(serde_json::from_str(FULL_SYNC).unwrap());
    app.handle(Event::Synced(Ok(sync))).unwrap();
    let effects: Vec<Effect> = [
        key(KeyCode::Char('1')),
        key(KeyCode::Char('j')),
        key(KeyCode::Enter),
    ]
    .into_iter()
    .flat_map(|event| app.handle(event).unwrap())
    .collect();
    let fetch_seq = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::FetchComments { fetch_seq, .. } => Some(*fetch_seq),
            _ => None,
        })
        .unwrap();
    app.handle(Event::Comments {
        task_id: "t-call".to_owned(),
        fetch_seq,
        result: Ok(comments),
    })
    .unwrap();
    app
}

fn comment(id: &str, uid: &str, posted_at: &str) -> Comment {
    Comment {
        id: id.to_owned(),
        content: format!("comment {id}"),
        posted_at: Some(posted_at.to_owned()),
        posted_by_uid: Some(uid.to_owned()),
        item_id: Some("t-call".to_owned()),
        ..Comment::default()
    }
}

#[test]
fn multi_byte_author_ids_urls_and_timestamps_render_without_panicking() {
    // Each value puts a two-byte character across the byte offset the old
    // code cut at: 8 for the id, 50 for the URL, 10, 11 and 16 for the time.
    let url = format!(
        "https://files.example.invalid/{}é/notes.pdf",
        "a".repeat(19)
    );
    assert!(!url.is_char_boundary(50));
    let mut with_url = comment("c2", "üser-two", "2026-11-21T10:05:00Z");
    with_url.content = String::new();
    with_url.attachment = Some(serde_json::json!({ "file_url": url }));
    let comments = vec![
        comment("c1", "abcdefgé-uid", "2026-11-20T09:15:00Z"),
        with_url,
        comment("c3", "u2", "2026-11-2éT09:1é:00"),
        comment("c4", "u1", "2026-11-21é10:05:00"),
        comment("c5", "u2", "２０２６-11-21"),
    ];

    let app = detail_with_comments(comments);

    let screen = screen(&app);
    assert!(screen.contains("abcdefgé"), "{screen}");
    assert!(!screen.contains("abcdefgé-"), "{screen}");
    assert!(
        screen.contains(&url.chars().take(50).collect::<String>()),
        "{screen}"
    );
    assert!(!screen.contains("notes.pdf"), "{screen}");
}

#[test]
fn comment_times_are_formatted_from_the_parsed_time() {
    let app = detail_with_comments(vec![
        comment("c1", "u2", "2026-11-20T09:15:00Z"),
        comment("c2", "u1", "2026-11-21T10:05:30.123456"),
        comment("c3", "u2", "not a time"),
    ]);

    let screen = screen(&app);
    assert!(screen.contains("2026-11-21 10:05"), "{screen}");
    assert!(screen.contains("not a time"), "{screen}");
}

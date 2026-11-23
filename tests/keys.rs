//! Keys driven through the app in both presets, the text fields, and the
//! `[keys]` config.

#![cfg(test)]

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatoist::app::{
    App, Effect, Event, ListFilter, Modal, Pane, PickerKind, PromptField, Screen, Sort, State,
};
use ratatoist::config;
use ratatoist::keys::{ACTIONS, Key, Preset, STANDARD};
use ratatoist::runtime::{UiSettings, initial_state_from};
use ratatoist::todoist::sync::{SyncCommand, SyncResponse};
use ratatui::backend::TestBackend;

const FULL_SYNC: &str = include_str!("fixtures/app/HYPOTHETICAL_full_sync.json");
const STANDARD_PRESET: &str = "[keys]\npreset = \"standard\"\n";

/// 2026-11-22 09:00 local time, a Sunday.
fn now() -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(2026, 11, 22)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .unwrap();
    Local.from_local_datetime(&naive).earliest().unwrap()
}

fn state_with(config: &str) -> State {
    let config = config::parse(config, &[]);
    assert_eq!(config.warnings, Vec::<String>::new());
    initial_state_from(
        &config,
        &UiSettings::default(),
        "*".to_owned(),
        now(),
        false,
    )
}

/// The app at 120x36 with `config` as `config.toml`, after the full sync.
fn app_with(config: &str) -> App<TestBackend> {
    let mut app = App::new(TestBackend::new(120, 36), state_with(config)).unwrap();
    let sync: Box<SyncResponse> = Box::new(serde_json::from_str(FULL_SYNC).unwrap());
    app.handle(Event::Synced(Ok(sync))).unwrap();
    app
}

fn vim() -> App<TestBackend> {
    app_with("")
}

fn standard() -> App<TestBackend> {
    app_with(STANDARD_PRESET)
}

/// The app in `preset`, for tests that run in both.
fn in_preset(preset: Preset) -> App<TestBackend> {
    match preset {
        Preset::Vim => vim(),
        Preset::Standard => standard(),
    }
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn ctrl(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn enter() -> Event {
    press(KeyCode::Enter)
}

fn esc() -> Event {
    press(KeyCode::Esc)
}

fn back_tab() -> Event {
    Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
}

fn typed(text: &str) -> Vec<Event> {
    text.chars().map(key).collect()
}

fn send(app: &mut App<TestBackend>, events: impl IntoIterator<Item = Event>) -> Vec<Effect> {
    events
        .into_iter()
        .flat_map(|event| app.handle(event).unwrap())
        .collect()
}

fn commands(effects: &[Effect]) -> Vec<SyncCommand> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SendCommands(commands) => Some(commands.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn command_types(effects: &[Effect]) -> Vec<String> {
    commands(effects).into_iter().map(|c| c.r#type).collect()
}

fn selected(app: &App<TestBackend>) -> String {
    app.state()
        .selected_task()
        .map(|task| task.id.clone())
        .unwrap_or_default()
}

fn visible(app: &App<TestBackend>, id: &str) -> bool {
    app.state().visible_tasks().iter().any(|task| task.id == id)
}

fn input(app: &App<TestBackend>) -> &str {
    app.state().input.text()
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

// The Inbox lists, open tasks only: t-milk, t-call, t-plants, t-trip, its
// subtask t-trip-train, and t-read.

#[test]
fn vim_keys_move_through_the_task_list() {
    let mut app = vim();
    send(&mut app, [key('1')]);
    assert_eq!(app.state().active_pane, Pane::Tasks);

    send(&mut app, [key('j')]);
    assert_eq!(selected(&app), "t-call");
    send(&mut app, [key('k'), key('k')]);
    assert_eq!(selected(&app), "t-read", "k wraps to the bottom");
    send(&mut app, [key('g'), key('g')]);
    assert_eq!(selected(&app), "t-milk");
    send(&mut app, [key('G')]);
    assert_eq!(selected(&app), "t-read");
    send(&mut app, [ctrl('u')]);
    assert_eq!(selected(&app), "t-milk");
    send(&mut app, [ctrl('d')]);
    assert_eq!(selected(&app), "t-read");
}

#[test]
fn the_standard_preset_moves_with_arrows_home_end_and_page_keys() {
    let mut app = standard();
    send(&mut app, [key('1')]);

    send(&mut app, [press(KeyCode::Down)]);
    assert_eq!(selected(&app), "t-call");
    send(&mut app, [press(KeyCode::Up)]);
    assert_eq!(selected(&app), "t-milk");
    send(&mut app, [press(KeyCode::End)]);
    assert_eq!(selected(&app), "t-read");
    send(&mut app, [press(KeyCode::Home)]);
    assert_eq!(selected(&app), "t-milk");
    send(&mut app, [press(KeyCode::PageDown)]);
    assert_eq!(selected(&app), "t-read");
    send(&mut app, [press(KeyCode::PageUp)]);
    assert_eq!(selected(&app), "t-milk");

    send(&mut app, [key('j'), key('G')]);
    assert_eq!(selected(&app), "t-milk", "vim letters do not move");
}

#[test]
fn the_standard_preset_switches_panes_with_tab_and_shift_tab() {
    let mut app = standard();
    send(&mut app, [key('1'), enter()]);
    assert_eq!(app.state().active_pane, Pane::Detail);

    send(&mut app, [back_tab()]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    send(&mut app, [press(KeyCode::Tab)]);
    assert_eq!(app.state().active_pane, Pane::Detail);
}

#[test]
fn vim_keys_switch_panes_with_h_and_l() {
    let mut app = vim();
    send(&mut app, [key('1'), enter()]);
    assert_eq!(app.state().active_pane, Pane::Detail);

    send(&mut app, [key('h')]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    send(&mut app, [key('l')]);
    assert_eq!(app.state().active_pane, Pane::Detail);
}

#[test]
fn the_standard_preset_adds_with_ctrl_a_and_completes_with_ctrl_x() {
    let mut app = standard();
    send(&mut app, [key('1')]);
    send(&mut app, [key('a'), key('x')]);
    assert!(app.state().modals.is_empty(), "a and x are unbound");

    let completed = send(&mut app, [ctrl('x')]);
    send(&mut app, [ctrl('a')]);
    send(&mut app, typed("Pay rent"));
    let added = send(&mut app, [enter()]);

    assert_eq!(command_types(&completed), ["item_close"]);
    assert_eq!(command_types(&added), ["item_add"]);
    assert_eq!(commands(&added)[0].args["content"], "Pay rent");
}

#[test]
fn the_standard_preset_changes_only_the_keys_it_lists() {
    for spec in ACTIONS {
        if STANDARD.iter().all(|(action, _)| *action != spec.action) {
            assert_eq!(
                Preset::Standard.default_keys(spec),
                spec.default_keys,
                "the standard preset keeps a key it does not list"
            );
        }
    }
}

#[test]
fn the_keys_both_presets_share_run_the_same_actions() {
    for preset in Preset::ALL {
        let mut app = in_preset(preset);
        send(&mut app, [key('1')]);

        let filtered = send(&mut app, [key('f'), key('2')]);
        assert_eq!(app.state().list_filter, ListFilter::Completed);
        assert!(matches!(
            filtered.as_slice(),
            [Effect::FetchCompletedTasks(_)]
        ));
        send(&mut app, [esc()]);
        assert_eq!(app.state().list_filter, ListFilter::Open);

        send(&mut app, [key('v'), key('s'), key('2')]);
        assert_eq!(app.state().sort, Sort::Priority);

        send(&mut app, [key('p')]);
        assert!(matches!(
            app.state().top_modal(),
            Some(Modal::Picker(picker)) if picker.kind == PickerKind::Priority
        ));
        send(&mut app, [esc()]);

        for (edit, field) in [
            ('e', PromptField::Content),
            ('E', PromptField::Description),
            ('d', PromptField::DueDate),
            ('c', PromptField::Comment),
        ] {
            send(&mut app, [key(edit)]);
            assert!(
                matches!(app.state().modals.as_slice(), [Modal::Prompt(prompt)] if prompt.field == field),
                "an edit key opens the prompt for its field"
            );
            send(&mut app, [esc()]);
        }
        send(&mut app, [esc()]);

        send(&mut app, [key('?')]);
        assert_eq!(app.state().modals, [Modal::Cheatsheet { scroll: 0 }]);
        send(&mut app, [esc()]);

        let resync = send(&mut app, [key('R')]);
        assert!(
            resync
                .iter()
                .any(|e| matches!(e, Effect::Sync(r) if r.sync_token == "*"))
        );

        send(&mut app, [key('3')]);
        assert_eq!(app.state().screen, Screen::Upcoming);

        assert_eq!(send(&mut app, [key('q')]), [Effect::Quit]);
        assert_eq!(send(&mut app, [ctrl('c')]), [Effect::Quit]);
    }
}

#[test]
fn folds_work_through_chords_and_space_in_both_presets() {
    for preset in Preset::ALL {
        let mut app = in_preset(preset);
        send(&mut app, [key('1')]);
        send(&mut app, (0..3).map(|_| press(KeyCode::Down)));
        assert_eq!(selected(&app), "t-trip");

        send(&mut app, [key('z')]);
        assert!(matches!(
            app.state().modals.as_slice(),
            [Modal::WhichKey { prefix }] if prefix == &[Key::char('z')]
        ));
        send(&mut app, [key('a')]);
        assert!(app.state().modals.is_empty());
        // The standard preset folds the row with Space alone.
        let za_folds = preset == Preset::Vim;
        assert_eq!(visible(&app, "t-trip-train"), !za_folds);

        send(&mut app, [key('z'), key('R')]);
        assert!(visible(&app, "t-trip-train"), "zR unfolds all");
        send(&mut app, [key('z'), key('M')]);
        assert!(!visible(&app, "t-trip-train"), "zM folds all");
        send(&mut app, [key(' ')]);
        assert!(visible(&app, "t-trip-train"), "Space unfolds");
    }
}

#[test]
fn a_key_that_ends_no_chord_closes_the_popup_and_does_nothing() {
    let mut app = vim();
    send(&mut app, [key('1')]);

    let effects = send(&mut app, [key('z'), key('q')]);

    assert!(effects.is_empty());
    assert!(app.state().modals.is_empty());
}

#[test]
fn space_folds_the_overdue_group_in_the_today_view() {
    let mut state = vim().state().clone();
    state.active_pane = Pane::Tasks;
    state.screen = Screen::Today;
    let mut app = App::new(TestBackend::new(120, 36), state).unwrap();

    send(&mut app, [key(' ')]);

    assert!(app.state().overdue_section_collapsed);
}

#[test]
fn esc_steps_back_from_the_detail_and_the_list() {
    let mut app = vim();
    send(&mut app, [key('4'), enter(), key('j'), enter()]);
    assert_eq!(app.state().active_pane, Pane::Detail);

    send(&mut app, [esc()]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
    send(&mut app, [esc()]);
    assert_eq!(app.state().active_pane, Pane::Browse);
}

#[test]
fn moves_in_the_detail_scroll_it_and_keep_the_focused_task() {
    let mut app = vim();
    send(&mut app, [key('1'), key('j'), enter()]);

    send(&mut app, [key('j'), key('G'), key('g'), key('g'), key('k')]);
    assert_eq!(app.state().active_pane, Pane::Detail);
    assert_eq!(selected(&app), "t-call");
    send(&mut app, [key('h')]);
    assert_eq!(app.state().active_pane, Pane::Tasks);
}

#[test]
fn s_marks_the_project_as_a_favourite() {
    let mut app = vim();

    let effects = send(&mut app, [key('4'), key('s')]);

    let command = &commands(&effects)[0];
    assert_eq!(command.r#type, "project_update");
    assert_eq!(command.args["is_favorite"], true);
}

#[test]
fn quitting_in_an_overlay_takes_esc_first() {
    let mut app = vim();
    send(&mut app, [key('?')]);

    let effects = send(&mut app, [key('q'), ctrl('c')]);

    assert!(effects.is_empty());
    send(&mut app, [esc()]);
    assert!(app.state().modals.is_empty());
    assert_eq!(send(&mut app, [key('q')]), [Effect::Quit]);
}

#[test]
fn only_the_keys_that_apply_everywhere_work_while_the_first_sync_runs() {
    let mut app = App::new(TestBackend::new(80, 24), state_with("")).unwrap();

    assert!(send(&mut app, [key('?')]).is_empty());
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Cheatsheet { .. })
    ));
    send(&mut app, [esc(), key('x')]);
    assert!(app.state().modals.is_empty());
    assert_eq!(send(&mut app, [key('q')]), [Effect::Quit]);
}

#[test]
fn a_key_release_does_nothing() {
    let mut app = vim();
    let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;

    assert!(send(&mut app, [Event::Key(release)]).is_empty());
}

// Text fields.

#[test]
fn a_text_field_takes_typed_keys_before_the_keymap() {
    let mut app = vim();
    send(&mut app, [key('1'), key('c')]);

    let effects = send(&mut app, typed("q? jk 1"));

    assert!(effects.is_empty());
    assert_eq!(input(&app), "q? jk 1");
    assert!(matches!(
        app.state().modals.as_slice(),
        [Modal::Prompt(prompt)] if prompt.field == PromptField::Comment
    ));
}

#[test]
fn the_cursor_moves_and_edits_in_a_text_field() {
    let mut app = vim();
    send(&mut app, [key('1'), key('c')]);
    send(&mut app, typed("helo"));

    send(&mut app, [press(KeyCode::Left)]);
    send(&mut app, typed("l"));
    assert_eq!(input(&app), "hello");
    send(&mut app, [press(KeyCode::Home)]);
    send(&mut app, typed(">"));
    send(&mut app, [press(KeyCode::End)]);
    send(&mut app, typed(" world"));
    assert_eq!(input(&app), ">hello world");
    send(&mut app, [ctrl('w')]);
    assert_eq!(input(&app), ">hello ");
    send(&mut app, [press(KeyCode::Backspace)]);
    send(&mut app, [press(KeyCode::Home), press(KeyCode::Right)]);
    send(&mut app, [press(KeyCode::Delete)]);
    assert_eq!(input(&app), ">ello");
    assert!(screen(&app).contains("> >ello"));
}

#[test]
fn a_paste_goes_into_the_text_field_on_one_line() {
    let mut app = vim();
    send(&mut app, [key('1'), key('c')]);
    send(&mut app, typed("see "));

    send(&mut app, [Event::Paste("the\nnotes".to_owned())]);
    let effects = send(&mut app, [enter()]);

    assert_eq!(commands(&effects)[0].args["content"], "see the notes");
}

#[test]
fn a_paste_outside_a_text_field_does_nothing() {
    let mut app = vim();
    let frames = app.frames_drawn();

    let effects = send(&mut app, [Event::Paste("x".to_owned())]);

    assert!(effects.is_empty());
    assert_eq!(app.frames_drawn(), frames);
}

#[test]
fn esc_in_a_prompt_or_the_form_sends_nothing() {
    for preset in Preset::ALL {
        let mut app = in_preset(preset);
        let add = match preset {
            Preset::Vim => key('a'),
            Preset::Standard => ctrl('a'),
        };
        send(&mut app, [key('1'), key('c')]);
        send(&mut app, typed("Never mind"));
        let comment = send(&mut app, [esc()]);
        send(&mut app, [add]);
        send(&mut app, typed("Pay rent"));
        send(&mut app, [press(KeyCode::Tab), press(KeyCode::Tab)]);
        send(&mut app, typed("tomorrow"));
        let form = send(&mut app, [esc()]);

        assert!(commands(&comment).is_empty());
        assert!(commands(&form).is_empty());
        assert!(app.state().modals.is_empty());
        assert!(app.state().tasks.iter().all(|t| t.content != "Pay rent"));
    }
}

/// An incremental sync that adds a task at the top of the Inbox, so every
/// row below it moves down one.
fn a_task_lands_at_the_top_of_the_inbox() -> Event {
    let json = r#"{
        "full_sync": false,
        "sync_token": "after",
        "items": [{
            "id": "t-first",
            "content": "Answer the landlord",
            "description": "",
            "checked": false,
            "child_order": 0,
            "priority": 1,
            "project_id": "p-inbox",
            "added_at": "2026-11-22T08:00:00Z"
        }]
    }"#;
    Event::Synced(Ok(Box::new(serde_json::from_str(json).unwrap())))
}

/// `text` typed into the open prompt, then Enter.
fn submitted(text: &str) -> Vec<Event> {
    let mut events = typed(text);
    events.push(enter());
    events
}

#[test]
fn an_edit_from_the_list_changes_the_task_it_opened_on_after_a_sync_reorders_the_list() {
    let openers: [(Event, Vec<Event>); 5] = [
        (key('e'), submitted(" x")),
        (key('E'), submitted(" x")),
        (key('d'), submitted(" x")),
        (key('p'), vec![key('1')]),
        (key('c'), submitted("On my way")),
    ];
    for (open, submit) in openers {
        let mut app = vim();
        send(&mut app, [key('1'), key('j')]);
        assert_eq!(selected(&app), "t-call");
        send(&mut app, [open]);

        send(&mut app, [a_task_lands_at_the_top_of_the_inbox()]);
        assert_ne!(selected(&app), "t-call");
        let effects = send(&mut app, submit);

        let sent = commands(&effects);
        assert_eq!(sent.len(), 1);
        let target = sent[0]
            .args
            .get("id")
            .or_else(|| sent[0].args.get("item_id"));
        assert_eq!(target.and_then(|id| id.as_str()), Some("t-call"));
        let shown = &app.state().comments;
        assert!(shown.iter().all(|comment| comment.content != "On my way"));
    }
}

#[test]
fn the_form_moves_between_fields_and_adds_the_task() {
    let mut app = vim();
    send(&mut app, [key('1'), key('a')]);
    send(&mut app, typed("Pay rent"));

    send(&mut app, [press(KeyCode::Tab), enter()]);
    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Picker(picker)) if picker.kind == PickerKind::Priority
    ));
    send(&mut app, [key('2')]);
    send(&mut app, [press(KeyCode::Tab)]);
    send(&mut app, typed("tomorrow"));
    send(&mut app, [back_tab(), back_tab()]);
    assert_eq!(input(&app), "Pay rent", "the content is kept");
    send(&mut app, typed(" now"));
    let effects = send(&mut app, [enter()]);

    let command = &commands(&effects)[0];
    assert_eq!(command.r#type, "item_add");
    assert_eq!(command.args["content"], "Pay rent now");
    assert_eq!(command.args["due_string"], "tomorrow");
    assert_eq!(command.args["priority"], 3);
}

// Overrides from config.toml.

#[test]
fn a_replaced_key_is_the_action_s_only_key_in_its_context() {
    let mut app = app_with("[keys.list]\ntoggle-complete = \"X\"\n");
    send(&mut app, [key('1')]);

    assert!(commands(&send(&mut app, [key('x')])).is_empty());
    assert_eq!(command_types(&send(&mut app, [key('X')])), ["item_close"]);
}

#[test]
fn an_override_applies_only_in_its_context() {
    let mut app = app_with("[keys.detail]\ntoggle-complete = \"X\"\n");
    send(&mut app, [key('1')]);

    assert_eq!(command_types(&send(&mut app, [key('x')])), ["item_close"]);
}

#[test]
fn prepended_and_appended_keys_run_the_action_beside_the_defaults() {
    let mut app = app_with(
        "[keys.list]\nmove-down = { prepend = [\"n\"] }\nmove-up = { append = [\"<C-p>\"] }\n",
    );
    send(&mut app, [key('1')]);

    send(&mut app, [key('n')]);
    assert_eq!(selected(&app), "t-call");
    send(&mut app, [key('j')]);
    assert_eq!(selected(&app), "t-plants");
    send(&mut app, [ctrl('p')]);
    assert_eq!(selected(&app), "t-call");
    send(&mut app, [key('k')]);
    assert_eq!(selected(&app), "t-milk");
}

#[test]
fn a_noop_key_does_nothing() {
    let mut app = app_with("[keys.list]\nnoop = [\"x\", \"zM\"]\n");
    send(&mut app, [key('1')]);
    let frames = app.frames_drawn();

    let effects = send(&mut app, [key('x')]);

    assert!(effects.is_empty());
    assert_eq!(app.frames_drawn(), frames);
}

#[test]
fn the_preset_and_overrides_combine() {
    let mut app = app_with("[keys]\npreset = \"standard\"\n\n[keys.list]\nmove-down = \"j\"\n");
    send(&mut app, [key('1')]);

    send(&mut app, [key('j')]);
    assert_eq!(selected(&app), "t-call");
    send(&mut app, [press(KeyCode::Down)]);
    assert_eq!(selected(&app), "t-call", "Down was replaced");
}

#[test]
fn choose_theme_has_no_key_until_one_is_bound() {
    let mut app = app_with("[keys.global]\nchoose-theme = \",\"\n");

    send(&mut app, [key(',')]);

    assert!(matches!(
        app.state().top_modal(),
        Some(Modal::Picker(picker)) if picker.kind == PickerKind::Theme
    ));
}

fn warnings(text: &str) -> Vec<String> {
    config::parse(text, &[]).warnings
}

#[test]
fn a_bad_binding_warns_with_its_place_and_resets_only_its_context() {
    let text = "[keys.global]\nquit = \"Q\"\n\n[keys.list]\nmove-down = \"<Nope>\"\n";
    let config = config::parse(text, &[]);

    assert_eq!(
        config.warnings,
        ["config.toml:5:13: `<Nope>` is not a key; [keys.list] uses the default keys"]
    );
    assert_eq!(config.keys.overrides.len(), 1);
    assert_eq!(config.keys.overrides[0].keys[0].to_string(), "Q");
}

#[test]
fn unknown_actions_and_contexts_warn() {
    assert_eq!(
        warnings("[keys.list]\nfly = \"f\"\n"),
        ["config.toml:2:1: unknown action `fly`; [keys.list] uses the default keys"]
    );
    assert_eq!(
        warnings("[keys.list]\nresync = \"r\"\n"),
        [
            "config.toml:2:1: `resync` does not apply in [keys.list]; [keys.list] uses the default keys"
        ]
    );
    assert_eq!(
        warnings("[keys.sidebar]\nquit = \"Q\"\n"),
        [
            "config.toml:1:7: unknown context `[keys.sidebar]` is ignored; [keys] takes `preset` \
             and the contexts global, list, detail, browse, projects, overlay"
        ]
    );
    assert_eq!(
        warnings("[keys.list]\nmove-down = { insert = [\"n\"] }\n"),
        [
            "config.toml:2:15: `insert` is not `prepend` or `append`; [keys.list] uses the default keys"
        ]
    );
}

#[test]
fn a_bad_preset_resets_the_keys_and_nothing_else() {
    let config = config::parse(
        "[keys]\npreset = \"emacs\"\n\n[keys.global]\nquit = \"Q\"\n\n[sync]\nidle-timeout = 60\n",
        &[],
    );

    assert_eq!(
        config.warnings,
        [
            "config.toml:2:10: `keys.preset` must be \"vim\" or \"standard\"; \
             [keys] uses the default keys"
        ]
    );
    assert_eq!(config.keys, config::Keys::default());
    assert_eq!(
        config.idle_timeout,
        Some(std::time::Duration::from_secs(60))
    );
}

/// Source files under `dir`, recursively.
fn sources(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

#[test]
fn no_key_is_bound_outside_the_registry() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let keys = src.join("keys");
    let offenders: Vec<_> = sources(&src)
        .into_iter()
        .filter(|path| !path.starts_with(&keys))
        .filter(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            code.contains("KeyCode::Char")
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "a KeyCode::Char match outside src/keys/ binds a key the registry does not know"
    );
}

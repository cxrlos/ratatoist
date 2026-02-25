mod app;
mod keys;
mod ui;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use ratatoist_core::api::client::TodoistClient;
use ratatoist_core::config::Config;
use ratatoist_core::logging;

use app::App;

#[derive(Parser)]
#[command(name = "ratatoist", version, about = "A terminal UI for Todoist")]
struct Cli {
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    idle_forcer: bool,
    #[arg(
        long,
        help = "Simulate new-user onboarding without touching your config"
    )]
    new_user: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let _log_guard = logging::init(cli.debug)?;

    let mut terminal = ratatui::init();

    let (client, ephemeral) = if cli.new_user {
        match run_new_user_setup(&mut terminal).await {
            Ok(token) => {
                run_alias_setup(&mut terminal).await;
                match TodoistClient::new(&token) {
                    Ok(c) => (c, true),
                    Err(e) => {
                        ratatui::restore();
                        eprintln!("Failed to initialize API client: {e:#}");
                        std::process::exit(1);
                    }
                }
            }
            Err(_) => {
                ratatui::restore();
                return Ok(());
            }
        }
    } else {
        let (client, ephemeral) = match Config::load() {
            Ok(c) => match TodoistClient::new(c.token()) {
                Ok(client) => (client, false),
                Err(e) => {
                    ratatui::restore();
                    eprintln!("Failed to initialize API client: {e:#}");
                    std::process::exit(1);
                }
            },
            Err(_) => match run_new_user_setup(&mut terminal).await {
                Ok(token) => {
                    if let Err(e) = Config::save_token(&token) {
                        ratatui::restore();
                        eprintln!("Failed to save config: {e:#}");
                        std::process::exit(1);
                    }
                    run_alias_setup(&mut terminal).await;
                    match TodoistClient::new(&token) {
                        Ok(c) => (c, false),
                        Err(e) => {
                            ratatui::restore();
                            eprintln!("Failed to initialize API client: {e:#}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(_) => {
                    ratatui::restore();
                    return Ok(());
                }
            },
        };
        (client, ephemeral)
    };

    let mut app = App::new(client, cli.idle_forcer, ephemeral);

    app.load_with_splash(&mut terminal).await;

    let result = app.run(&mut terminal).await;
    ratatui::restore();

    result
}

async fn run_alias_setup(terminal: &mut ratatui::DefaultTerminal) {
    let themes = ui::theme::Theme::builtin();
    let theme = &themes[0];

    let Some(rc_path) = detect_shell_rc() else {
        return;
    };
    let rc_display = rc_path
        .to_str()
        .unwrap_or("")
        .replace(&std::env::var("HOME").unwrap_or_default(), "~");

    let mut selected: usize = 0;
    let mut custom_input = String::new();
    let mut is_typing = false;
    let mut status: Option<String> = None;

    loop {
        terminal
            .draw(|f| {
                ui::setup::render_alias(
                    f,
                    selected,
                    &custom_input,
                    is_typing,
                    &rc_display,
                    status.as_deref(),
                    theme,
                )
            })
            .ok();

        let Ok(true) = event::poll(Duration::from_millis(16)) else {
            continue;
        };
        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            break;
        }

        if is_typing {
            match key.code {
                KeyCode::Esc => {
                    is_typing = false;
                    custom_input.clear();
                    status = None;
                }
                KeyCode::Backspace => {
                    custom_input.pop();
                    status = None;
                }
                KeyCode::Char(c) => {
                    custom_input.push(c);
                    status = None;
                }
                KeyCode::Enter if !custom_input.trim().is_empty() => {
                    let name = custom_input.trim().to_string();
                    commit_alias(&name, &rc_path, &rc_display, &mut status, terminal, theme).await;
                    break;
                }
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Esc => break,
            KeyCode::Char('j') | KeyCode::Down => selected = (selected + 1) % 3,
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.checked_sub(1).unwrap_or(2);
            }
            KeyCode::Enter => match selected {
                0 => {
                    commit_alias("rat", &rc_path, &rc_display, &mut status, terminal, theme).await;
                    break;
                }
                1 => is_typing = true,
                _ => break,
            },
            _ => {}
        }
    }
}

async fn commit_alias(
    name: &str,
    rc_path: &Path,
    rc_display: &str,
    status: &mut Option<String>,
    terminal: &mut ratatui::DefaultTerminal,
    theme: &ui::theme::Theme,
) {
    match write_alias(name, rc_path) {
        Ok(()) => {
            *status = Some(format!("added  alias {name}='ratatoist'  to {rc_display}"));
            terminal
                .draw(|f| {
                    ui::setup::render_alias(f, 0, name, false, rc_display, status.as_deref(), theme)
                })
                .ok();
            std::thread::sleep(Duration::from_millis(1200));
        }
        Err(e) => {
            *status = Some(format!("could not write: {e}"));
        }
    }
}

fn detect_shell_rc() -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = std::env::var("HOME").ok()?;
    let home = PathBuf::from(home);
    if shell.contains("zsh") {
        Some(home.join(".zshrc"))
    } else if shell.contains("bash") {
        let profile = home.join(".bash_profile");
        if profile.exists() {
            Some(profile)
        } else {
            Some(home.join(".bashrc"))
        }
    } else {
        None
    }
}

fn write_alias(name: &str, rc_path: &Path) -> Result<()> {
    let mut file = std::fs::OpenOptions::new().append(true).open(rc_path)?;
    writeln!(file, "\nalias {name}='ratatoist'")?;
    Ok(())
}

async fn run_new_user_setup(terminal: &mut ratatui::DefaultTerminal) -> Result<String> {
    let themes = ui::theme::Theme::builtin();
    let theme = &themes[0];

    let mut input = String::new();
    let mut error: Option<String> = None;

    loop {
        terminal
            .draw(|f| ui::setup::render(f, &input, error.as_deref(), false, theme))
            .ok();

        if !event::poll(Duration::from_millis(16))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            anyhow::bail!("cancelled");
        }

        match key.code {
            KeyCode::Esc => anyhow::bail!("cancelled"),

            KeyCode::Backspace => {
                input.pop();
                error = None;
            }

            KeyCode::Char(c) => {
                input.push(c);
                error = None;
            }

            KeyCode::Enter if !input.is_empty() => {
                let token = input.trim().to_string();
                terminal
                    .draw(|f| ui::setup::render(f, &token, None, true, theme))
                    .ok();

                match TodoistClient::new(&token) {
                    Err(e) => {
                        error = Some(format!("invalid token characters: {e}"));
                    }
                    Ok(client) => match client.get_user().await {
                        Ok(_) => return Ok(token),
                        Err(_) => {
                            error =
                                Some("token not recognized — check it and try again".to_string());
                            input.clear();
                        }
                    },
                }
            }

            _ => {}
        }
    }
}

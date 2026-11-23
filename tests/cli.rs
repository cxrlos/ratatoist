// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use clap::error::ErrorKind;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ratatoist::cli::{AuthCommand, Cli, Command};

fn parse(args: &[&str]) -> Option<Command> {
    Cli::try_parse_from(args).unwrap().command
}

#[test]
fn ratatoist_alone_has_no_subcommand() {
    let cli = Cli::try_parse_from(["ratatoist"]).unwrap();
    assert!(cli.command.is_none());
    assert!(!cli.demo);
}

#[test]
fn the_auth_subcommands_parse() {
    assert!(matches!(
        parse(&["ratatoist", "auth", "login"]),
        Some(Command::Auth(AuthCommand::Login { stdin: false }))
    ));
    assert!(matches!(
        parse(&["ratatoist", "auth", "login", "--stdin"]),
        Some(Command::Auth(AuthCommand::Login { stdin: true }))
    ));
    assert!(matches!(
        parse(&["ratatoist", "auth", "status"]),
        Some(Command::Auth(AuthCommand::Status))
    ));
    assert!(matches!(
        parse(&["ratatoist", "auth", "logout"]),
        Some(Command::Auth(AuthCommand::Logout))
    ));
}

#[test]
fn demo_cannot_be_combined_with_a_subcommand() {
    let error = Cli::try_parse_from(["ratatoist", "--demo", "auth", "status"]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{error}");
    assert!(Cli::try_parse_from(["ratatoist", "--demo"]).unwrap().demo);
}

#[test]
fn new_user_and_debug_are_unknown_flags() {
    for flag in ["--new-user", "--debug"] {
        let error = Cli::try_parse_from(["ratatoist", flag]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnknownArgument, "{flag}: {error}");
    }
}

/// A fresh home folder for one run, removed when dropped.
struct TempHome(PathBuf);

impl TempHome {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ratatoist-cli-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Regression: onboarding used to append a shell alias to the user's rc
/// file. The demo, run in a terminal and quit, leaves no rc file behind.
#[test]
fn a_demo_run_writes_no_shell_rc_file() {
    const PATIENCE: Duration = Duration::from_secs(10);
    let home = TempHome::new("rcfiles");
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_ratatoist"));
    command.arg("--demo");
    command.env_clear();
    command.env("PATH", std::env::var_os("PATH").unwrap_or_default());
    command.env("TERM", "xterm-256color");
    command.env("HOME", &home.0);
    command.env("SHELL", "/bin/zsh");
    for (var, dir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = home.0.join(dir);
        fs::create_dir(&path).unwrap();
        command.env(var, path);
    }
    let mut child = pair.slave.spawn_command(command).unwrap();

    let output = Arc::new(Mutex::new(Vec::new()));
    let mut reader = pair.master.try_clone_reader().unwrap();
    let sink = Arc::clone(&output);
    thread::spawn(move || {
        let mut buffer = [0; 4096];
        while let Ok(read @ 1..) = reader.read(&mut buffer) {
            sink.lock().unwrap().extend_from_slice(&buffer[..read]);
        }
    });
    let mut writer = pair.master.take_writer().unwrap();

    let start = Instant::now();
    while !String::from_utf8_lossy(&output.lock().unwrap()).contains("\x1b[?1049h") {
        assert!(start.elapsed() < PATIENCE, "the demo never entered the TUI");
        thread::sleep(Duration::from_millis(20));
    }
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(start.elapsed() < PATIENCE, "the demo did not quit on q");
        thread::sleep(Duration::from_millis(20));
    };

    assert!(status.success(), "{status:?}");
    for rc in [".zshrc", ".bashrc", ".bash_profile"] {
        assert!(!home.0.join(rc).exists(), "{rc} was written");
    }
}

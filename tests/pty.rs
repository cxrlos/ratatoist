//! End-to-end checks of the terminal from outside ratatoist: the real binary
//! runs with `--demo` in a pseudo-terminal, under an empty HOME and empty XDG
//! directories, and the tests read the terminal's mode and output as a shell
//! would.

// clippy's allow-unwrap-in-tests does not cover helpers outside #[test] functions.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, SlavePty, native_pty_system};
use rustix::termios::LocalModes;

/// How long anything may take before a test gives up.
const PATIENCE: Duration = Duration::from_secs(10);
const ENTER_ALTERNATE_SCREEN: &str = "\x1b[?1049h";
const LEAVE_ALTERNATE_SCREEN: &str = "\x1b[?1049l";
/// Shell startup files ratatoist must never create or touch.
const SHELL_RC_FILES: [&str; 3] = [".zshrc", ".bashrc", ".bash_profile"];

/// A program running in a pseudo-terminal of 100 by 30 cells.
struct Pty {
    master: Box<dyn MasterPty + Send>,
    /// Held so the terminal outlives the program and its mode can still be
    /// read once it exits.
    _slave: Box<dyn SlavePty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Arc<Mutex<Vec<u8>>>,
    /// Set once the reader has read everything the terminal will ever give.
    drained: Arc<AtomicBool>,
    home: TempDir,
}

impl Pty {
    /// `ratatoist --demo`, started from an interactive bash with no startup
    /// files, as at a prompt. On macOS a binary spawned straight into the
    /// pseudo-terminal does not behave as one started from a shell. `exec`
    /// keeps the pid ratatoist's, so signals reach it and not bash.
    fn ratatoist() -> Self {
        Self::ratatoist_with_config(None)
    }

    /// [`Pty::ratatoist`], with `config_toml` already in the config folder.
    fn ratatoist_with_config(config_toml: Option<&str>) -> Self {
        Self::spawn_in(
            Path::new("/bin/bash"),
            &[
                "--norc",
                "--noprofile",
                "-i",
                "-c",
                "exec \"$RATATOIST\" --demo",
            ],
            &[("RATATOIST", env!("CARGO_BIN_EXE_ratatoist"))],
            config_toml,
        )
    }

    /// `program` with `args` and the environment variables `env`, its HOME
    /// and XDG directories fresh and empty but for an empty
    /// `$XDG_CONFIG_HOME/ratatoist`.
    fn spawn_with(program: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        Self::spawn_in(program, args, env, None)
    }

    /// [`Pty::spawn_with`], with `config_toml` written to
    /// `$XDG_CONFIG_HOME/ratatoist/config.toml` before the program starts.
    fn spawn_in(
        program: &Path,
        args: &[&str],
        env: &[(&str, &str)],
        config_toml: Option<&str>,
    ) -> Self {
        let home = TempDir::new();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.env_clear();
        command.env("PATH", std::env::var_os("PATH").unwrap_or_default());
        command.env("TERM", "xterm-256color");
        command.env("HOME", &home.0);
        for (var, value) in env {
            command.env(var, value);
        }
        for (var, dir) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_STATE_HOME", "state"),
        ] {
            let path = home.0.join(dir);
            fs::create_dir(&path).unwrap();
            command.env(var, path);
        }
        // As on a machine that has run ratatoist before, so a stray write to
        // the config folder lands and the file checks see it.
        fs::create_dir(home.0.join("config").join("ratatoist")).unwrap();
        if let Some(text) = config_toml {
            fs::write(home.0.join("config/ratatoist/config.toml"), text).unwrap();
        }
        let child = pair.slave.spawn_command(command).unwrap();

        let output = Arc::new(Mutex::new(Vec::new()));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let drained = Arc::new(AtomicBool::new(false));
        let sink = Arc::clone(&output);
        let done = Arc::clone(&drained);
        thread::spawn(move || {
            let mut buffer = [0; 4096];
            while let Ok(read @ 1..) = reader.read(&mut buffer) {
                sink.lock().unwrap().extend_from_slice(&buffer[..read]);
            }
            done.store(true, Ordering::SeqCst);
        });
        let writer = pair.master.take_writer().unwrap();
        Self {
            master: pair.master,
            _slave: pair.slave,
            child,
            writer,
            output,
            drained,
            home,
        }
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }

    fn press(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).unwrap();
        self.writer.flush().unwrap();
    }

    fn signal(&self, name: &str) {
        let pid = self.child.process_id().unwrap();
        let sent = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(pid.to_string())
            .status()
            .unwrap();
        assert!(sent.success(), "kill -{name} failed");
    }

    /// Whether the terminal is cooked: line editing and echo on, as a shell
    /// prompt needs.
    fn cooked(&self) -> bool {
        let termios = self.master.get_termios().unwrap();
        let flags = u64::from(termios.local_flags.bits());
        let wanted = u64::from((LocalModes::ICANON | LocalModes::ECHO).bits());
        flags & wanted == wanted
    }

    fn exited(&mut self) -> bool {
        self.child.try_wait().unwrap().is_some()
    }

    /// Waits until `ready` holds, failing the test with `what` and the
    /// output so far if it does not in time.
    fn wait_until(&mut self, what: &str, mut ready: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + PATIENCE;
        while !ready(self) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting until {what}; output so far: {:?}",
                self.output()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits for ratatoist to take the terminal: raw mode on the alternate
    /// screen.
    fn wait_for_takeover(&mut self) {
        self.wait_until("ratatoist takes the terminal", |pty| {
            pty.output().contains(ENTER_ALTERNATE_SCREEN) && !pty.cooked()
        });
    }

    /// Whether the last alternate-screen sequence in the output so far
    /// leaves it.
    fn left_alternate_screen(&self) -> bool {
        let output = self.output();
        output.rfind(LEAVE_ALTERNATE_SCREEN) > output.rfind(ENTER_ALTERNATE_SCREEN)
    }

    /// Asserts the terminal is cooked and back on the main screen. The
    /// program has exited, but the reader thread may not have read its last
    /// bytes yet, so this waits for the leave sequence or the end of the
    /// output before judging.
    fn assert_restored(&mut self) {
        assert!(self.cooked(), "the terminal was left in raw mode");
        self.wait_until("the output is read to the end", |pty| {
            pty.left_alternate_screen() || pty.drained.load(Ordering::SeqCst)
        });
        assert!(
            self.left_alternate_screen(),
            "the alternate screen was not left: {:?}",
            self.output()
        );
    }

    /// Asserts the only file ratatoist left under HOME is one day's log in
    /// `$XDG_STATE_HOME/ratatoist/`.
    fn assert_only_a_log_in_the_state_dir(&self) {
        let files = files_under(&self.home.0);
        let logs = self.home.0.join("state").join("ratatoist");
        assert_eq!(files.len(), 1, "{files:?}");
        let name = files[0]
            .strip_prefix(&logs)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            name.starts_with("ratatoist.") && name.ends_with(".log"),
            "{files:?}"
        );
    }

    /// Asserts no shell startup file exists in the program's HOME.
    fn assert_no_shell_rc_files(&self) {
        for name in SHELL_RC_FILES {
            assert!(
                !self.home.0.join(name).exists(),
                "{name} was created in HOME"
            );
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // A failed test must not leave a ratatoist behind.
        if !self.exited() {
            let _ = self.child.kill();
        }
    }
}

/// Every file under `dir`, at any depth.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

/// A fresh directory under the system temp directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("ratatoist-pty-{}-{n}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn quitting_leaves_the_terminal_cooked_on_the_main_screen() {
    let mut pty = Pty::ratatoist();
    pty.wait_for_takeover();

    pty.press(b"q");

    pty.wait_until("ratatoist exits", Pty::exited);
    pty.assert_restored();
    pty.assert_no_shell_rc_files();
    pty.assert_only_a_log_in_the_state_dir();
}

#[test]
fn the_demo_never_reads_the_config_file() {
    let mut pty = Pty::ratatoist_with_config(Some("not_a_key = 1\n[theme]\nname = \"dracula\"\n"));
    pty.wait_for_takeover();

    pty.press(b"q");

    pty.wait_until("ratatoist exits", Pty::exited);
    assert!(
        !pty.output().contains("config.toml"),
        "the demo reported on config.toml, so it read it: {}",
        pty.output()
    );
}

#[test]
fn sigterm_leaves_the_terminal_cooked_on_the_main_screen() {
    let mut pty = Pty::ratatoist();
    pty.wait_for_takeover();

    pty.signal("TERM");

    pty.wait_until("ratatoist exits", Pty::exited);
    pty.assert_restored();
}

#[test]
fn sighup_leaves_the_terminal_cooked_on_the_main_screen() {
    let mut pty = Pty::ratatoist();
    pty.wait_for_takeover();

    pty.signal("HUP");

    pty.wait_until("ratatoist exits", Pty::exited);
    pty.assert_restored();
}

/// Set to make [`panics_inside_the_guard_when_asked`] panic.
const FORCE_PANIC: &str = "RATATOIST_PTY_FORCE_PANIC";
const PANIC_MESSAGE: &str = "forced panic to check that the terminal is restored";

/// Passes at once unless [`FORCE_PANIC`] is set, when it panics with the
/// terminal guard held, as `examples/force_panic.rs` does. The panic test
/// runs it in a pseudo-terminal from this same test binary, so it is never
/// older than the code it checks.
#[test]
fn panics_inside_the_guard_when_asked() {
    if std::env::var_os(FORCE_PANIC).is_none() {
        return;
    }
    let _guard = ratatoist::terminal::Guard::enter().unwrap();
    panic!("{PANIC_MESSAGE}");
}

#[test]
fn a_panic_restores_the_terminal_before_its_message_prints() {
    let this_test_binary = std::env::current_exe().unwrap();
    let mut pty = Pty::spawn_with(
        &this_test_binary,
        &[
            "panics_inside_the_guard_when_asked",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ],
        &[(FORCE_PANIC, "1")],
    );

    pty.wait_until("the panic message prints", |pty| {
        pty.exited() && pty.output().contains(PANIC_MESSAGE)
    });

    pty.assert_restored();
    let output = pty.output();
    assert!(
        output.rfind(LEAVE_ALTERNATE_SCREEN) < output.find(PANIC_MESSAGE),
        "the panic message printed on the alternate screen: {output:?}"
    );
}

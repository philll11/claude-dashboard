//! The dashboard in the terminal, run as a person runs it: `claude-dashboard` without a command,
//! in a pseudo-terminal (Windows' pseudo console on Windows), with the screen read as a terminal
//! shows it.

mod common;

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use common::{Fixture, NOW, request};

/// How long a step may take before the test gives up on it.
const WAIT: Duration = Duration::from_secs(60);

/// A terminal's question for where its cursor is, which Windows' pseudo console asks as it starts.
const CURSOR_QUESTION: &[u8] = b"\x1b[6n";

struct Terminal {
    master: Box<dyn MasterPty + Send>,
    input: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    child: Box<dyn Child + Send + Sync>,
    screen: vt100::Parser,
    /// What arrived and has not been searched for a cursor question yet, in case one is split.
    tail: Vec<u8>,
}

impl Terminal {
    /// Runs the dashboard over `fixture` in a terminal of `cols` by `rows`.
    fn open(fixture: &Fixture, (cols, rows): (u16, u16)) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("a pseudo-terminal opens");
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_claude-dashboard"));
        command.cwd(fixture.dir.path());
        for (name, value) in [
            ("CLAUDE_CONFIG_DIR", fixture.root().into_os_string()),
            (
                "CLAUDE_DASHBOARD_HISTORY",
                fixture.history().into_os_string(),
            ),
            ("CLAUDE_DASHBOARD_NOW", NOW.into()),
            // The background is named, so the terminal is never asked about it.
            ("CLAUDE_DASHBOARD_THEME", "dark".into()),
            ("TZ", "UTC".into()),
            ("TERM", "xterm-256color".into()),
            ("COLORTERM", "truecolor".into()),
        ] {
            command.env(name, value);
        }
        command.env_remove("NO_COLOR");
        let child = pair
            .slave
            .spawn_command(command)
            .expect("the program starts");
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("the terminal can be read");
        let input = pair
            .master
            .take_writer()
            .expect("the terminal can be written to");
        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0; 8192];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 || sender.send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
        });
        Self {
            master: pair.master,
            input,
            output,
            child,
            screen: vt100::Parser::new(rows, cols, 0),
            tail: Vec::new(),
        }
    }

    fn take(&mut self, bytes: &[u8]) {
        self.screen.process(bytes);
        self.tail.extend_from_slice(bytes);
        while let Some(at) = self
            .tail
            .windows(CURSOR_QUESTION.len())
            .position(|window| window == CURSOR_QUESTION)
        {
            self.press("\x1b[1;1R");
            self.tail.drain(..at + CURSOR_QUESTION.len());
        }
        let keep = self.tail.len().saturating_sub(CURSOR_QUESTION.len());
        self.tail.drain(..keep);
    }

    fn contents(&self) -> String {
        self.screen.screen().contents()
    }

    /// Waits until the screen shows `text`, failing with what it shows instead.
    fn wait_for(&mut self, text: &str) {
        let deadline = Instant::now() + WAIT;
        loop {
            if self.contents().contains(text) {
                return;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if let Ok(bytes) = self.output.recv_timeout(left) {
                self.take(&bytes);
            } else {
                let _ = self.child.kill();
                panic!(
                    "the screen did not show {text:?} in time; it shows:\n{}",
                    self.contents()
                );
            }
        }
    }

    fn press(&mut self, keys: &str) {
        self.input
            .write_all(keys.as_bytes())
            .and_then(|()| self.input.flush())
            .expect("keys reach the program");
    }

    fn resize(&mut self, (cols, rows): (u16, u16)) {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("the terminal resizes");
        self.screen.screen_mut().set_size(rows, cols);
    }

    /// Waits for the program to end, reading what it writes meanwhile, and returns whether it
    /// succeeded.
    fn ended(&mut self) -> bool {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().expect("the program can be waited on") {
                while let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(200)) {
                    self.take(&bytes);
                }
                return status.success();
            }
            assert!(Instant::now() < deadline, "the program did not end in time");
            if let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(50)) {
                self.take(&bytes);
            }
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn history() -> Fixture {
    let fixture = Fixture::new();
    fixture.transcript(
        "4f1c2a3b-0000-4000-8000-000000000001",
        &[
            serde_json::json!({"type": "ai-title", "aiTitle": "Fix the importer"}),
            request("1", "claude-opus-5-5", "2026-10-09T10:00:00Z"),
            request("2", "claude-sonnet-5-5", "2026-10-10T09:00:00Z"),
            request("3", "claude-opus-5-5", "2026-10-10T09:30:00Z"),
        ],
    );
    fixture
}

#[test]
fn the_dashboard_opens_moves_between_screens_and_gives_the_terminal_back() {
    let fixture = history();
    let mut terminal = Terminal::open(&fixture, (120, 36));

    terminal.wait_for("Cost per day by model · 30 days");
    terminal.wait_for("Opus 5.5");
    terminal.wait_for("Read just now");
    assert!(terminal.screen.screen().alternate_screen());

    terminal.press("2");
    terminal.wait_for("Sessions · 30 days");
    terminal.wait_for("Fix the importer");
    terminal.press("\r");
    terminal.wait_for("Cost over the session");
    terminal.wait_for("Sessions › Fix the importer");
    terminal.press("\x1b");
    terminal.wait_for("/ searches titles and projects");

    terminal.press("?");
    terminal.wait_for("read new transcripts now");
    terminal.press("\x1b");
    terminal.wait_for("Session ");

    terminal.resize((80, 24));
    terminal.press("1");
    terminal.wait_for("Cost, API-equivalent");
    terminal.resize((72, 20));
    terminal.wait_for("The window is 72 by 20.");
    terminal.resize((80, 24));
    terminal.wait_for("Cost, API-equivalent");

    terminal.press("q");
    assert!(terminal.ended(), "the dashboard ends successfully");
    let screen = terminal.screen.screen();
    assert!(
        !screen.alternate_screen(),
        "the terminal's own screen is back"
    );
    assert!(!screen.hide_cursor(), "the cursor shows again");
}

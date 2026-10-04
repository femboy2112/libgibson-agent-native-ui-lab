//! End-to-end: the real binary, in a real pseudo-terminal, driven with real key
//! presses, read back through a terminal emulator (vt100).

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Term {
    parser: vt100::Parser,
    rx: mpsc::Receiver<Vec<u8>>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    rows: u16,
    cols: u16,
}

impl Term {
    fn spawn(args: &[&str], rows: u16, cols: u16) -> Term {
        let pty = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .expect("openpty");
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_project-pulsar"));
        for a in args {
            cmd.arg(a);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let child = pty.slave.spawn_command(cmd).expect("spawn");
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader().expect("reader");
        let writer = pty.master.take_writer().expect("writer");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        Term {
            parser: vt100::Parser::new(rows, cols, 0),
            rx,
            writer,
            child,
            master: pty.master,
            rows,
            cols,
        }
    }

    fn pump(&mut self, dur: Duration) {
        let end = Instant::now() + dur;
        while Instant::now() < end {
            match self.rx.recv_timeout(Duration::from_millis(20)) {
                Ok(b) => self.parser.process(&b),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .expect("resize");
        self.rows = rows;
        self.cols = cols;
        self.parser.screen_mut().set_size(rows, cols);
    }

    fn screen(&self) -> String {
        self.parser.screen().contents()
    }

    fn wait_for(&mut self, needle: &str, secs: u64) -> bool {
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            self.pump(Duration::from_millis(100));
            if self.screen().contains(needle) {
                return true;
            }
        }
        false
    }

    fn send(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).expect("write");
        self.writer.flush().ok();
    }
}

#[test]
fn interactive_session_responds_to_keys_and_quits_cleanly() {
    let mut t = Term::spawn(&["--seed", "62", "--paused"], 24, 80);
    assert!(t.wait_for("PULSAR", 30), "no first frame:\n{}", t.screen());
    assert!(t.screen().contains("STREAM"));
    // switch to the sky view
    t.send("5");
    assert!(t.wait_for("SKY", 10), "view did not switch:\n{}", t.screen());
    // direct seek via the goto prompt
    t.send("g");
    assert!(t.wait_for("goto t", 10), "no goto prompt:\n{}", t.screen());
    t.send("120\r");
    assert!(t.wait_for("t  120.0s", 20), "seek failed:\n{}", t.screen());
    // step one second
    t.send(".");
    assert!(t.wait_for("t  121.0s", 10), "step failed:\n{}", t.screen());
    // select BETA, the rail marker follows
    t.send("b");
    t.pump(Duration::from_millis(300));
    assert!(t.screen().contains("▶●"), "selection marker missing:\n{}", t.screen());
    // quit
    t.send("q");
    let end = Instant::now() + Duration::from_secs(10);
    let mut status = None;
    while Instant::now() < end {
        t.pump(Duration::from_millis(50));
        if let Ok(Some(s)) = t.child.try_wait() {
            status = Some(s);
            break;
        }
    }
    let status = status.expect("process did not exit after q");
    assert!(status.success(), "exit status {status:?}");
}

#[test]
fn resize_is_followed_live_and_the_layout_adapts() {
    let mut t = Term::spawn(&["--seed", "62", "--at", "200", "--view", "sky"], 40, 120);
    assert!(t.wait_for("INTERFERENCE", 30), "wide layout has the rail:\n{}", t.screen());
    t.resize(15, 42);
    // narrow layout: the rail is replaced by the one-row identity strip
    assert!(t.wait_for("◆A", 10), "no strip after resize:\n{}", t.screen());
    assert!(!t.screen().contains("INTERFERENCE"));
    t.resize(24, 80);
    assert!(t.wait_for("INTERFERENCE", 10), "rail back at 80 columns:\n{}", t.screen());
    t.send("q");
    t.pump(Duration::from_millis(500));
}

#[test]
fn help_overlay_event_jump_and_selection_keys() {
    let mut t = Term::spawn(&["--seed", "62", "--at", "200", "--view", "spectrum"], 30, 100);
    assert!(t.wait_for("SPECTRUM", 30), "{}", t.screen());
    t.send("?");
    assert!(t.wait_for("KEYS", 10), "help did not open:\n{}", t.screen());
    t.send("\x1b"); // Esc dismisses the modal
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end && t.screen().contains("KEYS") {
        t.pump(Duration::from_millis(100));
    }
    assert!(!t.screen().contains("KEYS"), "help did not close:\n{}", t.screen());
    // n / N jump between analysis events (uppercase must arrive as 'N')
    t.send("n");
    assert!(t.wait_for("t  248.0s", 10), "next event (ALPHA resolved at 248 s):\n{}", t.screen());
    t.send("N");
    assert!(t.wait_for("t  192.0s", 10), "previous event (GAMMA lock at 192 s):\n{}", t.screen());
    // Tab cycles the selected identity: ALPHA -> BETA
    t.send("\t");
    t.pump(Duration::from_millis(300));
    assert!(t.screen().contains("▶●"), "Tab did not select BETA:\n{}", t.screen());
    // inspect cursor shows a readout
    t.send("i");
    t.send("l");
    assert!(t.wait_for("┼", 10), "{}", t.screen());
    // compare mode
    t.send("m");
    assert!(t.wait_for("COMPARE", 10), "{}", t.screen());
    t.send("q");
    t.pump(Duration::from_millis(500));
}

#[test]
fn a_bare_launch_is_a_film_that_starts_playing() {
    let mut t = Term::spawn(&["--seed", "62"], 24, 80);
    assert!(t.wait_for("▶ ×16", 30), "should start playing at x16:\n{}", t.screen());
    // time advances by itself, and keeps going until we quit
    assert!(t.wait_for("t   ", 5));
    let before = t.screen();
    t.pump(Duration::from_millis(1500));
    assert_ne!(before, t.screen(), "playback did not advance");
    t.send("q");
    t.pump(Duration::from_millis(500));
}

#[test]
fn live_demo_runs_to_completion_on_a_real_terminal_and_exits() {
    let mut t = Term::spawn(&["--seed", "62", "--demo", "--live", "--hold-ms", "150"], 24, 80);
    assert!(t.wait_for("PULSAR", 30), "{}", t.screen());
    // it is bounded: 12 steps x 150 ms, then the process ends by itself
    let end = Instant::now() + Duration::from_secs(40);
    let mut status = None;
    let mut saw_dossier = false;
    while Instant::now() < end {
        t.pump(Duration::from_millis(50));
        saw_dossier |= t.screen().contains("INTERPRETATION");
        if let Ok(Some(s)) = t.child.try_wait() {
            status = Some(s);
            break;
        }
    }
    let status = status.expect("the live demo did not terminate on its own");
    assert!(status.success());
    assert!(saw_dossier, "the cue sheet reaches the dossier step");
}

#[test]
fn a_recorded_interactive_session_replays_to_the_same_screen() {
    let log = std::env::temp_dir().join(format!("pulsar-replay-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let mut t = Term::spawn(&["--seed", "62", "--paused", "--record", log.to_str().unwrap()], 24, 80);
    assert!(t.wait_for("PULSAR", 30), "{}", t.screen());
    // a session with a bit of everything
    for k in ["5", "g", "2", "0", "0", "\r", "b", "f", "+", ".", ".", "m", "i", "l", "l", "j"] {
        t.send(k);
        t.pump(Duration::from_millis(60));
    }
    t.pump(Duration::from_millis(600)); // let the dissolve settle
    let live: Vec<String> = t
        .screen()
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    assert!(live.iter().any(|l| l.contains("COMPARE")), "{}", t.screen());
    t.send("q");
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        t.pump(Duration::from_millis(50));
        if let Ok(Some(_)) = t.child.try_wait() {
            break;
        }
    }
    let text = std::fs::read_to_string(&log).expect("record file written on exit");
    assert!(text.contains("goto-char 2") && text.contains("view sky"), "{text}");
    // replay it headless through the same binary
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_project-pulsar"))
        .args(["--seed", "62", "--replay", log.to_str().unwrap(), "--size", "80x24"])
        .output()
        .expect("replay runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let replay: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let norm = |v: &[String]| v.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n").trim_end().to_string();
    assert_eq!(norm(&live), norm(&replay), "the replayed frame differs from the live screen");
    let _ = std::fs::remove_file(&log);
}

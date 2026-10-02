//! Real-PTY smoke and stress harness for Project Theseus.
//!
//! Drives the *actual* binary over a pseudo-terminal (not a nested `cargo run`),
//! with `--no-audio`, and exercises navigation, fidelity edits, regeneration,
//! resize, the help overlay, rapid interaction, a clean quit, and a
//! terminal-restoration proxy (the alternate-screen enter/leave sequences).
//!
//! ## Harness rules learned the hard way
//! * The child must run with `CARGO_MANIFEST_DIR` as its CWD (the binary loads
//!   `fixtures/…` relatively).
//! * The output drain thread must retry `ErrorKind::Interrupted`; `resize`
//!   delivers `SIGWINCH` and an interrupted `read` must not kill the drain or
//!   the PTY buffer fills and the app blocks on write.

use portable_pty::{Child, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_project-theseus")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/debug/project-theseus"))
}

struct Session {
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    out: Arc<Mutex<Vec<u8>>>,
}

impl Session {
    fn spawn(cols: u16, rows: u16) -> Session {
        let pty = NativePtySystem::default();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(bin());
        cmd.arg("--no-audio");
        cmd.cwd(env!("CARGO_MANIFEST_DIR"));
        let child = pair.slave.spawn_command(cmd).expect("spawn project-theseus");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("clone reader");
        let writer = pair.master.take_writer().expect("take writer");
        let out = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = Arc::clone(&out);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => sink.lock().unwrap().extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });

        Session {
            child,
            writer,
            master: pair.master,
            out,
        }
    }

    fn send(&mut self, keys: &str) {
        self.writer.write_all(keys.as_bytes()).expect("write keys");
        self.writer.flush().expect("flush");
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.out.lock().unwrap()).into_owned()
    }

    /// Wait up to `budget` for the child to exit; `None` means still running.
    fn wait_exit(&mut self, budget: Duration) -> Option<portable_pty::ExitStatus> {
        let start = Instant::now();
        while start.elapsed() < budget {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => return None,
            }
        }
        None
    }

    fn quit_or_fail(&mut self, label: &str, budget: Duration) -> String {
        let status = self.wait_exit(budget);
        if status.is_none() {
            let tail = {
                let out = self.output();
                out.chars().rev().take(1200).collect::<String>().chars().rev().collect::<String>()
            };
            self.kill();
            panic!("{label}: no clean exit within {budget:?}; output tail:\n{tail}");
        }
        status.unwrap();
        self.output()
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

fn settle(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn assert_restored(out: &str) {
    assert!(
        out.contains("\u{1b}[?1049h") || out.contains("\u{1b}[?47h"),
        "expected an alternate-screen enter sequence in the output"
    );
    assert!(
        out.contains("\u{1b}[?1049l") || out.contains("\u{1b}[?47l"),
        "expected an alternate-screen leave sequence (terminal restoration proxy)"
    );
    assert!(
        out.contains("THESEUS") || out.contains("QUOTIENT"),
        "expected the application chrome in the output"
    );
}

#[test]
fn pty_navigation_resize_help_and_clean_quit() {
    let mut s = Session::spawn(80, 24);
    settle(700);

    // Navigation + all eight axes + fidelity regime cycling + world + seed.
    s.send("12345678");
    settle(250);
    s.send("fff");
    settle(250);
    s.send("wws");
    settle(250);

    // Resize mid-session (no input backlog), then keep interacting.
    s.resize(140, 44);
    settle(400);
    s.send("1d2");
    settle(250);

    // Open the help overlay, close it with Esc (Esc closes help when open).
    s.send("?");
    settle(250);
    s.send("\u{1b}");
    settle(250);

    // Clean quit.
    s.send("q");
    let out = s.quit_or_fail("navigation", Duration::from_secs(30));
    assert_restored(&out);
}

#[test]
fn pty_rapid_interaction_stress() {
    let mut s = Session::spawn(70, 22);
    settle(700);

    // Fire long bursts of mixed interactions faster than the frame rate. Every
    // byte lands in the PTY buffer at once, so the app must coalesce a large
    // backlog without panicking or corrupting its state. The help toggle is
    // deliberately excluded so the volume is pure interaction.
    for _ in 0..3 {
        s.send("12345678dffwwss1d2");
    }
    settle(400);
    for _ in 0..2 {
        s.send("ffffwwww1234ddrrr");
    }
    settle(400);

    s.send("q");
    let out = s.quit_or_fail("stress", Duration::from_secs(60));
    assert_restored(&out);
}

/// Regression guard for the upstream defect reproduced by this harness:
/// resizing the PTY while a burst of input is still queued causes the queued
/// keystrokes (including `q`) to be lost, so the app can no longer be quit from
/// the keyboard. It is the same mechanism as
/// `femboy2112/libgibson` issue #15 ("queued input after resize under graphical
/// PTY backpressure"). Ignored by default because it reproduces the bug; run
/// with `cargo test --test pty_smoke -- --ignored` to see it fail.
#[test]
#[ignore = "reproduces femboy2112/libgibson#15: queued input lost after resize"]
fn pty_resize_under_input_backpressure_regression() {
    let mut s = Session::spawn(70, 22);
    settle(700);

    for _ in 0..3 {
        s.send("12345678dffwwss1d2");
    }
    settle(300);
    // Resize with the backlog still draining.
    s.resize(48, 16);
    settle(150);
    for _ in 0..2 {
        s.send("ffffwwww1234ddrrr");
    }
    settle(300);

    s.send("q");
    let out = s.quit_or_fail("resize-under-backpressure", Duration::from_secs(20));
    assert_restored(&out);
}

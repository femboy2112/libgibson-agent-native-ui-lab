//! The real binary in a real PTY: keys in, a VT emulator on the other side, and the terminal
//! state checked after exit. (Linux x86_64 only; no tmux/screen/SSH/other terminals.)

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());

struct Session {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    parser: Arc<Mutex<vt100::Parser>>,
    log: Arc<Mutex<Vec<u8>>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // a failing assertion must not leak the child process
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Session {
    fn start(cols: u16, rows: u16, args: &[&str]) -> Session {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_project-chronoscope"));
        cmd.args(args);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("CHRONO_AUDIO", "off");
        if let Ok(t) = std::env::var("CHRONO_TRACE_DIR") {
            cmd.env(
                "CHRONO_TRACE",
                format!(
                    "{t}/trace-{}.log",
                    std::process::id() as u64 * 1000
                        + std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .subsec_millis() as u64
                ),
            );
        }
        let child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().expect("reader");
        let writer = pair.master.take_writer().expect("writer");
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let log = Arc::new(Mutex::new(Vec::new()));
        let (p2, l2) = (parser.clone(), log.clone());
        let t = std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                p2.lock().unwrap().process(&buf[..n]);
                l2.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        Session {
            master: pair.master,
            writer,
            child,
            parser,
            log,
            reader: Some(t),
        }
    }

    fn send(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }

    fn screen(&self) -> String {
        self.parser.lock().unwrap().screen().contents()
    }

    fn wait_for(&self, what: &str, timeout: Duration) -> String {
        let t0 = Instant::now();
        loop {
            let s = self.screen();
            if s.contains(what) {
                return s;
            }
            if t0.elapsed() > timeout {
                panic!("timed out waiting for {what:?}; screen:\n{s}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        // The emulator must learn the new size *before* the application can react to SIGWINCH,
        // otherwise bytes drawn for the new geometry are interpreted at the old one.
        self.parser
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(rows, cols);
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
    }

    fn wait_exit(&mut self, timeout: Duration) -> Option<u32> {
        let t0 = Instant::now();
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                return Some(st.exit_code());
            }
            if t0.elapsed() > timeout {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn finish(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(t) = self.reader.take() {
            let _ = t.join();
        }
    }
}

fn head(s: &Session) -> String {
    s.screen()
        .lines()
        .next()
        .unwrap_or("")
        .trim_end()
        .to_string()
}

fn pos_of(screen: &str) -> Option<u32> {
    // header looks like "◷ CHRONOSCOPE  A  200/344 · ..."
    let line = screen.lines().next()?;
    let tok = line.split_whitespace().find(|t| {
        t.contains('/')
            && t.chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
    })?;
    tok.split('/').next()?.parse().ok()
}

fn restored(s: &Session) -> (bool, bool) {
    let p = s.parser.lock().unwrap();
    (!p.screen().alternate_screen(), !p.screen().hide_cursor())
}

#[test]
fn full_session_drives_step_scrub_fork_switch_resize_modal_rapid_input_and_quit() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = Session::start(100, 32, &["--no-audio"]);
    let screen = s.wait_for("CHRONOSCOPE", Duration::from_secs(10));
    assert!(
        screen.contains("HISTORIES"),
        "wide terminal shows the side panel:\n{screen}"
    );
    assert!(
        s.parser.lock().unwrap().screen().alternate_screen(),
        "fullscreen uses the alternate screen"
    );

    // step / scrub
    s.send("...");
    s.wait_for(" 3/344", Duration::from_secs(5));
    s.send(",");
    s.wait_for(" 2/344", Duration::from_secs(5));
    s.send("\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C"); // Right ×5
    s.wait_for(" 7/344", Duration::from_secs(5));
    s.send("\x1b[6~"); // PageDown +32
    s.wait_for(" 39/344", Duration::from_secs(5));
    s.send("n"); // next landmark
    let after_n = {
        let t0 = Instant::now();
        loop {
            let scr = s.screen();
            if pos_of(&scr).map(|p| p > 39).unwrap_or(false)
                || t0.elapsed() > Duration::from_secs(5)
            {
                break scr;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    assert!(
        pos_of(&after_n).unwrap() > 39,
        "n jumps to a later landmark: {}",
        after_n.lines().next().unwrap()
    );
    // jump to the catastrophe and look at it
    s.send("c");
    let cat = s.wait_for("344/344", Duration::from_secs(5));
    assert!(
        cat.contains("CATASTROPHE"),
        "the end of the demo history is the meltdown:\n{cat}"
    );

    // rewind far, fork at the earlier decision with the keyboard
    s.send("0");
    s.wait_for(" 0/344", Duration::from_secs(5));
    s.send("g"); // next input: stands at position 50, just before the BOOST is injected
    s.wait_for(" 50/344", Duration::from_secs(5));
    s.send("f");
    s.wait_for("FORK", Duration::from_secs(5));
    s.send("\t\r"); // Tab to the next option, Enter
    let forked = s.wait_for("branch B born", Duration::from_secs(5));
    assert!(forked.contains("old future persists"), "{forked}");

    // switch, compare, A/B toggle without audio
    s.send("b");
    s.wait_for("HISTORIES", Duration::from_secs(5));
    s.send("v");
    s.send("."); // the compare banner is a Story reveal: invisible at t=0 of its ramp
    let cmp = s.wait_for("COMPARE", Duration::from_secs(5));
    eprintln!("TRACE after compare: {}", head(&s));
    assert!(
        cmp.contains("⇄"),
        "the header names the compared pair:\n{cmp}"
    );

    // resize while comparing: 100x32 -> 60x20 -> 160x50 -> 42x15
    for (c, r) in [(60u16, 20u16), (160, 50), (42, 15)] {
        s.resize(c, r);
        std::thread::sleep(Duration::from_millis(300));
        s.send(","); // force a frame
        let t0 = Instant::now();
        loop {
            let scr = s.screen();
            let rows: Vec<&str> = scr.lines().collect();
            if scr.contains("CHRONOSCOPE")
                && rows.len() <= r as usize
                && rows.iter().all(|l| l.chars().count() <= c as usize)
            {
                break;
            }
            assert!(
                t0.elapsed() < Duration::from_secs(5),
                "no coherent frame after resize to {c}x{r}:\n{scr}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    eprintln!("TRACE after resizes: {}", head(&s));
    s.resize(100, 32);
    // NOTE: keys sent *immediately* after a resize can be held back by crossterm until a later key
    // releases them (LibGibson issue #15). Real users do not type within a millisecond of a
    // resize, so the harness lets the new geometry settle; `tests/pty_resize_collision.rs`
    // measures the collision on purpose.
    std::thread::sleep(Duration::from_millis(400));
    s.send("v"); // leave compare
    s.send(".");
    std::thread::sleep(Duration::from_millis(300));
    eprintln!("TRACE after v+.: {}", head(&s));

    // modal inspection
    s.send("i");
    s.wait_for("INSPECT", Duration::from_secs(5));
    // (read asynchronously: a mid-frame snapshot can hold a stale cell, so wait for the whole label)
    let insp = s.wait_for("(causal parents)", Duration::from_secs(5));
    assert!(
        insp.contains("AUDITOR") || insp.contains("REACTOR") || insp.contains("COOLER"),
        "the inspector lists causal parents:\n{insp}"
    );
    s.send("q"); // swallowed by the modal
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        s.child.try_wait().unwrap().is_none(),
        "q must not quit while a modal has focus"
    );
    s.send("\x1b"); // Esc
    std::thread::sleep(Duration::from_millis(200));

    // rapid input: 400 keystrokes in one write
    let before = pos_of(&s.screen()).unwrap();
    let burst: String = (0..200).map(|_| ".,").collect::<String>() + &".".repeat(40);
    s.send(&burst);
    std::thread::sleep(Duration::from_millis(500));
    s.send("x"); // collapse (forces a frame)
    let after = {
        let t0 = Instant::now();
        loop {
            let p = pos_of(&s.screen());
            if p == Some(before + 40) || t0.elapsed() > Duration::from_secs(5) {
                break p;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    assert_eq!(
        after,
        Some(before + 40),
        "400 rapid keys net +40 steps, none dropped or reordered"
    );

    // quit and verify terminal restoration
    s.send("q");
    let code = s.wait_exit(Duration::from_secs(5));
    assert_eq!(code, Some(0), "clean exit");
    std::thread::sleep(Duration::from_millis(200));
    let (main_screen, cursor_visible) = restored(&s);
    assert!(main_screen, "alternate screen left");
    assert!(cursor_visible, "cursor shown again");
    let log = String::from_utf8_lossy(&s.log.lock().unwrap()).to_string();
    assert!(
        log.contains("\x1b[?1049l"),
        "explicit leave-alternate-screen on the wire"
    );
    assert!(
        log.contains("\x1b[?25h"),
        "explicit show-cursor on the wire"
    );
    let final_screen = s.screen();
    assert!(
        final_screen.contains("recorded reactor-colony"),
        "the session journal is printed to the normal screen:\n{final_screen}"
    );
    assert!(final_screen.contains("fork B from A@50"), "{final_screen}");
    s.finish();
}

#[test]
fn ctrl_c_quits_and_restores_the_terminal() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = Session::start(80, 24, &["--no-audio"]);
    s.wait_for("CHRONOSCOPE", Duration::from_secs(10));
    s.send("\x03");
    assert_eq!(s.wait_exit(Duration::from_secs(5)), Some(0));
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(restored(&s), (true, true));
    s.finish();
}

#[test]
fn quit_does_not_wait_for_an_in_flight_music_render() {
    // With audio enabled (silent playback), a ~100 s performance renders in the background
    // (seconds of CPU). Quitting immediately must not block on it: OfflineRenderer cannot be
    // cancelled, so the app detaches the worker.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = Session::start(80, 24, &["--mute"]);
    s.wait_for("CHRONOSCOPE", Duration::from_secs(10));
    s.wait_for("composing", Duration::from_secs(5));
    let t0 = Instant::now();
    s.send("q");
    let code = s.wait_exit(Duration::from_secs(4));
    let took = t0.elapsed();
    assert_eq!(code, Some(0), "exited");
    assert!(
        took < Duration::from_millis(2500),
        "quit took {took:?} with a render in flight"
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(restored(&s), (true, true));
    s.finish();
}

#[test]
fn sigterm_and_sighup_restore_the_terminal_through_the_app_handler() {
    // LibGibson itself restores on explicit restore / Drop / panic, not on a signal's default
    // action (FRICTION.md). The app turns SIGTERM/SIGHUP into a normal exit.
    for (sig, want) in [(libc::SIGTERM, 143u32), (libc::SIGHUP, 129)] {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = Session::start(80, 24, &["--no-audio"]);
        s.wait_for("CHRONOSCOPE", Duration::from_secs(10));
        let pid = s.child.process_id().expect("pid") as i32;
        unsafe {
            libc::kill(pid, sig);
        }
        let code = s.wait_exit(Duration::from_secs(5));
        assert_eq!(code, Some(want), "signal {sig} exit status");
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            restored(&s),
            (true, true),
            "terminal restored after signal {sig}"
        );
        s.finish();
    }
}

#[test]
fn sigkill_cannot_be_survived_and_that_is_documented() {
    // The one boundary no userland code can cross: record it so nobody claims otherwise.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = Session::start(80, 24, &["--no-audio"]);
    s.wait_for("CHRONOSCOPE", Duration::from_secs(10));
    let pid = s.child.process_id().expect("pid") as i32;
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    assert!(s.wait_exit(Duration::from_secs(5)).is_some());
    std::thread::sleep(Duration::from_millis(150));
    let (main_screen, cursor_visible) = restored(&s);
    println!("after SIGKILL: main_screen={main_screen} cursor_visible={cursor_visible}");
    assert!(
        !main_screen,
        "SIGKILL leaves the alternate screen up (nothing could have run)"
    );
    s.finish();
}

#[test]
fn a_vanished_terminal_does_not_leave_a_spinning_orphan() {
    // `nohup`-style: SIGHUP is ignored, so the process outlives its terminal. crossterm's poll
    // then spins forever (docs/upstream/repro_hangup_spin.rs); the app's watchdog must end it.
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new("sh");
    cmd.args([
        "-c",
        "trap '' HUP; exec \"$0\" \"$@\"",
        env!("CARGO_BIN_EXE_project-chronoscope"),
        "--no-audio",
    ]);
    cmd.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    // no reader thread: a cloned master fd would keep the PTY alive and mask the hang-up
    std::thread::sleep(Duration::from_millis(1200));
    assert!(child.try_wait().unwrap().is_none(), "running");
    drop(pair.master); // the terminal window disappears
    let t0 = Instant::now();
    let mut status = None;
    while t0.elapsed() < Duration::from_secs(4) {
        if let Some(st) = child.try_wait().unwrap() {
            status = Some(st.exit_code());
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if status.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    assert_eq!(
        status,
        Some(129),
        "the watchdog ends a process whose terminal vanished (took {:?})",
        t0.elapsed()
    );
}

#[test]
fn non_tty_stdout_fails_loudly_instead_of_rendering_nothing() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_project-chronoscope"))
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not an interactive terminal"), "{err}");
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains('\x1b'),
        "no escape bytes leaked to a pipe"
    );
}

#[test]
fn headless_capture_cli_prints_a_frame_to_a_pipe() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_project-chronoscope"))
        .args([
            "--capture",
            "--size=80x24",
            "--color=mono",
            "--script=goto 120",
            "--no-audio",
        ])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("CHRONOSCOPE") && text.contains("120/344"),
        "{text}"
    );
    assert_eq!(text.lines().count(), 24);
}

//! The audio *lifecycle* in the real binary, with a fake sound server on PATH: the app must
//! compose in the background, start an external player on a real WAV when playing at 1×, stop it
//! on pause, and replace it (different audio) after a fork. No sound device is involved or claimed.

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn wait(parser: &Arc<Mutex<vt100::Parser>>, what: &str, secs: u64) -> String {
    let t0 = Instant::now();
    loop {
        let s = parser.lock().unwrap().screen().contents();
        if s.contains(what) {
            return s;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(secs),
            "timed out waiting for {what:?}:\n{s}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn pid_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

#[test]
fn compose_play_pause_fork_replace_with_a_fake_player() {
    let dir = std::env::temp_dir().join(format!("chrono-fake-audio-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let log: PathBuf = dir.join("player.log");
    // The fake `pw-play`: records pid + argv + RIFF header + size, keeps a copy of the WAV, and
    // plays "forever" (until killed).
    let script = format!(
        "#!/bin/sh\nn=$(ls {d}/copy-* 2>/dev/null | wc -l)\ncp \"$1\" {d}/copy-$n.wav\necho \"$$ $(head -c4 \"$1\") $(stat -c %s \"$1\") $(head -c12 \"$1\" | tail -c4)\" >> {l}\nexec sleep 120\n",
        d = dir.display(),
        l = log.display()
    );
    let fake = dir.join("bin/pw-play");
    std::fs::write(&fake, script).unwrap();
    std::process::Command::new("chmod")
        .args(["+x", fake.to_str().unwrap()])
        .status()
        .unwrap();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 32,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_project-chronoscope"));
    cmd.env("TERM", "xterm-256color");
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            dir.join("bin").display(),
            std::env::var("PATH").unwrap()
        ),
    );
    cmd.env_remove("CHRONO_AUDIO");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let parser = Arc::new(Mutex::new(vt100::Parser::new(32, 100, 0)));
    let p2 = parser.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            p2.lock().unwrap().process(&buf[..n]);
        }
    });
    let send = |w: &mut Box<dyn Write + Send>, s: &str| {
        w.write_all(s.as_bytes()).unwrap();
        w.flush().unwrap();
    };

    // 1. the root performance composes in the background; the header says so, then "live"
    wait(&parser, "CHRONOSCOPE", 10);
    wait(&parser, "composing", 10);
    wait(&parser, "♪ live", 30);

    // 2. play at 1×: the fake player is started on a real WAV
    send(&mut writer, " ");
    let t0 = Instant::now();
    while !log.exists() {
        assert!(t0.elapsed() < Duration::from_secs(10), "no player started");
        std::thread::sleep(Duration::from_millis(50));
    }
    let first = std::fs::read_to_string(&log).unwrap();
    let mut it = first.lines().next().unwrap().split_whitespace();
    let pid1: i32 = it.next().unwrap().parse().unwrap();
    assert_eq!(it.next(), Some("RIFF"));
    let size: u64 = it.next().unwrap().parse().unwrap();
    assert_eq!(it.next(), Some("WAVE"));
    assert!(
        size > 5_000_000,
        "the WAV holds the rest of the branch ({size} bytes)"
    );
    assert!(pid_alive(pid1));

    // 3. pause: the player is stopped (killed and reaped)
    send(&mut writer, "p");
    let t1 = Instant::now();
    while pid_alive(pid1) && t1.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    // (the fake is a shell that exec'd sleep; the app killed that pid)
    assert!(
        !pid_alive(pid1)
            || std::fs::read_to_string(format!("/proc/{pid1}/stat"))
                .map(|s| s.contains(" Z "))
                .unwrap_or(true),
        "player still running after pause"
    );

    // 4. rewind, fork one input, play the new future: a second player on *different* audio
    send(&mut writer, "0");
    send(&mut writer, "g"); // next input: position 50, before the BOOST is injected
    wait(&parser, " 50/344", 5);
    send(&mut writer, "f");
    wait(&parser, "FORK", 5);
    send(&mut writer, "\t\t\r"); // pick a replace/insert option
    wait(&parser, "branch B born", 5);
    wait(&parser, "♪ live", 40);
    send(&mut writer, " ");
    let t2 = Instant::now();
    loop {
        let n = std::fs::read_to_string(&log)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        if n >= 2 {
            break;
        }
        assert!(
            t2.elapsed() < Duration::from_secs(10),
            "no replacement player started"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let copies: Vec<Vec<u8>> = (0..2)
        .map(|i| std::fs::read(dir.join(format!("copy-{i}.wav"))).unwrap())
        .collect();
    assert_ne!(
        copies[0], copies[1],
        "the second history sounds different from the first"
    );
    assert_eq!(&copies[0][..4], b"RIFF");
    assert_eq!(&copies[1][..4], b"RIFF");
    // the replacement WAV starts at the cursor (position 50) and runs to the end of the 640-step future:
    // ≈ 590 steps × 8,182 frames × 4 bytes ≈ 19.3 MB (+ reverb tail); the first started at 0 and ended at 344
    let (n0, n1) = (copies[0].len() as f64, copies[1].len() as f64);
    assert!((18.0e6..21.5e6).contains(&n1), "replacement WAV {n1} bytes");
    assert!((10.0e6..14.0e6).contains(&n0), "first WAV {n0} bytes");

    send(&mut writer, "q");
    let t3 = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(t3.elapsed() < Duration::from_secs(5), "did not quit");
        std::thread::sleep(Duration::from_millis(25));
    }
    // quitting also stops the player
    let last_pid: i32 = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .last()
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !pid_alive(last_pid)
            || std::fs::read_to_string(format!("/proc/{last_pid}/stat"))
                .map(|s| s.contains(" Z "))
                .unwrap_or(true),
        "player outlived the app"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

type FakeRun = (
    PathBuf,
    Box<dyn portable_pty::Child + Send + Sync>,
    Arc<Mutex<vt100::Parser>>,
    Box<dyn Write + Send>,
    Box<dyn portable_pty::MasterPty + Send>,
    Reader,
);

/// Reads the PTY master through its own fd (no `try_clone_reader`: a cloned master fd keeps the
/// PTY alive and masks a hang-up). `stop()` joins the thread, after which dropping the master
/// really does make the terminal vanish.
struct Reader(
    Arc<std::sync::atomic::AtomicBool>,
    Option<std::thread::JoinHandle<()>>,
);

impl Reader {
    fn stop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(h) = self.1.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_with_fake_player(script_body: &str, extra_env: &[(&str, &str)]) -> FakeRun {
    let dir = std::env::temp_dir().join(format!(
        "chrono-fake2-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let fake = dir.join("bin/pw-play");
    std::fs::write(&fake, script_body.replace("{DIR}", dir.to_str().unwrap())).unwrap();
    std::process::Command::new("chmod")
        .args(["+x", fake.to_str().unwrap()])
        .status()
        .unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 32,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_project-chronoscope"));
    cmd.env("TERM", "xterm-256color");
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            dir.join("bin").display(),
            std::env::var("PATH").unwrap()
        ),
    );
    cmd.env_remove("CHRONO_AUDIO");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let fd = pair.master.as_raw_fd().expect("master fd");
    let writer = pair.master.take_writer().unwrap();
    let parser = Arc::new(Mutex::new(vt100::Parser::new(32, 100, 0)));
    let p2 = parser.clone();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s2 = stop.clone();
    let h = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while !s2.load(std::sync::atomic::Ordering::SeqCst) {
            let mut p = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut p, 1, 50) } <= 0 || p.revents & libc::POLLIN == 0 {
                if p.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                    break;
                }
                continue;
            }
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            p2.lock().unwrap().process(&buf[..n as usize]);
        }
    });
    (
        dir,
        child,
        parser,
        writer,
        pair.master,
        Reader(stop, Some(h)),
    )
}

#[test]
fn an_instantly_exiting_player_is_not_restarted_every_frame() {
    // A box with `pw-play` installed but no sound server: the player exits at once. The app must
    // notice, say so, and stop trying (it used to rewrite a ~20 MB WAV and spawn 30×/second).
    let (dir, mut child, parser, mut writer, _master, _reader) =
        spawn_with_fake_player("#!/bin/sh\necho start >> {DIR}/calls.log\nexit 1\n", &[]);
    wait(&parser, "composing", 10);
    wait(&parser, "♪ live", 40);
    writer.write_all(b" ").unwrap();
    writer.flush().unwrap();
    wait(&parser, "audio off", 10);
    std::thread::sleep(Duration::from_millis(1500));
    let calls = std::fs::read_to_string(dir.join("calls.log"))
        .map(|s| s.lines().count())
        .unwrap_or(0);
    assert!(calls <= 2, "the app spawned the dead player {calls} times");
    assert!(calls >= 1);
    // `m` re-arms it
    writer.write_all(b"m").unwrap();
    writer.write_all(b"m").unwrap();
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_player_dies_with_the_app_even_when_the_app_is_killed() {
    let (dir, mut child, parser, mut writer, _master, _reader) =
        spawn_with_fake_player("#!/bin/sh\necho $$ >> {DIR}/pid.log\nexec sleep 120\n", &[]);
    wait(&parser, "composing", 10);
    wait(&parser, "♪ live", 40);
    writer.write_all(b" ").unwrap();
    writer.flush().unwrap();
    let t0 = Instant::now();
    while !dir.join("pid.log").exists() {
        assert!(t0.elapsed() < Duration::from_secs(10), "no player started");
        std::thread::sleep(Duration::from_millis(50));
    }
    let pid: i32 = std::fs::read_to_string(dir.join("pid.log"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(pid_alive(pid));
    let app = child.process_id().unwrap() as i32;
    unsafe {
        libc::kill(app, libc::SIGKILL);
    }
    let _ = child.wait();
    let t1 = Instant::now();
    while pid_alive(pid)
        && !std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.contains(" Z "))
            .unwrap_or(true)
        && t1.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    let gone = !pid_alive(pid)
        || std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.contains(" Z "))
            .unwrap_or(true);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        gone,
        "PR_SET_PDEATHSIG: the player must not outlive a SIGKILLed app"
    );
}

#[test]
fn stderr_never_paints_over_the_screen_and_is_kept_in_a_file() {
    let (dir, mut child, parser, mut writer, _master, _reader) = spawn_with_fake_player(
        "#!/bin/sh\nexit 0\n",
        &[("CHRONO_STDERR_PROBE", "1"), ("CHRONO_AUDIO", "off")],
    );
    wait(&parser, "CHRONOSCOPE", 10);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !parser
            .lock()
            .unwrap()
            .screen()
            .contents()
            .contains("stderr probe"),
        "stderr leaked onto the TUI"
    );
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    let t0 = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(t0.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(25));
    }
    std::thread::sleep(Duration::from_millis(200));
    let screen = parser.lock().unwrap().screen().contents();
    assert!(
        screen.contains("bytes were written to stderr during the session"),
        "{screen}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_vanished_terminal_removes_the_wav_directory_and_the_player_goes_with_it() {
    let (dir, mut child, parser, mut writer, master, mut reader) =
        spawn_with_fake_player("#!/bin/sh\necho $$ >> {DIR}/pid.log\nexec sleep 120\n", &[]);
    wait(&parser, "composing", 10);
    wait(&parser, "♪ live", 40);
    writer.write_all(b" ").unwrap();
    writer.flush().unwrap();
    let t0 = Instant::now();
    while !dir.join("pid.log").exists() {
        assert!(t0.elapsed() < Duration::from_secs(10), "no player started");
        std::thread::sleep(Duration::from_millis(50));
    }
    let app = child.process_id().unwrap();
    let wavs = std::env::temp_dir().join(format!("chronoscope-{app}"));
    assert!(
        wavs.exists(),
        "the player's WAV directory exists while playing"
    );
    let pid: i32 = std::fs::read_to_string(dir.join("pid.log"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // the terminal vanishes
    drop(writer);
    reader.stop();
    drop(master);
    let t1 = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(
            t1.elapsed() < Duration::from_secs(5),
            "the app did not exit after its terminal vanished"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let t2 = Instant::now();
    while (wavs.exists() || pid_alive_not_zombie(pid)) && t2.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let (wav_left, player_left) = (wavs.exists(), pid_alive_not_zombie(pid));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&wavs);
    assert!(!wav_left, "WAV directory leaked");
    assert!(!player_left, "player outlived the app");
}

fn pid_alive_not_zombie(pid: i32) -> bool {
    pid_alive(pid)
        && !std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.contains(" Z "))
            .unwrap_or(true)
}

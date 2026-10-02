//! Evidence for LibGibson issue #15 ("queued input after resize"): how often does a key sent
//! immediately after a PTY resize get held back? Opt-in: `cargo test --release --test
//! pty_resize_collision -- --ignored --nocapture`. It asserts nothing about the rate; it prints it.

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn trial(delay_ms: u64) -> (bool, u64) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 32,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_project-chronoscope"));
    cmd.args(["--no-audio"]);
    cmd.env("TERM", "xterm-256color");
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
    let pos = |p: &Arc<Mutex<vt100::Parser>>| -> Option<u32> {
        let s = p.lock().unwrap().screen().contents();
        let line = s.lines().next()?.to_string();
        let tok = line
            .split_whitespace()
            .find(|t| {
                t.contains('/')
                    && t.chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
            })?
            .to_string();
        tok.split('/').next()?.parse().ok()
    };
    let t0 = Instant::now();
    while pos(&parser).is_none() && t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    // resize, wait `delay_ms`, then three single-step keys
    parser.lock().unwrap().screen_mut().set_size(20, 60);
    pair.master
        .resize(PtySize {
            rows: 20,
            cols: 60,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(delay_ms));
    writer.write_all(b"...").unwrap();
    writer.flush().unwrap();
    let t1 = Instant::now();
    let mut got = false;
    while t1.elapsed() < Duration::from_millis(1500) {
        if pos(&parser) == Some(3) {
            got = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let latency = t1.elapsed().as_millis() as u64;
    let _ = child.kill();
    let _ = child.wait();
    (got, latency)
}

#[test]
#[ignore = "measurement, probabilistic; see LibGibson issue #15"]
fn how_often_does_a_key_right_after_a_resize_get_held_back() {
    for delay in [0u64, 5, 50, 300] {
        let mut ok = 0;
        let mut lat = vec![];
        let n = 12;
        for _ in 0..n {
            let (g, l) = trial(delay);
            ok += g as u32;
            lat.push(l);
        }
        lat.sort();
        println!("resize→keys gap {delay:>3} ms: {ok}/{n} trials applied all 3 keys within 1.5 s; latency p50 {} ms max {} ms", lat[lat.len() / 2], lat.last().unwrap());
    }
}

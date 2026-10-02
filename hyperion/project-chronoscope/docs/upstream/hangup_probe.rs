use std::time::Duration;
use std::io::Write;

// Variant selected by argv[1]: "poll" = crossterm poll only; "render" = gibson render only;
// "write" = plain stdout writes.
fn main() -> std::io::Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_default();
    std::thread::spawn(|| { let mut f = std::fs::OpenOptions::new().create(true).append(true).open(std::env::var("PROBE_LOG").unwrap()).unwrap(); loop { std::thread::sleep(Duration::from_millis(250)); for fd in [0, 1, 2] { let mut p = libc::pollfd { fd, events: 0, revents: 0 }; let r = unsafe { libc::poll(&mut p, 1, 0) }; let _ = writeln!(f, "wd fd{fd}: r={r} revents={:#x} (HUP={:#x} ERR={:#x} NVAL={:#x})", p.revents, libc::POLLHUP, libc::POLLERR, libc::POLLNVAL); } } });
    let log = |m: &str| { let mut f = std::fs::OpenOptions::new().create(true).append(true).open(std::env::var("PROBE_LOG").unwrap()).unwrap(); let _ = writeln!(f, "{m}"); };
    match mode.as_str() {
        "poll" => {
            crossterm::terminal::enable_raw_mode()?;
            let mut n = 0u64;
            loop {
                log(&format!("poll #{n}"));
                n += 1;
                let r = crossterm::event::poll(Duration::from_millis(100));
                log(&format!("poll returned {:?}", r.as_ref().map(|b| *b).map_err(|e| e.kind())));
                if let Ok(true) = r {
                    let e = crossterm::event::read();
                    log(&format!("read returned {:?}", e.as_ref().map(|_| ()).map_err(|e| e.kind())));
                    if e.is_err() { break; }
                }
                if r.is_err() { break; }
                if n > 5_000_000 { break; }
            }
        }
        "render" => {
            use gibson::context::{Context, RenderMode};
            use gibson::node::Node;
            let mut ctx = Context::new(RenderMode::Fullscreen)?;
            let mut n = 0u64;
            loop {
                ctx.set_root(Node::col().child(Node::text(format!("tick {n}"), gibson::Style::default())));
                n += 1;
                let r = ctx.render_now();
                if n % 1000 == 0 || r.is_err() { log(&format!("render #{n}: {:?}", r.as_ref().map(|_| ()).map_err(|e| e.kind()))); }
                if r.is_err() { break; }
                std::thread::sleep(Duration::from_millis(16));
            }
        }
        _ => {
            let mut n = 0u64;
            loop {
                let r = std::io::stdout().write_all(b"x").and_then(|_| std::io::stdout().flush());
                n += 1;
                if r.is_err() { log(&format!("write err after {n}: {:?}", r.map_err(|e| e.kind()))); break; }
                std::thread::sleep(Duration::from_millis(16));
            }
        }
    }
    log("loop exited");
    Ok(())
}

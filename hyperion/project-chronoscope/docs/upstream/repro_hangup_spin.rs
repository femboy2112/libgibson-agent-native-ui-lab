// Minimal reproduction: the canonical Context::run_once loop never returns, and burns ~65% of a
// core forever, once the controlling terminal has hung up *and the process survived SIGHUP*
// (nohup, a SIGHUP handler, a PTY test harness, ...).
//
// Build against libgibson v0.4.0, then drive it with docs/upstream/hangup_driver.py:
//     python3 hangup_driver.py ./target/release/repro_hangup_spin
use gibson::cell::Style;
use gibson::context::{Context, RenderMode};
use gibson::node::Node;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut ctx = Context::new(RenderMode::Fullscreen)?;
    if !ctx.is_interactive() {
        return Ok(());
    }
    let mut n = 0u64;
    loop {
        ctx.set_root(Node::col().child(Node::text(format!("tick {n}"), Style::default())));
        n += 1;
        // After the terminal vanishes this call never returns (observed: R state, ~65% CPU).
        if let Some(gibson::Event::Key(k)) = ctx.run_once(Duration::from_millis(100))? {
            if k.code == gibson::KeyCode::Char('q') {
                break;
            }
        }
    }
    ctx.restore()
}
// The same hang with crossterm alone (no LibGibson): see repro_hangup_crossterm_only.rs. Rendering/writing is
// NOT the problem: gibson's render and a plain stdout write both return EIO promptly and the loop exits.

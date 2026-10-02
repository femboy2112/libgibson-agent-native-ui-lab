// The same hang with crossterm 0.29 alone (no LibGibson in the loop): after the controlling
// terminal hangs up and the process survived SIGHUP, `event::poll` never blocks and never errors.
//
//     python3 hangup_driver.py ./target/release/repro_hangup_crossterm_only
//
// Observed: state R, ~65% CPU, forever. (Dependency: crossterm = "0.29", the version libgibson v0.4.0 resolves.)
use crossterm::{event, terminal};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    terminal::enable_raw_mode()?;
    loop {
        // returns immediately forever once the PTY master is gone (no error, no true)
        if event::poll(Duration::from_millis(100))? {
            if let event::Event::Key(k) = event::read()? {
                if k.code == event::KeyCode::Char('q') {
                    break;
                }
            }
        }
    }
    terminal::disable_raw_mode()
}

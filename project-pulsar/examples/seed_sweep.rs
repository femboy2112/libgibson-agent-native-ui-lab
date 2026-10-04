//! Robustness sweep: run the whole pipeline on many seeds and tabulate what it
//! recovers against the injected truth.
//!
//!   cargo run --release --example seed_sweep [N_SEEDS]
//!
//! A seed "passes" when A, B and C end LOCKED with |df| < 5e-4 Hz, D ends as
//! rejected CW interference and E never locks.

use project_pulsar::observatory::Observatory;
use project_pulsar::sim;
use project_pulsar::track::State;

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
    let mut failing = 0;
    for seed in 1..=n {
        let o = Observatory::from_raw(seed, sim::generate(seed, true));
        let last = o.timeline.epoch(sim::N_EPOCHS).expect("final epoch");
        let mut row = format!("seed {seed:3}");
        let mut ok = true;
        for s in 0..sim::SLOTS {
            let (sl, tr) = (last.slots[s], &o.truth()[s]);
            let want_ok = match s {
                0..=2 => sl.state == State::Locked && (sl.f - tr.f).abs() < 5e-4,
                3 => sl.state == State::RejectedCw,
                _ => sl.state != State::Locked,
            };
            ok &= want_ok;
            let off = sl
                .sky
                .map(|k| sim::separation_deg(k.best(), (tr.az, tr.el)))
                .filter(|_| s < 3)
                .map_or("   -".to_string(), |d| format!("{d:4.1}"));
            row += &format!(
                " | {}{} {:<9.9} {}",
                crate::letter(s),
                if want_ok { ' ' } else { '!' },
                sl.state.label(),
                off
            );
        }
        if !ok {
            failing += 1;
        }
        println!("{row}");
    }
    println!("{failing} of {n} seeds deviate from the expected outcome");
}

fn letter(s: usize) -> char {
    project_pulsar::identity::IDENT[s].letter
}

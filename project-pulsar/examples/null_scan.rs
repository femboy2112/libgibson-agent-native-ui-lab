//! Developer tool: false-alarm accounting on pure-noise receivers.
//!   cargo run --release --example null_scan -- [n_seeds]
use pulsar::analysis::{Engine, LineClass, Stage};
use pulsar::scenario::{Receiver, SigId, N_CP};
fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let (mut cand, mut lock, mut total, mut rfi, mut seeds_with_cand) = (0, 0, 0, 0, 0);
    for seed in 1..=n {
        let eng = Engine::from_receiver(Receiver::null(seed));
        let mut any = false;
        for k in 1..=N_CP {
            let cp = eng.checkpoint(k);
            for id in SigId::ALL {
                total += 1;
                match cp.signal(id).stage {
                    Stage::Candidate => { cand += 1; any = true }
                    s if s >= Stage::Locked => lock += 1,
                    _ => {}
                }
            }
        }
        let cp = eng.checkpoint(N_CP);
        rfi += cp.rejected.iter().filter(|l| l.class == LineClass::Terrestrial).count();
        if any { seeds_with_cand += 1 }
    }
    println!("{n} noise-only universes x {N_CP} checkpoints x 3 identities = {total} identity-checkpoints");
    println!("  CANDIDATE: {cand} ({:.2} %)   LOCKED or better: {lock}   terrestrial flags: {rfi}   universes with any candidate: {seeds_with_cand}/{n}", 100.0 * cand as f64 / total as f64);
}

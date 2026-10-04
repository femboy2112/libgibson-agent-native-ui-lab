//! Developer tool: score many seeds by the story they tell.
//!   cargo run --release --example seed_scan -- [from] [to]
use pulsar::analysis::{Engine, EventKind, Who, N_CP_PUB};
use pulsar::scenario::SigId;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let from: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let to: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    for seed in from..=to {
        let eng = Engine::new(seed);
        let cp = eng.checkpoint(N_CP_PUB);
        let mut s = format!("seed {seed:3}");
        for id in SigId::ALL {
            let p = cp.signal(id);
            let ev = |k: Option<usize>| k.map(|c| format!("{:3}", c * 8)).unwrap_or("  -".into());
            s += &format!(
                " | {} {:5} c{} L{} F{} R{} σ={:.3}",
                id.letter(), p.stage.short(), ev(p.cand_cp), ev(p.lock_cp), ev(p.loc_cp), ev(p.res_cp),
                p.sky.map(|k| k.sig_major).unwrap_or(f64::NAN)
            );
        }
        let rev = cp.events.iter().filter(|e| e.kind == EventKind::Revised).count();
        let terr = cp.events.iter().find(|e| e.kind == EventKind::FlagTerrestrial).map(|e| e.t());
        let tran = cp.events.iter().find(|e| e.kind == EventKind::FlagTransient).map(|e| e.t());
        let _ = Who::Line(0.0);
        s += &format!(" | revised={rev} terr={:?} trans={:?}", terr, tran);
        println!("{s}");
    }
}

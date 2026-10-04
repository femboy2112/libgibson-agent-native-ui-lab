//! Developer tool: print the analysis timeline for a seed next to the injected truth.
//!   cargo run --release --example analysis_dump -- [seed] [every]
use pulsar::analysis::Engine;
use pulsar::scenario::{SigId, CHUNK, DEFAULT_SEED, FS, N_CP};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SEED);
    let every: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
    let t0 = Instant::now();
    let eng = Engine::new(seed);
    let scn = &eng.rx.scn;
    println!("seed {seed}  build {:?}", t0.elapsed());
    println!(
        "TRUTH A f={:.4} lm=({:.2},{:.2}) | B f0={:.4} fdot={:.2e} lm=({:.2},{:.2}) | C f={:.4} lm=({:.2},{:.2}) | rfi f={:.3} | ghost f={:.3} on {:.0}-{:.0}",
        scn.alpha.f, scn.alpha.lm.0, scn.alpha.lm.1, scn.beta.f0, scn.beta.fdot, scn.beta.lm.0, scn.beta.lm.1,
        scn.gamma.f, scn.gamma.lm.0, scn.gamma.lm.1, scn.rfi.f, scn.ghost.f, scn.ghost.t_on, scn.ghost.t_off
    );
    let t1 = Instant::now();
    for k in 1..=N_CP {
        let cp = eng.checkpoint(k);
        if k % every != 0 && k != N_CP {
            continue;
        }
        let t = (k * CHUNK) as f64 / FS;
        let mut line = format!("t={t:5.0}");
        for id in SigId::ALL {
            let p = cp.signal(id);
            let truth = scn.freq_at(id, t);
            let sky = p.sky.map(|s| format!("({:+.2},{:+.2}) s={:.3}", s.lm.0, s.lm.1, s.sig_major)).unwrap_or("-".into());
            line += &format!(
                " | {} {:5} lf={:6.1} f={:.4}±{:.4}(tr {:.4}) {}",
                id.letter(), p.stage.short(), p.log10fap, p.f_now, p.f_sigma, truth, sky
            );
        }
        println!("{line}");
        for l in &cp.lines {
            println!("      line {} w={:.1} {:?} z={:.1} snr_end={:.1} lf={:.1}", l.label, l.w_peak, l.class, l.amp_z, l.growth.last().map(|g| g.1).unwrap_or(0.0), l.log10fap);
        }
    }
    println!("total checkpoints {:?}", t1.elapsed());
    let cp = eng.checkpoint(N_CP);
    for e in &cp.events {
        println!("event t={:.0} {:?} {:?}", e.t(), e.kind, e.who);
    }
    for id in SigId::ALL {
        let p = cp.signal(id);
        let tl = scn.lm(id);
        if let Some(s) = p.sky {
            let dl = s.lm.0 - tl.0;
            let dm = s.lm.1 - tl.1;
            // Mahalanobis
            let c = s.cov;
            let det = c[0][0] * c[1][1] - c[0][1] * c[1][0];
            let m2 = (c[1][1] * dl * dl - 2.0 * c[0][1] * dl * dm + c[0][0] * dm * dm) / det;
            println!("{} truth lm=({:.3},{:.3}) est=({:.3},{:.3}) err=({:.3},{:.3}) mahal2={:.2}", id.name(), tl.0, tl.1, s.lm.0, s.lm.1, dl, dm, m2);
        }
    }
}

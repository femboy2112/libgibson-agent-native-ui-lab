//! Developer tool: is the analysis's *stated* uncertainty honest?
//!
//! For many seeds, compare the final estimates with the injected truth in units of the
//! analysis's own 1σ. A calibrated analysis has z-scores of order 1 and ~95 % of 2-D
//! Mahalanobis² below 5.99.
//!   cargo run --release --example calibration -- [n_seeds]
use pulsar::analysis::{Engine, Stage};
use pulsar::scenario::{SigId, N_CP, T_END};

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(16);
    let seeds: Vec<u64> = (1..=n).collect();
    let results: Vec<_> = std::thread::scope(|s| {
        let hs: Vec<_> = seeds
            .chunks(seeds.len().div_ceil(6))
            .map(|chunk| {
                let chunk = chunk.to_vec();
                s.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|seed| {
                            let eng = Engine::new(seed);
                            let cp = eng.checkpoint(N_CP);
                            let mut rows = Vec::new();
                            for id in SigId::ALL {
                                let p = cp.signal(id);
                                let truth = eng.rx.scn.freq_at(id, T_END);
                                let zf = (p.f_now - truth) / p.f_sigma;
                                let m2 = p.sky.map(|sk| {
                                    let t = eng.rx.scn.lm(id);
                                    let (dl, dm) = (sk.lm.0 - t.0, sk.lm.1 - t.1);
                                    let c = sk.cov;
                                    let det = c[0][0] * c[1][1] - c[0][1] * c[1][0];
                                    (c[1][1] * dl * dl - 2.0 * c[0][1] * dl * dm + c[0][0] * dm * dm) / det
                                });
                                rows.push((seed, id, p.stage, zf, m2, p.f_now - truth, p.f_sigma));
                            }
                            rows
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).flatten().collect()
    });
    for id in SigId::ALL {
        let rows: Vec<_> = results.iter().filter(|r| r.1 == id).collect();
        let zs: Vec<f64> = rows.iter().filter(|r| r.2 >= Stage::Locked).map(|r| r.3).collect();
        let rms = (zs.iter().map(|z| z * z).sum::<f64>() / zs.len().max(1) as f64).sqrt();
        let max = zs.iter().fold(0.0f64, |a, b| a.max(b.abs()));
        let m2: Vec<f64> = rows.iter().filter(|r| r.2 >= Stage::Locked).filter_map(|r| r.4).collect();
        let inside = m2.iter().filter(|&&m| m <= 5.99).count();
        let mean_m2 = m2.iter().sum::<f64>() / m2.len().max(1) as f64;
        println!(
            "{:5}: n_locked={:2}  freq z-score rms={:.2} max|z|={:.1}   sky: mean Mahalanobis²={:.2} (ideal 2.0), inside 95% region {}/{}",
            id.name(), zs.len(), rms, max, mean_m2, inside, m2.len()
        );
    }
}

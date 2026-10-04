//! Scientific tests: the analysis is causal, calibrated, and tells the truth about
//! interference and about what it cannot know. Injected truth is consulted *only
//! here* (and in the optional truth overlay) — never by the analysis.

mod support;
use pulsar::analysis::{Engine, EventKind, LineClass, Stage, Who};
use pulsar::scenario::{delay_s, Receiver, Scenario, SigId, CHUNK, FS, N_CP, T_END};
use std::sync::Arc;
use support::SEED;

fn mahalanobis2(sk: &pulsar::analysis::coherent::SkyFit, truth: (f64, f64)) -> f64 {
    let (dl, dm) = (sk.lm.0 - truth.0, sk.lm.1 - truth.1);
    let c = sk.cov;
    let det = c[0][0] * c[1][1] - c[0][1] * c[1][0];
    (c[1][1] * dl * dl - 2.0 * c[0][1] * dl * dm + c[0][0] * dm * dm) / det
}

fn engines(seeds: &[u64]) -> Vec<Arc<Engine>> {
    std::thread::scope(|s| {
        let hs: Vec<_> = seeds
            .iter()
            .map(|&seed| s.spawn(move || Arc::new(Engine::new(seed))))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

#[test]
fn the_analysis_is_causal_future_data_cannot_change_a_checkpoint() {
    let rx = Receiver::new(SEED);
    let real = Engine::from_receiver(rx.clone());
    for k in [5usize, 17, 33] {
        let n = k * CHUNK;
        let fake = Engine::from_receiver(rx.zero_after(n));
        let a = real.checkpoint(k);
        let b = fake.checkpoint(k);
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "checkpoint {k} depends on data it should not have seen");
    }
}

#[test]
fn stated_uncertainties_are_calibrated_across_universes() {
    let seeds: Vec<u64> = (1..=8).collect();
    let mut zs = Vec::new();
    let mut m2s = Vec::new();
    for eng in engines(&seeds) {
        let cp = eng.checkpoint(N_CP);
        for id in SigId::ALL {
            let p = cp.signal(id);
            assert!(p.stage >= Stage::Locked, "seed {} {} not locked: {:?}", eng.seed(), id.name(), p.stage);
            let truth = eng.rx.scn.freq_at(id, T_END);
            let z = (p.f_now - truth) / p.f_sigma;
            assert!(z.abs() < 5.0, "seed {} {}: frequency error is {z:.1} stated σ", eng.seed(), id.name());
            zs.push(z);
            if let Some(sk) = &p.sky {
                m2s.push(mahalanobis2(sk, eng.rx.scn.lm(id)));
            }
        }
    }
    let rms = (zs.iter().map(|z| z * z).sum::<f64>() / zs.len() as f64).sqrt();
    assert!(rms < 1.7, "frequency z-score rms {rms:.2} (calibrated ≈ 1)");
    let inside = m2s.iter().filter(|&&m| m <= 5.99).count() as f64 / m2s.len() as f64;
    assert!(inside >= 0.85, "only {:.0}% of position errors inside the stated 95% region", inside * 100.0);
    assert!(m2s.iter().all(|&m| m < 13.8), "a position error outside the 99.9% region: {m2s:?}");
}

#[test]
fn noise_only_data_never_locks() {
    // The null: same receiver, all signal amplitudes zero. The lock machinery must not
    // manufacture sources out of noise.
    let mut candidates = 0;
    for seed in 1..=6u64 {
        let eng = Engine::from_receiver(Receiver::null(seed));
        for k in 1..=N_CP {
            let cp = eng.checkpoint(k);
            for id in SigId::ALL {
                let st = cp.signal(id).stage;
                assert!(st <= Stage::Candidate, "seed {seed} k={k} {} reached {st:?} on pure noise", id.name());
                if st == Stage::Candidate {
                    candidates += 1;
                }
            }
        }
        let cp = eng.checkpoint(N_CP);
        assert!(
            cp.events.iter().all(|e| !matches!(e.kind, EventKind::Lock | EventKind::Localized | EventKind::Resolved)),
            "seed {seed}: lock events on pure noise"
        );
        assert!(cp.lines.iter().all(|l| l.class != LineClass::Terrestrial), "noise flagged as terrestrial RFI");
    }
    // a sparse false-candidate rate is expected and *shown* as candidates, never as locks
    let possible = 6 * 3 * N_CP;
    assert!(
        (candidates as f64) < 0.15 * possible as f64,
        "{candidates}/{possible} candidate-checkpoints on pure noise"
    );
}

#[test]
fn sign_and_orientation_conventions_recover_the_sky_at_high_snr() {
    // Boost every source far above the noise so statistics are irrelevant; the only
    // thing left to be wrong is geometry (station layout, delay sign, (l, m) axes).
    let mut scn = Scenario::from_seed(SEED);
    scn.alpha.amp *= 8.0;
    scn.beta.amp *= 8.0;
    scn.gamma.amp *= 8.0;
    let eng = Engine::from_receiver(Receiver::from_scenario(scn.clone()));
    let cp = eng.checkpoint(N_CP);
    for id in [SigId::Alpha, SigId::Beta] {
        let p = cp.signal(id);
        let sk = p.sky.expect("position");
        let t = scn.lm(id);
        let err = ((sk.lm.0 - t.0).powi(2) + (sk.lm.1 - t.1).powi(2)).sqrt();
        assert!(err < 0.03, "{}: truth {:?} estimate {:?} (err {err:.3})", id.name(), t, sk.lm);
    }
    // GAMMA is a 0.53 Hz source: even at 8x amplitude the 56 ms baseline only spans
    // ~0.19 rad of its cycle, so its error is judged against its *own* stated σ.
    let g = cp.signal(SigId::Gamma).sky.expect("position");
    assert!(mahalanobis2(&g, scn.lm(SigId::Gamma)) < 13.8);
    // and the delays themselves have the injected sign
    let d = cp.signal(SigId::Alpha).delays.unwrap();
    let want1 = delay_s(scn.alpha.lm, 1);
    assert!((d.tau[0] - want1).abs() < 5e-4, "tau1 {} vs {}", d.tau[0], want1);
}

#[test]
fn interference_is_rejected_for_the_right_reasons_and_the_ghost_fades() {
    let eng = Engine::shared(SEED);
    let cp = eng.checkpoint(N_CP);
    let scn = &eng.rx.scn;
    // the terrestrial line: amplitude differs between stations, delays fit no sky
    let rfi = cp
        .interference()
        .into_iter()
        .find(|l| (l.f - scn.rfi.f).abs() < 0.05)
        .expect("RFI line found");
    assert_eq!(rfi.class, LineClass::Terrestrial);
    assert!(rfi.amp_z >= 4.0, "amplitude non-uniformity z = {}", rfi.amp_z);
    // (Its 66 ms delay exceeds the 56 ms baseline's unambiguous range at 13 Hz — the
    // period is 75 ms — so the phase test alone *cannot* reject it; the amplitude test does.)
    // flagged early enough to matter, and before the pulse train locks
    let flag = cp
        .events
        .iter()
        .find(|e| e.kind == EventKind::FlagTerrestrial)
        .expect("flag event");
    let alpha_lock = cp.signal(SigId::Alpha).lock_cp.expect("alpha lock");
    assert!(flag.cp <= alpha_lock && flag.t() <= 130.0, "flagged at {} s", flag.t());
    // the ghost: grows like a source while the burst lasts, then bends over
    let ghost = cp
        .interference()
        .into_iter()
        .find(|l| (l.f - scn.ghost.f).abs() < 0.05)
        .expect("ghost remembered");
    assert_eq!(ghost.class, LineClass::Transient);
    let (t_peak, peak) = ghost
        .growth
        .iter()
        .cloned()
        .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    assert!(t_peak >= scn.ghost.t_on + 60.0 && t_peak <= scn.ghost.t_off + 25.0, "S/N peaks at {t_peak} s");
    assert!(ghost.growth.last().unwrap().1 < 0.85 * peak);
    // …and was *not* flagged transient while it still looked like a source
    let tflag = cp.events.iter().find(|e| e.kind == EventKind::FlagTransient).unwrap();
    assert!(tflag.t() > scn.ghost.t_off, "flagged transient at {} s, before the burst ended at {} s", tflag.t(), scn.ghost.t_off);
}

#[test]
fn a_misleading_candidate_is_formed_then_revised() {
    // On the default seed the bright terrestrial line and the ghost happen to sit on
    // harmonics of a wrong fundamental. The analysis adopts it as a candidate for
    // ALPHA, then — once the terrestrial line is flagged and masked — revises it.
    let eng = Engine::shared(SEED);
    let cp = eng.checkpoint(N_CP);
    let rev = cp
        .events
        .iter()
        .find(|e| e.kind == EventKind::Revised && e.who == Who::Sig(SigId::Alpha))
        .expect("alpha candidate revised");
    let flag = cp.events.iter().find(|e| e.kind == EventKind::FlagTerrestrial).unwrap();
    assert!(rev.cp < flag.cp + 2, "revision at {} s, flag at {} s", rev.t(), flag.t());
    // the early (wrong) frequency really was wrong, the final one is right
    let early = eng.checkpoint(rev.cp - 1).signal(SigId::Alpha).f_now;
    let truth = eng.rx.scn.alpha.f;
    assert!((early - truth).abs() > 0.05, "early candidate {early} vs truth {truth}");
    let fin = cp.signal(SigId::Alpha).f_now;
    assert!((fin - truth).abs() < 5.0 * cp.signal(SigId::Alpha).f_sigma);
}

#[test]
fn confidence_and_position_precision_improve_with_time() {
    let eng = Engine::shared(SEED);
    for id in [SigId::Alpha, SigId::Beta] {
        let mut last = f64::INFINITY;
        for t in [104.0, 152.0, 200.0, 300.0, 480.0] {
            let cp = eng.checkpoint_at_sample((t * FS) as usize);
            let s = cp.signal(id).sky.unwrap().sig_major;
            assert!(s < last * 1.02, "{} σ went from {last:.3} to {s:.3} at {t}", id.name());
            last = s;
        }
        let early = eng.checkpoint_at_sample((104.0 * FS) as usize).signal(id).sky.unwrap().sig_major;
        assert!(last < 0.6 * early, "{}: {early:.3} -> {last:.3}", id.name());
    }
    // lock is sticky: stages never regress once Locked
    for id in SigId::ALL {
        let mut prev = Stage::Unseen;
        for k in 1..=N_CP {
            let st = eng.checkpoint(k).signal(id).stage;
            if prev >= Stage::Locked {
                assert!(st >= prev, "{} regressed {prev:?} -> {st:?} at k={k}", id.name());
            }
            prev = st;
        }
    }
}

#[test]
fn the_dossier_is_honest_about_what_cannot_be_known() {
    // GAMMA (0.53 Hz) is locked but its position is unconstrained: a 56 ms baseline
    // spans only ~0.19 rad of its cycle. The final interpretation must say so rather
    // than print a confident az/el.
    let eng = Engine::shared(SEED);
    let cp = eng.checkpoint(N_CP);
    let g = cp.signal(SigId::Gamma);
    assert_eq!(g.stage, Stage::Locked);
    let sk = g.sky.unwrap();
    assert!(sk.sig_major > 0.3);
    let lines = pulsar::render::dossier::identity_lines(&cp, SigId::Gamma, 200);
    let txt: String = lines.iter().map(|l| l.0.clone() + "\n").collect();
    assert!(txt.contains("unconstrained"), "{txt}");
    assert!(!txt.contains(" az "), "must not print a confident direction: {txt}");
    // while a resolved source does print one
    let a = pulsar::render::dossier::identity_lines(&cp, SigId::Alpha, 200);
    assert!(a.iter().any(|l| l.0.contains("az ")));
}

#[test]
fn coherent_sources_grow_like_root_t_and_the_slope_is_measured() {
    let eng = Engine::shared(SEED);
    let cp = eng.checkpoint(N_CP);
    for id in SigId::ALL {
        let slope = pulsar::render::dossier::growth_slope(&cp.signal(id).growth).expect("enough points");
        assert!((0.3..0.85).contains(&slope), "{} growth exponent {slope:.2} (coherent ⇒ ½)", id.name());
    }
}

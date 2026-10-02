//! The fixture-tuning search (second, finer stage), restored verbatim from the session that
//! produced `fixture.rs::Params::default()`. It grids VM physics × operator scripts × seeds and
//! keeps the worlds whose root history melts down in 280..=520 steps *and* where dropping/replacing
//! one command survives, ranking by how many distinct epochs the pair of histories visits.
//!
//!     cargo run --release --example tune -- 1 16        # seeds 1..=16
//!
//! NOTE: this search does **not** reproduce the shipped fixture. It varies five of the nine `Params`;
//! the shipped defaults (root meltdown at step 344, deadlock at 214) additionally changed `grace`,
//! `surge_heat`, `pump_odds` and `operator_rest` by hand-probing afterwards, and those probes were
//! not kept as code. What the claim "tuned by search" rests on is this harness plus those probes.
//!
//! The first, coarser stage (3×3×4×2 grid, no `sup_quantum`) was an earlier version of this same
//! file; it was not kept separately. The grids/weights below are the ones of the final run.
use project_chronoscope::epoch::*;
use project_chronoscope::fixture::*;
use project_chronoscope::history::*;
use project_chronoscope::vm::*;

fn eps(h: &History, b: BranchId) -> std::collections::BTreeSet<Epoch> {
    h.timeline(b).iter().map(|r| r.epoch).collect()
}
fn letters(e: &std::collections::BTreeSet<Epoch>) -> String {
    e.iter().map(|x| x.letter()).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let lo: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let hi: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let scripts: Vec<Vec<(u32, u8)>> = vec![
        vec![(50, 1), (62, 1), (74, 1)],
        vec![(60, 1), (110, 1), (160, 1)],
        vec![(40, 1), (70, 1), (100, 1), (130, 1)],
        vec![(80, 1), (95, 1), (110, 1)],
    ];
    let mut results: Vec<(i32, String)> = vec![];
    for sensor_sleep in [1u16, 2] {
        for heat_gain in [2u8, 3] {
            for cool_init in [2i32, 3, 4] {
                for sup_sleep in [3u16, 6] {
                    for sup_quantum in [5u8, 8, 12] {
                        let params = Params {
                            sensor_sleep,
                            heat_gain,
                            cool_init,
                            sup_sleep,
                            sup_quantum,
                            ..Params::default()
                        };
                        let prog = colony_with(&params);
                        for (si, cmds) in scripts.iter().enumerate() {
                            for seed in lo..=hi {
                                let sc = Script::new(
                                    cmds.iter()
                                        .map(|(at, c)| Input {
                                            at: *at,
                                            kind: InputKind::Cmd(*c),
                                        })
                                        .collect(),
                                );
                                let mut h = History::new(prog.clone(), seed, sc);
                                h.run_to_end(0);
                                if h.branch(0).terminal != Some(Terminal::Meltdown) {
                                    continue;
                                }
                                let end = h.branch(0).end();
                                if !(280..=520).contains(&end) {
                                    continue;
                                }
                                let re = eps(&h, 0);
                                if !(re.contains(&Epoch::Escalating)
                                    && re.contains(&Epoch::Catastrophe))
                                {
                                    continue;
                                }
                                let mut best: Option<(i32, String)> = None;
                                let mut nsurv = 0i32;
                                for (at, _) in cmds.iter() {
                                    for edit in [Edit::DropCmd, Edit::ReplaceCmd(2)] {
                                        let b = h.fork(0, *at, edit.clone()).unwrap();
                                        h.run_to_end(b);
                                        if h.branch(b).terminal == Some(Terminal::Meltdown) {
                                            continue;
                                        }
                                        nsurv += 1;
                                        let ve = eps(&h, b);
                                        let union: std::collections::BTreeSet<Epoch> =
                                            re.union(&ve).copied().collect();
                                        let sc = union.len() as i32 * 10
                                            + (ve.contains(&Epoch::Resolution) as i32) * 6
                                            + (ve.contains(&Epoch::Stable) as i32) * 3
                                            + (re.contains(&Epoch::Uncertain) as i32) * 3
                                            + (re.contains(&Epoch::Contradiction) as i32) * 5
                                            + (re.contains(&Epoch::Deadlock) as i32) * 5;
                                        if best.as_ref().map(|b| sc > b.0).unwrap_or(true) {
                                            best = Some((
                                                sc,
                                                format!(
                                                    "{:?}@{at} -> {:?}@{} eps={}",
                                                    edit,
                                                    h.branch(b).terminal,
                                                    h.branch(b).end(),
                                                    letters(&ve)
                                                ),
                                            ));
                                        }
                                    }
                                }
                                if let Some((sc, d)) = best {
                                    let score = sc - (nsurv - 3).abs();
                                    results.push((score, format!("score {score} p=ss{sensor_sleep} hg{heat_gain} ci{cool_init} sl{sup_sleep} sq{sup_quantum} script{si} seed {seed}: root MELT@{end} eps={} | {d} (nsurv {nsurv})", letters(&re))));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    results.sort_by_key(|r| std::cmp::Reverse(r.0));
    for (_, l) in results.iter().take(25) {
        println!("{l}");
    }
    println!("{} candidates", results.len());
}

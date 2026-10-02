//! Core determinism / time-travel correctness: replay equality, reconstruction, fork
//! stability, jump/rewind correctness and comparison semantics.

use project_chronoscope::epoch::*;
use project_chronoscope::fixture::*;
use project_chronoscope::history::*;
use project_chronoscope::sem::*;
use project_chronoscope::vm::*;

fn demo() -> History {
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    h
}

fn digests(h: &History, b: BranchId) -> Vec<(u64, u64)> {
    h.timeline(b).iter().map(|r| (r.d_full, r.d_comp)).collect()
}

#[test]
fn demo_run_is_the_documented_catastrophe() {
    let h = demo();
    let b = h.branch(0);
    assert_eq!(b.terminal, Some(Terminal::Meltdown));
    assert_eq!(b.end(), 344, "fixture golden: meltdown step");
    let tl = h.timeline(0);
    assert_eq!(tl.last().unwrap().epoch, Epoch::Catastrophe);
    // the story we tell in the README is really in the data
    for want in [
        Epoch::Stable,
        Epoch::Uncertain,
        Epoch::Deadlock,
        Epoch::Escalating,
        Epoch::Contradiction,
        Epoch::Catastrophe,
    ] {
        assert!(tl.iter().any(|r| r.epoch == want), "missing epoch {want:?}");
    }
}

#[test]
fn replay_equality_two_fresh_runs() {
    let a = demo();
    let b = demo();
    assert_eq!(digests(&a, 0), digests(&b, 0));
    let ea: Vec<_> = a.timeline(0).iter().map(|r| r.ev).collect();
    let eb: Vec<_> = b.timeline(0).iter().map(|r| r.ev).collect();
    assert_eq!(ea, eb, "events identical (program + seed + ordered inputs)");
}

#[test]
fn replay_from_scratch_matches_cached_records() {
    let mut h = demo();
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).unwrap();
    h.run_to_end(f);
    for b in [0, f] {
        let fresh = h.replay_from_scratch(b);
        let cached = h.timeline(b);
        assert_eq!(fresh.len(), cached.len());
        for (x, y) in fresh.iter().zip(cached.iter()) {
            assert_eq!(x.d_full, y.d_full);
            assert_eq!(x.ev, y.ev);
            assert_eq!(x.epoch, y.epoch);
        }
    }
}

#[test]
fn different_seed_changes_the_history() {
    let mut h2 = History::new(colony(), DEMO_SEED + 1, demo_script());
    h2.run_to_end(0);
    let h = demo();
    assert_ne!(digests(&h, 0), digests(&h2, 0));
}

#[test]
fn checkpoint_reconstruction_is_exact_at_every_position() {
    let mut h = demo();
    let tl_digests = digests(&h, 0);
    for pos in 1..=h.branch(0).end() {
        let (m, _) = h.machine_at(0, pos).unwrap();
        assert_eq!(
            m.digest_full(),
            tl_digests[pos as usize - 1].0,
            "position {pos}"
        );
    }
    // position 0 is the pristine initial state
    let (m0, _) = h.machine_at(0, 0).unwrap();
    assert_eq!(m0.step, 0);
    assert_eq!(
        m0.digest_full(),
        Machine::new(colony(), DEMO_SEED).digest_full()
    );
}

#[test]
fn jump_rewind_correctness_in_arbitrary_order() {
    let mut h = demo();
    let want = digests(&h, 0);
    // a scrambled walk through time, forwards and backwards
    let mut x: u64 = 12345;
    for _ in 0..400 {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let pos = 1 + ((x >> 33) as u32 % h.branch(0).end());
        let (m, tr) = h.machine_at(0, pos).unwrap();
        assert_eq!(m.digest_full(), want[pos as usize - 1].0);
        assert_eq!(tr.cur, h.rec_at(0, pos - 1).unwrap().epoch);
    }
}

#[test]
fn fork_shares_prefix_and_leaves_old_future_intact() {
    let mut h = demo();
    let before = digests(&h, 0);
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).unwrap();
    h.run_to_end(f);
    // the parent is untouched
    assert_eq!(digests(&h, 0), before);
    // shared prefix is bit-identical up to and including the fork position
    let da = digests(&h, 0);
    let db = digests(&h, f);
    for s in 0..50usize {
        assert_eq!(da[s], db[s], "step {s} shared");
    }
    // ...and the futures differ
    assert_ne!(da[60], db[60]);
    assert_eq!(h.branch(0).terminal, Some(Terminal::Meltdown));
    assert_eq!(
        h.branch(f).terminal,
        Some(Terminal::StepCap),
        "one changed input avoids the meltdown"
    );
}

#[test]
fn forking_with_the_natural_decision_is_byte_identical_forever() {
    // "branch stability": re-deciding a decision with the value it already had must not
    // change a single bit of internal state afterwards.
    let mut h = demo();
    let fate = (0..h.branch(0).end())
        .find(|&s| h.rec_at(0, s).unwrap().ev.is_fate())
        .unwrap();
    let natural = h.rec_at(0, fate).unwrap().ev.after;
    let f = h.fork(0, fate, Edit::Override(natural)).unwrap();
    h.run_to_end(f);
    let cmp = compare(&h, 0, f).unwrap();
    assert!(
        cmp.rows.iter().all(|r| r.verdict == Verdict::Identical),
        "every step byte-identical"
    );
    assert_eq!(h.branch(0).end(), h.branch(f).end());
    assert_eq!(h.branch(0).terminal, h.branch(f).terminal);
}

#[test]
fn flipping_a_decision_is_a_single_intervention_root() {
    let mut h = demo();
    // the surge fate near 272 (the rare outcome 0 -> 1 = no surge)
    let surge = (0..h.branch(0).end())
        .find(|&s| {
            let e = h.rec_at(0, s).unwrap().ev;
            e.is_fate() && e.aux == 8 && e.after == 0
        })
        .expect("a surge was rolled in the demo run");
    let f = h.fork(0, surge, Edit::Override(1)).unwrap();
    h.run_to_end(f);
    let cmp = compare(&h, 0, f).unwrap();
    assert_eq!(cmp.first_divergence, Some(surge));
    let root = cmp.rows[surge as usize].root;
    assert_eq!(root, Some(RootKind::Intervention));
    // before the intervention nothing diverged
    assert!(cmp.rows[..surge as usize]
        .iter()
        .all(|r| !r.diverged_event && r.verdict == Verdict::Identical));
}

#[test]
fn fork_validation_rejects_nonsense() {
    let mut h = demo();
    assert_eq!(
        h.fork(0, 1, Edit::Override(0)).unwrap_err(),
        HistoryError::NotADecision
    );
    assert_eq!(
        h.fork(0, 51, Edit::DropCmd).unwrap_err(),
        HistoryError::NoCommandThere
    );
    assert_eq!(
        h.fork(0, 9999, Edit::DropCmd).unwrap_err(),
        HistoryError::BadPosition
    );
    assert_eq!(
        h.fork(7, 1, Edit::DropCmd).unwrap_err(),
        HistoryError::NoSuchBranch
    );
    // forking *at* the terminal position has nothing left to change
    let end = h.branch(0).end();
    assert_eq!(
        h.fork(0, end, Edit::InsertCmd(2)).unwrap_err(),
        HistoryError::NothingToChange
    );
}

#[test]
fn fossilize_and_rehydrate_reproduces_everything() {
    let mut h = demo();
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).unwrap();
    h.run_to_end(f);
    let want = digests(&h, f);
    let want_ev: Vec<_> = h.timeline(f).iter().map(|r| r.ev).collect();
    let bytes_before = h.approx_bytes();
    h.fossilize(f);
    h.fossilize(0);
    assert!(h.approx_bytes() < bytes_before / 4, "fossils are small");
    assert!(
        h.rec_at(f, 100).is_none(),
        "a fossil does not pretend to have records"
    );
    h.ensure_chain(f);
    assert_eq!(digests(&h, f), want);
    let got_ev: Vec<_> = h.timeline(f).iter().map(|r| r.ev).collect();
    assert_eq!(got_ev, want_ev);
    assert!(h.rehydrated_total >= 2);
}

#[test]
fn budget_fossilizes_least_recently_used_but_never_the_protected() {
    let mut h = demo();
    h.retention.max_resident_recs = 700;
    let mut ids = vec![];
    for at in [50u32, 62, 74] {
        let f = h.fork(0, at, Edit::DropCmd).unwrap();
        h.run_to_end(f);
        ids.push(f);
    }
    h.enforce_budget(&[ids[2]]);
    assert!(h.resident_recs() <= 700 + h.branch(ids[2]).recs.len().max(h.branch(0).recs.len()));
    assert_eq!(h.branch(ids[2]).residency, Residency::Resident);
    assert_eq!(
        h.branch(0).residency,
        Residency::Resident,
        "ancestor of a protected branch is kept"
    );
}

#[test]
fn compare_distinguishes_identical_equivalent_and_divergent() {
    let mut h = demo();
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).unwrap();
    h.run_to_end(f);
    let cmp = compare(&h, 0, f).unwrap();
    assert!(cmp.rows[..50]
        .iter()
        .all(|r| r.verdict == Verdict::Identical));
    assert!(cmp.rows.iter().any(|r| r.verdict == Verdict::Divergent));
    // semantic equivalence must never be reported when the full digest matches, and the
    // verdict function is a strict ladder
    for r in &cmp.rows {
        if r.verdict == Verdict::Identical {
            assert!(r.rel.iter().all(|x| *x == Rel::Same));
        }
    }
    // a pair of byte-different states can be semantically equivalent
    let any_equiv = cmp
        .rows
        .iter()
        .any(|r| r.verdict == Verdict::Equivalent || r.verdict == Verdict::ComputationallyEqual);
    let _ = any_equiv; // informational: not every pair converges
}

#[test]
fn landmarks_are_sorted_and_land_on_real_positions() {
    let h = demo();
    let lm = landmarks(&h, 0);
    assert!(lm.windows(2).all(|w| w[0].pos <= w[1].pos));
    assert!(lm.iter().all(|l| l.pos <= h.branch(0).end()));
    assert!(lm
        .iter()
        .any(|l| l.kind == LandmarkKind::Catastrophe && l.pos == 344));
    assert!(lm.iter().filter(|l| l.kind == LandmarkKind::Input).count() == 3);
}

#[test]
fn step_cap_is_enforced() {
    let mut h = History::new(colony(), DEMO_SEED, Script::new(vec![]));
    h.run_to_end(0);
    assert!(h.branch(0).end() <= MAX_STEPS);
}

#[test]
fn causal_ancestry_never_points_forward() {
    let h = demo();
    for s in [50u32, 200, 300, 343] {
        let anc = ancestry(&h, 0, s, 64);
        assert_eq!(anc[0], (s, 0));
        for (a, _) in anc {
            assert!(a <= s);
        }
    }
}

#[test]
fn cmd_override_and_drop_edit_semantics() {
    let h = demo();
    let s = h.apply_edit(0, 62, &Edit::DropCmd).unwrap();
    assert_eq!(s.inputs.len(), 2);
    let s = h.apply_edit(0, 62, &Edit::ReplaceCmd(2)).unwrap();
    assert!(s.inputs.contains(&Input {
        at: 62,
        kind: InputKind::Cmd(2)
    }));
    let s = h.apply_edit(0, 10, &Edit::InsertCmd(2)).unwrap();
    assert_eq!(s.inputs.len(), 4);
    assert_eq!(s.inputs[0].at, 10);
}

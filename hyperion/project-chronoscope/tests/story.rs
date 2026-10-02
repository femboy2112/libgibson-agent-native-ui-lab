//! Story + Scene under time travel: the director at any history position equals a from-scratch
//! replay, rewind-then-forward equals forward-only, and effects are pure functions of the
//! director state (so they can be repositioned deterministically).

use gibson::story::StoryDirector;
use project_chronoscope::director::*;
use project_chronoscope::fixture::*;
use project_chronoscope::history::*;
use std::time::Duration;

fn fingerprint(d: &StoryDirector) -> String {
    let mut s = format!(
        "{}|{:?}|{:?}",
        d.current_beat(),
        d.elapsed(),
        d.time_in_beat()
    );
    for (k, v) in d.facts().iter() {
        s += &format!("|{k}={v:?}");
    }
    for m in d.mounted() {
        s += &format!("|mount:{m}");
    }
    s
}

fn scratch(a: &Atmosphere, h: &History, b: BranchId, pos: u32) -> StoryDirector {
    // chain events from step 0 of the whole timeline (ancestors included): no checkpoints
    let mut d = a.fresh();
    for s in 0..pos {
        d.update(STEP_DT, &events_for(h, b, s));
    }
    d
}

fn demo() -> (History, BranchId) {
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).unwrap();
    h.run_to_end(f);
    (h, f)
}

#[test]
fn director_at_every_position_equals_replay_from_scratch() {
    let (h, f) = demo();
    let mut a = Atmosphere::new();
    for b in [0, f] {
        for pos in (0..=h.branch(b).end())
            .step_by(7)
            .chain([h.branch(b).end()])
        {
            let got = a.director_at(&h, b, pos);
            assert_eq!(
                fingerprint(&got),
                fingerprint(&scratch(&a, &h, b, pos)),
                "branch {b} pos {pos}"
            );
        }
    }
    assert!(
        a.stats.checkpoint_hits > 0,
        "checkpoints were actually used"
    );
}

#[test]
fn rewind_then_forward_equals_forward_only() {
    let (h, _f) = demo();
    let mut a = Atmosphere::new();
    let forward: Vec<String> = (0..=344)
        .step_by(5)
        .map(|p| fingerprint(&a.director_at(&h, 0, p)))
        .collect();
    let mut b = Atmosphere::new();
    // visit positions in a hostile order
    let mut order: Vec<u32> = (0..=344).step_by(5).collect();
    let mut x = 99u64;
    for i in (1..order.len()).rev() {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        order.swap(i, (x >> 33) as usize % (i + 1));
    }
    let mut got = std::collections::BTreeMap::new();
    for p in order {
        got.insert(p, fingerprint(&b.director_at(&h, 0, p)));
    }
    let shuffled: Vec<String> = got.into_values().collect();
    assert_eq!(forward, shuffled);
}

#[test]
fn replay_cost_is_bounded_by_the_checkpoint_interval() {
    let (h, _) = demo();
    let mut a = Atmosphere::new();
    a.director_at(&h, 0, 340); // warm: lays checkpoints
    let before = a.stats.replayed_updates;
    a.director_at(&h, 0, 337);
    let cost = a.stats.replayed_updates - before;
    assert!(
        cost <= STORY_CKPT_EVERY as u64,
        "a jump replayed {cost} updates"
    );
}

#[test]
fn atmosphere_follows_the_semantics_not_the_wall_clock() {
    let (h, _) = demo();
    let mut a = Atmosphere::new();
    // inside the deadlock the beat is "deadlock", at the catastrophe it is "catastrophe"
    let dl = (1..=344)
        .find(|&p| h.rec_at(0, p - 1).unwrap().epoch == project_chronoscope::epoch::Epoch::Deadlock)
        .unwrap();
    assert_eq!(a.director_at(&h, 0, dl + 1).current_beat(), "deadlock");
    assert_eq!(a.director_at(&h, 0, 344).current_beat(), "catastrophe");
    assert!(a.director_at(&h, 0, 344).facts().bool("catastrophe"));
    // rewinding before it clears the fact again: the director is rebuilt, not mutated backwards
    assert!(!a.director_at(&h, 0, 100).facts().bool("catastrophe"));
}

#[test]
fn a_forked_branch_replays_its_own_birth() {
    let (h, f) = demo();
    let mut a = Atmosphere::new();
    let before = a.director_at(&h, f, 50);
    let after = a.director_at(&h, f, 52);
    assert!(!before.facts().bool("forked"));
    assert!(
        after.facts().bool("forked"),
        "the child's own timeline carries the fork event"
    );
    let parent = a.director_at(&h, 0, 52);
    assert!(!parent.facts().bool("forked"), "the parent never forked");
}

#[test]
fn effects_are_pure_functions_of_time_so_they_can_be_repositioned() {
    let (h, _) = demo();
    let mut a = Atmosphere::new();
    let d = a.director_at(&h, 0, 344);
    // evaluate the same director's presentation repeatedly and at shuffled times: identical
    let p1 = a.presentation(&d);
    let p2 = a.presentation(&d);
    let off = |p: &gibson::Presentation| {
        a.scene
            .evaluate(p)
            .iter()
            .map(|e| (e.label.clone(), e.offset, e.visible))
            .collect::<Vec<_>>()
    };
    assert_eq!(off(&p1), off(&p2));
    // Effect::eval at arbitrary t is order-independent
    // (Effect::shake never settles — see tests/capability.rs — so the app uses settling_shake)
    let e = settling_shake(a.scene.target("viewport").unwrap(), 2.0);
    let mut results = vec![];
    for ms in [300u64, 100, 450, 0, 250, 100, 900] {
        let mut p = gibson::Presentation::new();
        e.eval(Duration::from_millis(ms), &a.scene, &mut p);
        results.push((ms, p.offset_of(&a.scene, a.ids.viewport)));
    }
    let again: Vec<_> = results
        .iter()
        .map(|(ms, _)| {
            let mut p = gibson::Presentation::new();
            e.eval(Duration::from_millis(*ms), &a.scene, &mut p);
            (*ms, p.offset_of(&a.scene, a.ids.viewport))
        })
        .collect();
    assert_eq!(results, again);
    assert_eq!(
        results[1].1, results[5].1,
        "the same t gives the same offset whatever was evaluated in between"
    );
    // a shake settles exactly at its deadline
    let mut p = gibson::Presentation::new();
    e.eval(Duration::from_millis(5_000), &a.scene, &mut p);
    assert_eq!(
        p.offset_of(&a.scene, a.ids.viewport),
        (0, 0),
        "the composed shake settles exactly"
    );
}

#[test]
fn jump_to_and_facts_mut_are_not_replayable_so_we_never_use_them() {
    // The reason director_at is built from recorded updates only: an unrecorded jump_to is
    // invisible to Story::replay (documented by LibGibson itself; here it is demonstrated).
    let a = Atmosphere::new();
    let mut d = a.fresh();
    assert!(d.jump_to("deadlock"));
    assert_eq!(d.current_beat(), "deadlock");
    let replayed = project_chronoscope::director::build_story(a.ids).replay(d.trace());
    assert_eq!(
        replayed.current_beat(),
        "stable",
        "replay cannot reproduce an unrecorded jump"
    );
}

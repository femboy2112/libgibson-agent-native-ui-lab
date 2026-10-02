//! The guided "WTF moment" is a real sequence of real commands, not a recording.

mod common;
use common::*;
use gibson::ColorDepth;
use project_chronoscope::demo::Demo;
use project_chronoscope::epoch::Epoch;
use project_chronoscope::history::Edit;
use project_chronoscope::vm::Terminal;

#[test]
fn the_demo_runs_to_catastrophe_rewinds_forks_one_input_and_compares() {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    r.model.demo = Some(Demo::new());
    let mut hit_catastrophe = false;
    let mut turned_around = false;
    let mut frames = 0;
    while r.model.demo.is_some() && frames < 20_000 {
        r.frame().unwrap();
        frames += 1;
        hit_catastrophe |= r.model.cur_b == 0
            && r.model.cursor == 344
            && r.model.epoch_here() == Epoch::Catastrophe;
        turned_around |= r.model.facing_back;
    }
    assert!(r.model.demo.is_none(), "the demo finishes by itself");
    assert!(hit_catastrophe, "it visits the meltdown");
    assert!(
        turned_around,
        "the camera turned to face the past while rewinding"
    );
    assert_eq!(r.model.hist.branches.len(), 2, "exactly one fork");
    let b = r.model.hist.branch(1);
    assert_eq!(b.fork_at, 50);
    assert_eq!(b.edit, Edit::ReplaceCmd(2), "one changed input");
    assert_eq!(
        b.terminal,
        Some(Terminal::StepCap),
        "the second future survives"
    );
    assert_eq!(
        r.model.hist.branch(0).terminal,
        Some(Terminal::Meltdown),
        "the first future is still there, dead"
    );
    assert!(r.model.compare.is_some(), "the demo ends in comparison");
    assert!(r
        .model
        .journal
        .iter()
        .any(|l| l.starts_with("fork B from A@50")));
}

#[test]
fn any_key_takes_over_from_the_demo() {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    r.model.demo = Some(Demo::new());
    for _ in 0..30 {
        r.frame().unwrap();
    }
    assert!(r.model.demo.is_some());
    r.key('p');
    assert!(r.model.demo.is_none(), "a key press cancels the demo");
}

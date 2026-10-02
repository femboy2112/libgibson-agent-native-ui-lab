//! Focus, modals and input routing through the real `UiRuntime`, plus rapid input.

mod common;
use common::*;
use gibson::{ColorDepth, Event, KeyCode, KeyEvent, KeyModifiers};
use project_chronoscope::app::*;
use project_chronoscope::driver::*;

fn fresh_fork() -> Rig {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    run_script(&mut r, "end; goto 50; fork replace2; settle").unwrap();
    r
}

#[test]
fn tab_walks_from_the_viewport_through_the_branch_list_and_enter_picks_a_branch() {
    let mut r = fresh_fork();
    assert_eq!(r.model.cur_b, 1);
    assert_eq!(
        r.focus().as_deref(),
        Some("viewport"),
        "the viewport is the first keyed control and owns the arrows"
    );
    r.code(KeyCode::Tab);
    r.frame().unwrap();
    assert_eq!(r.focus().as_deref(), Some("branch.1"));
    r.code(KeyCode::Tab);
    r.frame().unwrap();
    assert_eq!(r.focus().as_deref(), Some("branch.0"));
    r.code(KeyCode::Enter);
    r.frame().unwrap();
    assert_eq!(
        r.model.cur_b, 0,
        "Enter on a focused history row switches to it"
    );
    // Esc hands focus back to the viewport
    r.code(KeyCode::Esc);
    r.frame().unwrap();
    assert_eq!(r.focus().as_deref(), Some("viewport"));
}

#[test]
fn focus_survives_time_travel_and_branch_switches() {
    let mut r = fresh_fork();
    r.code(KeyCode::Tab);
    r.frame().unwrap();
    let f = r.focus();
    assert!(f.is_some());
    for cmd in [
        "goto 10", "goto 300", "step -5", "end", "goto 0", "compare", "compare", "collapse",
        "collapse",
    ] {
        run_script(&mut r, cmd).unwrap();
        r.frame().unwrap();
        // focus is presentation state keyed by identity; rebuilding the *historical* tree must not move it
        assert!(r.focus().is_some(), "focus lost after `{cmd}`");
    }
    // `branch N` is an application-level switch (a different keyed row becomes `selected`) but
    // focus stays where the user put it:
    let before = r.focus();
    run_script(&mut r, "branch 0; frames 1; branch 1; frames 1").unwrap();
    assert_eq!(r.focus(), before);
}

#[test]
fn modal_captures_input_and_restores_focus_on_dismiss() {
    let mut r = fresh_fork();
    r.code(KeyCode::Tab);
    r.code(KeyCode::Tab); // focus branch.0, neither the first nor the default control
    r.frame().unwrap();
    let before = r.focus();
    assert_eq!(before.as_deref(), Some("branch.0"));
    run_script(&mut r, "inspect; frames 2").unwrap();
    assert_eq!(r.rt.modal_depth(), 1);
    assert_eq!(
        r.focus().as_deref(),
        Some("inspect.fork"),
        "focus moves into the modal"
    );
    // a modal consumes unbound keys: 'q' must not quit and 'n' must not move the cursor
    let cur = r.model.cursor;
    r.key('q');
    r.key('n');
    assert!(!r.model.quit, "q is swallowed by the modal");
    assert_eq!(r.model.cursor, cur);
    r.code(KeyCode::Esc);
    r.frame().unwrap();
    assert_eq!(r.rt.modal_depth(), 0);
    assert_eq!(
        r.focus(),
        before,
        "dismissal restores the focus the modal interrupted"
    );
}

#[test]
fn keyboard_only_fork_from_the_modal_creates_the_branch() {
    let mut r = fresh_fork();
    run_script(&mut r, "branch 0; goto 62").unwrap();
    r.key('f');
    r.frame().unwrap();
    assert!(matches!(r.model.modal, Modal::Fork(_)));
    assert_eq!(r.focus().as_deref(), Some("fork.0"));
    let n = r.model.hist.branches.len();
    r.code(KeyCode::Enter);
    r.frame().unwrap();
    assert_eq!(
        r.model.hist.branches.len(),
        n + 1,
        "Enter activates the focused fork option"
    );
    assert_eq!(r.model.cur_b as usize, n);
    assert_eq!(r.model.modal, Modal::None);
    assert_eq!(
        r.model.cursor, 62,
        "the fork happens exactly where the cursor was"
    );
}

#[test]
fn space_and_arrows_follow_focus_with_the_viewport_as_the_default() {
    // Friction, documented: the UI layer's Space/arrow handling belongs to whichever keyed
    // control has focus. The viewport is a keyed `on_event` sink, so it gets them by default.
    let mut r = fresh_fork();
    assert_eq!(r.focus().as_deref(), Some("viewport"));
    r.key(' ');
    assert!(
        r.model.playing,
        "Space reaches the application while the viewport is focused"
    );
    r.key(' ');
    assert!(!r.model.playing);
    // focus a history row: now Space activates it and the arrows walk the focus ring
    r.code(KeyCode::Tab);
    r.code(KeyCode::Tab); // branch.0
    r.frame().unwrap();
    let cur = r.model.cursor;
    r.code(KeyCode::Right);
    assert_eq!(
        r.model.cursor, cur,
        "Right is focus traversal, not scrub, while a row has focus"
    );
    assert_eq!(
        r.focus().as_deref(),
        Some("viewport"),
        "…and moved focus forward (wrapping to the viewport)"
    );
    r.code(KeyCode::Tab);
    r.code(KeyCode::Tab);
    r.frame().unwrap();
    assert_eq!(r.focus().as_deref(), Some("branch.0"));
    r.key(' ');
    assert!(
        !r.model.playing,
        "Space activated the focused row instead of toggling run"
    );
    assert_eq!(r.model.cur_b, 0);
}

#[test]
fn rapid_input_is_exact() {
    let mut r = rig(80, 24, ColorDepth::TrueColor);
    r.settle().unwrap();
    let mut expect: i64 = 0;
    // 600 keys with only an occasional frame in between: the queue model must not drop or reorder
    let seq = ['.', '.', ',', '.', 'L', 'H', '.', ']', '[', '.'];
    for i in 0..600usize {
        let c = seq[i % seq.len()];
        let d: i64 = match c {
            '.' => 1,
            ',' => -1,
            'L' => 8,
            'H' => -8,
            ']' => 32,
            '[' => -32,
            _ => 0,
        };
        let before = r.model.cursor as i64;
        r.key(c);
        // `L`/`H` are shifted arrows in the key map only when SHIFT is held; as chars they map too
        let after = r.model.cursor as i64;
        expect = (before + d).clamp(0, r.model.end() as i64);
        assert_eq!(after, expect, "key {i} {c:?}");
        if i % 97 == 0 {
            r.frame().unwrap();
        }
    }
    let _ = expect;
}

#[test]
fn shift_arrows_and_page_keys_scrub_by_the_documented_amounts() {
    let mut r = rig(80, 24, ColorDepth::TrueColor);
    r.settle().unwrap();
    r.route(&Event::Key(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::SHIFT,
    )));
    assert_eq!(r.model.cursor, 8);
    r.code(KeyCode::PageDown);
    assert_eq!(r.model.cursor, 40);
    r.code(KeyCode::PageUp);
    r.code(KeyCode::Left);
    assert_eq!(r.model.cursor, 7);
    r.code(KeyCode::End);
    assert_eq!(r.model.cursor, r.model.end());
    r.code(KeyCode::Home);
    assert_eq!(r.model.cursor, 0);
}

#[test]
fn landmark_jumps_land_on_the_documented_events() {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    r.settle().unwrap();
    run_script(&mut r, "jump next-input").unwrap();
    assert_eq!(
        r.model.cursor, 51,
        "first BOOST is injected at step 50, visible at position 51"
    );
    run_script(&mut r, "jump next-input; jump next-input").unwrap();
    assert_eq!(r.model.cursor, 75);
    run_script(&mut r, "jump prev-input").unwrap();
    assert_eq!(r.model.cursor, 63);
    run_script(&mut r, "end; jump prev-epoch").unwrap();
    assert!(r.model.cursor < 344);
    let e = r.model.epoch_here();
    assert_ne!(e, project_chronoscope::epoch::Epoch::Catastrophe);
    run_script(&mut r, "start; jump next-epoch").unwrap();
    assert!(r.model.cursor > 0);
}

#[test]
fn compare_requires_two_histories_and_collapse_rejoin_animates() {
    let mut r = rig(120, 40, ColorDepth::TrueColor);
    r.settle().unwrap();
    run_script(&mut r, "compare").unwrap();
    assert!(r.model.compare.is_none(), "one history cannot be compared");
    run_script(
        &mut r,
        "end; goto 50; fork replace2; settle; compare; settle",
    )
    .unwrap();
    assert_eq!(r.model.compare, Some((1, 0)));
    run_script(&mut r, "collapse").unwrap();
    let mut spreads = vec![];
    for _ in 0..40 {
        r.frame().unwrap();
        spreads.push(r.model.spread);
    }
    assert!(
        spreads.windows(2).all(|w| w[1] <= w[0] + 1e-6),
        "collapse folds monotonically"
    );
    assert!(*spreads.last().unwrap() < 0.02);
    run_script(&mut r, "collapse").unwrap();
    for _ in 0..40 {
        r.frame().unwrap();
    }
    assert!(r.model.spread > 0.98, "rejoin unfolds the lanes again");
}

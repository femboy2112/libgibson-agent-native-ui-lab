//! The VM's own semantics on tiny purpose-built programs (not the colony fixture).

use project_chronoscope::sem::*;
use project_chronoscope::vm::*;
use std::sync::Arc;

fn prog(tasks: Vec<Vec<Op>>) -> Arc<Program> {
    let n = tasks.len();
    Arc::new(Program {
        name: "t",
        tasks,
        init_vars: VAR_INIT,
        quantum: vec![1; n],
    })
}

fn run(m: &mut Machine, s: &Script, n: u32) -> Vec<StepEvent> {
    run_steps(m, s, n)
}

#[test]
fn opposite_lock_orders_deadlock_and_it_is_detected() {
    let p = prog(vec![
        vec![
            Op::Lock(L_FUEL),
            Op::Sleep(1),
            Op::Lock(L_COOLANT),
            Op::Halt,
        ],
        vec![
            Op::Lock(L_COOLANT),
            Op::Sleep(1),
            Op::Lock(L_FUEL),
            Op::Halt,
        ],
    ]);
    let mut m = Machine::new(p, 1);
    let s = Script::default();
    assert!(m.deadlocked_tasks().is_empty());
    run(&mut m, &s, 12);
    assert_eq!(m.deadlocked_tasks(), vec![0, 1]);
    // a deadlocked machine with nothing else to wake it is wedged, not spinning
    run(&mut m, &s, 20);
    assert_eq!(m.terminal, Some(Terminal::Wedged));
}

#[test]
fn lock_handoff_wakes_the_first_waiter_and_records_why() {
    let p = prog(vec![
        vec![Op::Lock(L_FUEL), Op::Sleep(4), Op::Unlock(L_FUEL), Op::Halt],
        vec![Op::Sleep(1), Op::Lock(L_FUEL), Op::Unlock(L_FUEL), Op::Halt],
    ]);
    let mut m = Machine::new(p, 1);
    let ev = run(&mut m, &Script::default(), 30);
    let wait = ev
        .iter()
        .find(|e| e.class == Class::LockWait)
        .expect("task 1 had to wait");
    assert_eq!(wait.task, 1);
    let rel = ev
        .iter()
        .find(|e| e.class == Class::LockRel && e.task == 0)
        .unwrap();
    assert!(
        rel.flags & F_HANDOFF != 0,
        "the release handed the lock to the waiter"
    );
    // the waiter's very next event names the release as its wake cause
    let next = ev
        .iter()
        .find(|e| e.task == 1 && e.step > rel.step)
        .unwrap();
    assert_eq!(next.parents[2], rel.step);
}

#[test]
fn bounded_channels_apply_backpressure_and_deliver_in_order() {
    let mut producer = vec![];
    for i in 0..(CHAN_CAP as i32 + 3) {
        producer.push(Op::SendI(C_NOTE, 100 + i));
    }
    producer.push(Op::Halt);
    let consumer = vec![
        Op::Sleep(20),
        Op::Recv(C_NOTE, 0),
        Op::Recv(C_NOTE, 1),
        Op::Recv(C_NOTE, 2),
        Op::Recv(C_NOTE, 3),
        Op::Halt,
    ];
    let mut m = Machine::new(prog(vec![producer, consumer]), 1);
    let ev = run(&mut m, &Script::default(), 80);
    assert!(
        ev.iter().any(|e| e.class == Class::SendWait),
        "the producer blocked on a full queue"
    );
    let got: Vec<i32> = ev
        .iter()
        .filter(|e| e.class == Class::Recv)
        .map(|e| e.aux)
        .collect();
    assert!(
        got.starts_with(&[100, 101, 102, 103]),
        "FIFO order: {got:?}"
    );
    assert!(m.chans[C_NOTE as usize].q.len() <= CHAN_CAP);
}

#[test]
fn a_crash_keeps_its_locks_until_a_restart_releases_them() {
    let p = prog(vec![
        vec![Op::Lock(L_FUEL), Op::Fail(9)],
        vec![Op::Sleep(3), Op::Lock(L_FUEL), Op::Set(0, 7), Op::Halt],
        vec![Op::Sleep(12), Op::Restart(0), Op::Halt],
    ]);
    let mut m = Machine::new(p, 1);
    run(&mut m, &Script::default(), 10);
    assert!(matches!(m.tasks[0].status, TStatus::Crashed(9)));
    assert_eq!(
        m.locks[L_FUEL as usize].owner,
        Some(0),
        "a crashed task still holds its lock"
    );
    assert!(matches!(m.tasks[1].status, TStatus::WaitLock(_)));
    assert_eq!(m.vars[V_FAULTS as usize], 1);
    run(&mut m, &Script::default(), 30);
    assert_eq!(
        m.tasks[1].regs[0], 7,
        "the restart released the lock and the waiter proceeded"
    );
    assert_eq!(m.tasks[0].restarts, 1);
}

#[test]
fn decision_override_changes_the_outcome_but_not_the_random_stream() {
    let p = prog(vec![vec![Op::Fate(0, 8), Op::Fate(1, 200), Op::Halt]]);
    let natural = {
        let mut m = Machine::new(p.clone(), 5);
        run(&mut m, &Script::default(), 4);
        (m.tasks[0].regs[0], m.tasks[0].regs[1])
    };
    let overridden = {
        let s = Script::new(vec![Input {
            at: 0,
            kind: InputKind::Override(3),
        }]);
        let mut m = Machine::new(p, 5);
        let ev = run(&mut m, &s, 4);
        assert!(ev[0].flags & F_OVERRIDDEN != 0);
        assert_eq!(
            ev[0].before, natural.0,
            "the dice said what they would have said"
        );
        assert_eq!(ev[0].after, 3);
        (m.tasks[0].regs[0], m.tasks[0].regs[1])
    };
    assert_eq!(overridden.0, 3);
    assert_eq!(
        overridden.1, natural.1,
        "the *next* decision is unaffected: one controlled intervention"
    );
}

#[test]
fn a_command_input_reaches_a_blocked_receiver_with_its_cause_recorded() {
    let p = prog(vec![vec![
        Op::Recv(C_CONSOLE, 0),
        Op::Inc(V_LOAD, 1),
        Op::Halt,
    ]]);
    let s = Script::new(vec![Input {
        at: 5,
        kind: InputKind::Cmd(1),
    }]);
    let mut m = Machine::new(p, 1);
    let ev = run(&mut m, &s, 12);
    assert!(ev[5].flags & F_INPUT != 0);
    let inc = ev.iter().find(|e| e.class == Class::Rmw).unwrap();
    assert_eq!(m.vars[V_LOAD as usize], VAR_INIT[V_LOAD as usize] + 1);
    // the input is a causal root: an input marker (high bit) names the step that injected it
    let roots: Vec<u32> = ev
        .iter()
        .flat_map(|e| e.parents)
        .filter(|p| *p != NONE && p & 0x8000_0000 != 0)
        .collect();
    assert!(roots.contains(&(0x8000_0000 | 5)), "{roots:?} / {inc:?}");
}

#[test]
fn the_step_cap_is_a_hard_bound() {
    let p = prog(vec![vec![Op::Jmp(0)]]);
    let mut m = Machine::new(p, 1);
    let n = run(&mut m, &Script::default(), MAX_STEPS + 50).len() as u32;
    assert_eq!(n, MAX_STEPS);
    assert_eq!(m.terminal, Some(Terminal::StepCap));
    assert!(m.step(&Script::default()).is_none());
}

#[test]
fn thermal_runaway_is_terminal_and_flagged() {
    let p = prog(vec![vec![
        Op::Set(0, 60),
        Op::IncR(V_HEAT, 0),
        Op::IncR(V_HEAT, 0),
        Op::Halt,
    ]]);
    let mut m = Machine::new(p, 1);
    let ev = run(&mut m, &Script::default(), 20);
    assert_eq!(m.terminal, Some(Terminal::Meltdown));
    assert!(ev.last().unwrap().flags & F_CATASTROPHE != 0);
}

#[test]
fn state_digest_ladder_separates_computation_from_provenance() {
    let a = Machine::new(prog(vec![vec![Op::Halt]]), 1);
    let b = Machine::new(prog(vec![vec![Op::Halt]]), 2);
    assert_ne!(
        a.digest_computational(),
        b.digest_computational(),
        "the rng seed is computational state"
    );
    let c = a.clone();
    assert_eq!(a.digest_full(), c.digest_full());
    let mut d = a.clone();
    d.prov.rng_last = 5; // same behaviour, different history
    assert_eq!(a.digest_computational(), d.digest_computational());
    assert_ne!(a.digest_full(), d.digest_full());
    assert_eq!(
        verdict(
            a.digest_full(),
            d.digest_full(),
            a.digest_computational(),
            d.digest_computational(),
            1,
            2
        ),
        Verdict::ComputationallyEqual
    );
    assert_eq!(verdict(1, 2, 3, 4, 9, 9), Verdict::Equivalent);
    assert_eq!(verdict(1, 2, 3, 4, 9, 8), Verdict::Divergent);
    assert_eq!(verdict(7, 7, 3, 4, 9, 8), Verdict::Identical);
}

#[test]
fn semantic_projection_ignores_bookkeeping_and_notices_class_changes() {
    let a = Machine::new(prog(vec![vec![Op::Halt]]), 1);
    let mut b = a.clone();
    b.vars[V_TICKS as usize] = 999;
    b.vars[V_OUT as usize] = 12345;
    b.vars[V_HEAT as usize] += 2; // same band
    assert_eq!(
        sem_key(&dims(&a)),
        sem_key(&dims(&b)),
        "TICKS/OUT/in-band HEAT are not semantic"
    );
    b.vars[V_HEAT as usize] = 80; // different band
    assert_ne!(sem_key(&dims(&a)), sem_key(&dims(&b)));
    let da = dims(&a);
    let db = dims(&b);
    assert_eq!(
        relate(
            da[V_TICKS as usize],
            dims(&{
                let mut c = a.clone();
                c.vars[V_TICKS as usize] = 5;
                c
            })[V_TICKS as usize],
            dim_is_semantic(V_TICKS as usize)
        ),
        Rel::Equiv
    );
    assert_eq!(
        relate(da[V_HEAT as usize], db[V_HEAT as usize], true),
        Rel::Apart
    );
}

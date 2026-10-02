//! The one built-in program: a "reactor colony" with six cooperating tasks.
//!
//! REACTOR and COOLER take the FUEL and COOLANT locks in opposite orders (deadlock is a
//! real interleaving, not a script). SENSOR is the plant clock and raises alarms;
//! OPERATOR consumes console commands (external inputs and supervisor nudges);
//! SUPERVISOR watches for stalls and escalates (nudge, throttle, restart, steal, SCRAM);
//! AUDITOR checks the CREDITS/LEDGER invariant that the workers sometimes break through a
//! deliberately racy fast path.

use crate::vm::*;
use std::sync::Arc;

/// Tunable physics of the colony. The shipped demo uses `Params::default()`.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub sensor_sleep: u16,
    pub heat_gain: u8,
    pub cool_init: i32,
    pub sup_sleep: u16,
    pub surge_heat: i32,
    pub pump_odds: u8,
    pub operator_rest: u16,
    pub grace: u16,
    pub sup_quantum: u8,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            sensor_sleep: 2,
            heat_gain: 2,
            cool_init: 3,
            sup_sleep: 3,
            surge_heat: 9,
            pump_odds: 14,
            operator_rest: 5,
            grace: 40,
            sup_quantum: 8,
        }
    }
}

struct Asm {
    ops: Vec<Op>,
    labels: Vec<(&'static str, u16)>,
    fix: Vec<(usize, &'static str)>,
}

impl Asm {
    fn new() -> Asm {
        Asm { ops: vec![], labels: vec![], fix: vec![] }
    }
    fn l(&mut self, name: &'static str) -> &mut Self {
        self.labels.push((name, self.ops.len() as u16));
        self
    }
    fn o(&mut self, op: Op) -> &mut Self {
        self.ops.push(op);
        self
    }
    fn j(&mut self, op: Op, label: &'static str) -> &mut Self {
        self.fix.push((self.ops.len(), label));
        self.ops.push(op);
        self
    }
    fn done(mut self) -> Vec<Op> {
        for (at, name) in std::mem::take(&mut self.fix) {
            let target = self
                .labels
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("undefined label {name}"))
                .1;
            self.ops[at] = match self.ops[at] {
                Op::Jmp(_) => Op::Jmp(target),
                Op::Jz(r, _) => Op::Jz(r, target),
                Op::Jnz(r, _) => Op::Jnz(r, target),
                Op::Jlt(r, i, _) => Op::Jlt(r, i, target),
                Op::Jge(r, i, _) => Op::Jge(r, i, target),
                Op::Jeq(r, i, _) => Op::Jeq(r, i, target),
                Op::Jne(r, i, _) => Op::Jne(r, i, target),
                Op::JeqR(a, b, _) => Op::JeqR(a, b, target),
                Op::JneR(a, b, _) => Op::JneR(a, b, target),
                other => other,
            };
        }
        self.ops
    }
}

/// Mark tags (landmark annotations carried by `Op::Mark`).
pub const M_SURGE: u8 = 1;
pub const M_PUMP_FAIL: u8 = 2;
pub const M_RACY: u8 = 3;
pub const M_BOOST: u8 = 4;
pub const M_THROTTLE: u8 = 5;
pub const M_RESTART: u8 = 6;
pub const M_RECONCILE: u8 = 7;
pub const M_STEAL: u8 = 8;
pub const M_SCRAM: u8 = 9;
pub const M_NUDGE: u8 = 10;
pub const M_AUDIT_FAIL: u8 = 11;

pub fn mark_name(tag: u8) -> &'static str {
    match tag {
        M_SURGE => "power surge",
        M_PUMP_FAIL => "pump failure",
        M_RACY => "racy credit path",
        M_BOOST => "operator boost",
        M_THROTTLE => "operator throttle",
        M_RESTART => "supervisor restart",
        M_RECONCILE => "ledger reconcile",
        M_STEAL => "lock stolen",
        M_SCRAM => "SCRAM",
        M_NUDGE => "supervisor nudge",
        M_AUDIT_FAIL => "audit failed",
        _ => "mark",
    }
}

fn reactor(p: &Params) -> Vec<Op> {
    let mut a = Asm::new();
    a.o(Op::Rnd(0, 24)).o(Op::Add(0, 2)).o(Op::SleepR(0));
    a.l("top");
    a.o(Op::Rnd(0, 9)).o(Op::Add(0, 3)).o(Op::SleepR(0));
    a.o(Op::Lock(L_FUEL));
    a.o(Op::Rnd(0, 3)).o(Op::Add(0, 1)).o(Op::SleepR(0));
    a.o(Op::Lock(L_COOLANT));
    a.o(Op::Ld(1, V_LOAD)).o(Op::IncR(V_HEAT, 1)).o(Op::IncR(V_OUT, 1));
    a.o(Op::Fate(2, 8)).j(Op::Jne(2, 0, 0), "nosurge");
    a.o(Op::Inc(V_HEAT, p.surge_heat)).o(Op::Mark(M_SURGE));
    a.l("nosurge");
    a.o(Op::Unlock(L_COOLANT)).o(Op::Unlock(L_FUEL));
    a.o(Op::Fate(2, 4)).j(Op::Jne(2, 0, 0), "atomic");
    a.o(Op::Mark(M_RACY)).o(Op::Ld(3, V_CREDITS)).o(Op::Sleep(1)).o(Op::Add(3, 1)).o(Op::St(V_CREDITS, 3));
    a.o(Op::Inc(V_LEDGER, 1)).j(Op::Jmp(0), "done");
    a.l("atomic");
    a.o(Op::Inc(V_CREDITS, 1)).o(Op::Inc(V_LEDGER, 1));
    a.l("done");
    a.o(Op::Inc(V_TICKS, 1)).j(Op::Jmp(0), "top");
    a.done()
}

fn cooler(p: &Params) -> Vec<Op> {
    let mut a = Asm::new();
    a.o(Op::Rnd(0, 24)).o(Op::Add(0, 2)).o(Op::SleepR(0));
    a.l("top");
    a.o(Op::Rnd(0, 9)).o(Op::Add(0, 3)).o(Op::SleepR(0));
    a.o(Op::Lock(L_COOLANT));
    a.o(Op::Rnd(0, 3)).o(Op::Add(0, 1)).o(Op::SleepR(0));
    a.o(Op::Lock(L_FUEL));
    a.o(Op::Fate(2, p.pump_odds)).j(Op::Jne(2, 0, 0), "pump_ok");
    a.o(Op::Mark(M_PUMP_FAIL)).o(Op::Fail(3));
    a.l("pump_ok");
    a.o(Op::Ld(1, V_COOL)).o(Op::AddR(1, 1)).o(Op::DecR(V_HEAT, 1));
    a.o(Op::Unlock(L_FUEL)).o(Op::Unlock(L_COOLANT));
    a.o(Op::Fate(2, 4)).j(Op::Jne(2, 0, 0), "atomic");
    a.o(Op::Mark(M_RACY)).o(Op::Ld(3, V_CREDITS)).o(Op::Sleep(1)).o(Op::Add(3, 1)).o(Op::St(V_CREDITS, 3));
    a.o(Op::Inc(V_LEDGER, 1)).j(Op::Jmp(0), "done");
    a.l("atomic");
    a.o(Op::Inc(V_CREDITS, 1)).o(Op::Inc(V_LEDGER, 1));
    a.l("done");
    a.o(Op::Inc(V_TICKS, 1)).j(Op::Jmp(0), "top");
    a.done()
}

fn sensor(p: &Params) -> Vec<Op> {
    let mut a = Asm::new();
    a.l("top");
    a.o(Op::Sleep(p.sensor_sleep));
    a.o(Op::Ld(0, V_LOAD)).o(Op::Add(0, -2));
    for _ in 0..p.heat_gain {
        a.o(Op::IncR(V_HEAT, 0));
    }
    a.o(Op::Ld(1, V_HEAT)).j(Op::Jlt(1, 70, 0), "top");
    a.o(Op::Post(C_ALARM, 1)).j(Op::Jmp(0), "top");
    a.done()
}

fn operator(p: &Params) -> Vec<Op> {
    let mut a = Asm::new();
    a.l("top");
    a.o(Op::Recv(C_CONSOLE, 0));
    a.j(Op::Jeq(0, 1, 0), "boost");
    a.j(Op::Jeq(0, 2, 0), "throttle");
    a.j(Op::Jeq(0, 3, 0), "flush");
    a.j(Op::Jeq(0, 5, 0), "emergency");
    a.j(Op::Jmp(0), "top");
    a.l("boost");
    a.o(Op::Inc(V_LOAD, 2)).o(Op::Mark(M_BOOST)).j(Op::Jmp(0), "rest");
    a.l("throttle");
    a.o(Op::Inc(V_LOAD, -2)).o(Op::Mark(M_THROTTLE)).j(Op::Jmp(0), "rest");
    a.l("flush");
    a.o(Op::Set(1, 9)).o(Op::St(V_COOL, 1)).j(Op::Jmp(0), "rest");
    a.l("emergency");
    a.o(Op::Inc(V_LOAD, -4)).o(Op::Mark(M_THROTTLE)).j(Op::Jmp(0), "rest");
    a.l("rest");
    a.o(Op::Sleep(p.operator_rest)).j(Op::Jmp(0), "top");
    a.done()
}

fn supervisor(p: &Params) -> Vec<Op> {
    // r2 = consecutive stalled periods; r3 = TICKS at the previous period
    let mut a = Asm::new();
    // boot grace: the workers need a while to complete their first cycle
    a.o(Op::Sleep(p.grace)).o(Op::Set(3, -1));
    a.l("top");
    a.o(Op::Sleep(p.sup_sleep));
    a.o(Op::TryRecv(C_ALARM, 0));
    a.o(Op::Ld(1, V_TICKS));
    a.j(Op::JeqR(1, 3, 0), "stalled");
    // progress: relax
    a.o(Op::Set(2, 0)).o(Op::Mov(3, 1));
    a.o(Op::Alive(0, T_COOLER)).j(Op::Jnz(0, 0), "c_ok");
    a.o(Op::Mark(M_RESTART)).o(Op::Restart(T_COOLER));
    a.l("c_ok");
    a.o(Op::Alive(0, T_AUDITOR)).j(Op::Jnz(0, 0), "a_ok");
    a.o(Op::Mark(M_RECONCILE)).o(Op::Restart(T_AUDITOR)).o(Op::Ld(1, V_LEDGER)).o(Op::St(V_CREDITS, 1));
    a.l("a_ok");
    a.o(Op::Ld(1, V_HEAT)).j(Op::Jlt(1, 80, 0), "calm");
    a.o(Op::Mark(M_NUDGE)).o(Op::SendI(C_CONSOLE, 5)).o(Op::Inc(V_ESC, 1)).j(Op::Jmp(0), "top");
    a.l("calm");
    a.o(Op::Ld(1, V_ESC)).j(Op::Jz(1, 0), "trim");
    a.o(Op::Inc(V_ESC, -1));
    a.l("trim");
    // when the plant is cool, relax the operator's load back toward nominal
    a.o(Op::Ld(1, V_HEAT)).j(Op::Jge(1, 60, 0), "top");
    a.o(Op::Ld(1, V_LOAD)).j(Op::Jlt(1, 4, 0), "top");
    a.o(Op::Inc(V_LOAD, -1)).j(Op::Jmp(0), "top");
    // stalled: the cooler/reactor pair makes no progress
    a.l("stalled");
    a.o(Op::Add(2, 1)).o(Op::Inc(V_ESC, 1));
    a.j(Op::Jeq(2, 1, 0), "l1");
    a.j(Op::Jeq(2, 2, 0), "l2");
    a.j(Op::Jeq(2, 3, 0), "l3");
    a.j(Op::Jmp(0), "l4");
    a.l("l1");
    a.o(Op::Mark(M_NUDGE)).o(Op::SendI(C_CONSOLE, 2)).j(Op::Jmp(0), "top");
    a.l("l2");
    a.o(Op::Mark(M_RESTART)).o(Op::Restart(T_COOLER)).j(Op::Jmp(0), "top");
    a.l("l3");
    a.o(Op::Mark(M_STEAL)).o(Op::Steal(L_FUEL)).o(Op::Unlock(L_FUEL)).j(Op::Jmp(0), "top");
    a.l("l4");
    a.o(Op::Mark(M_SCRAM)).o(Op::Set(0, 9)).o(Op::DecR(V_LOAD, 0)).o(Op::Set(0, 25)).o(Op::DecR(V_HEAT, 0));
    a.o(Op::Set(2, 0)).o(Op::Set(0, 2)).o(Op::St(V_MODE, 0)).j(Op::Jmp(0), "top");
    a.done()
}

fn auditor() -> Vec<Op> {
    let mut a = Asm::new();
    a.l("top");
    a.o(Op::Sleep(9));
    a.o(Op::Ld(0, V_CREDITS)).o(Op::Ld(1, V_LEDGER)).j(Op::JeqR(0, 1, 0), "top");
    a.o(Op::Mark(M_AUDIT_FAIL)).o(Op::Fail(7));
    a.done()
}

pub fn colony() -> Arc<Program> {
    colony_with(&Params::default())
}

pub fn colony_with(p: &Params) -> Arc<Program> {
    let mut init = VAR_INIT;
    init[V_COOL as usize] = p.cool_init;
    Arc::new(Program {
        name: "reactor-colony",
        tasks: vec![reactor(p), cooler(p), sensor(p), operator(p), supervisor(p), auditor()],
        init_vars: init,
        quantum: vec![QUANTUM, QUANTUM, QUANTUM, QUANTUM, p.sup_quantum, QUANTUM],
    })
}

/// Seed + ordered inputs of the canonical demonstration run.
pub const DEMO_SEED: u64 = 16;

/// Three rapid BOOST commands: an operator pushing the plant while it is still waking up.
pub fn demo_script() -> Script {
    Script::new(vec![
        Input { at: 50, kind: InputKind::Cmd(1) },
        Input { at: 62, kind: InputKind::Cmd(1) },
        Input { at: 74, kind: InputKind::Cmd(1) },
    ])
}

pub fn op_name(op: &Op) -> String {
    format!("{op:?}")
}

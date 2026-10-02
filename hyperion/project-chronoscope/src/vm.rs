//! The Chrono VM: a small, bounded, fully deterministic cooperative machine.
//!
//! It exists only to give the debugger something with real history to navigate:
//! shared mutable state, two mutexes acquired in opposite orders (deadlock), bounded
//! message queues (back-pressure), seeded decisions (forkable "fate points"), crashes,
//! supervised recovery, and a thermal invariant whose violation is a terminal
//! catastrophe. It performs no host I/O, executes no external code and every loop is
//! bounded by [`MAX_STEPS`].
//!
//! Replay contract: `(program, seed, script)` fully determines every [`StepEvent`] and
//! every intermediate [`Machine`] state. Nothing in here reads a clock, the environment
//! or an address.

use std::sync::Arc;

pub const MAX_TASKS: usize = 6;
pub const NVARS: usize = 10;
pub const NLOCKS: usize = 2;
pub const NCHAN: usize = 3;
pub const CHAN_CAP: usize = 4;
pub const NREGS: usize = 4;
/// Hard ceiling on steps in any one branch. The VM stops at this step (`Terminal::StepCap`).
pub const MAX_STEPS: u32 = 640;
pub const NONE: u32 = u32::MAX;
pub const HEAT_LIMIT: i32 = 100;
/// Instructions a task may run back-to-back before the scheduler rotates.
pub const QUANTUM: u8 = 4;

// --- named indices -----------------------------------------------------------------

pub const V_HEAT: u8 = 0;
pub const V_LOAD: u8 = 1;
pub const V_COOL: u8 = 2;
pub const V_CREDITS: u8 = 3;
pub const V_LEDGER: u8 = 4;
pub const V_TICKS: u8 = 5;
pub const V_FAULTS: u8 = 6;
pub const V_ESC: u8 = 7;
pub const V_MODE: u8 = 8;
pub const V_OUT: u8 = 9;

pub const VAR_NAMES: [&str; NVARS] = [
    "HEAT", "LOAD", "COOL", "CREDITS", "LEDGER", "TICKS", "FAULTS", "ESC", "MODE", "OUT",
];
pub const VAR_BOUNDS: [(i32, i32); NVARS] = [
    (0, 130),
    (0, 9),
    (0, 9),
    (0, 9999),
    (0, 9999),
    (0, 9999),
    (0, 99),
    (0, 9),
    (0, 2),
    (0, 99999),
];
pub const VAR_INIT: [i32; NVARS] = [35, 3, 6, 0, 0, 0, 0, 0, 0, 0];

pub const L_FUEL: u8 = 0;
pub const L_COOLANT: u8 = 1;
pub const LOCK_NAMES: [&str; NLOCKS] = ["FUEL", "COOLANT"];

pub const C_CONSOLE: u8 = 0;
pub const C_ALARM: u8 = 1;
pub const C_NOTE: u8 = 2;
pub const CHAN_NAMES: [&str; NCHAN] = ["CONSOLE", "ALARM", "NOTE"];

pub const T_REACTOR: u8 = 0;
pub const T_COOLER: u8 = 1;
pub const T_SENSOR: u8 = 2;
pub const T_OPERATOR: u8 = 3;
pub const T_SUPERVISOR: u8 = 4;
pub const T_AUDITOR: u8 = 5;
pub const TASK_NAMES: [&str; MAX_TASKS] = [
    "REACTOR",
    "COOLER",
    "SENSOR",
    "OPERATOR",
    "SUPERVISOR",
    "AUDITOR",
];
/// System pseudo-task id used for idle / input-only events.
pub const T_SYSTEM: u8 = 255;

// --- ISA ---------------------------------------------------------------------------

pub type R = u8;
pub type V = u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Nop,
    Set(R, i32),
    Add(R, i32),
    AddR(R, R),
    Mov(R, R),
    /// Non-atomic read of a shared variable into a register.
    Ld(R, V),
    /// Non-atomic write of a register into a shared variable.
    St(V, R),
    /// Atomic `var += imm` (clamped to the variable's bounds).
    Inc(V, i32),
    /// Atomic `var += reg`.
    IncR(V, R),
    /// Atomic `var -= reg`.
    DecR(V, R),
    /// Timing-jitter decision: `reg = rng % bound`. Forkable, not a landmark.
    Rnd(R, u8),
    /// Significant decision ("fate point"): same mechanics as `Rnd`, but a landmark.
    Fate(R, u8),
    Jmp(u16),
    Jz(R, u16),
    Jnz(R, u16),
    Jlt(R, i32, u16),
    Jge(R, i32, u16),
    Jeq(R, i32, u16),
    Jne(R, i32, u16),
    JeqR(R, R, u16),
    JneR(R, R, u16),
    Lock(u8),
    Unlock(u8),
    /// Privileged: forcibly take a lock from its owner (recovery tool).
    Steal(u8),
    /// Blocking send (blocks while the bounded queue is full).
    Send(u8, R),
    SendI(u8, i32),
    /// Non-blocking send; the message is dropped when the queue is full.
    Post(u8, R),
    /// Blocking receive.
    Recv(u8, R),
    /// Non-blocking receive; `reg = -1` when empty.
    TryRecv(u8, R),
    Sleep(u16),
    SleepR(R),
    /// The task crashes with a fault code (it keeps whatever locks it held).
    Fail(u8),
    /// Reset task `T` to pc 0, releasing its locks. Counts as a recovery.
    Restart(u8),
    /// `reg = 1` if the task is not crashed, else 0.
    Alive(R, u8),
    /// Semantic landmark annotation; no effect on state.
    Mark(u8),
    Halt,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub name: &'static str,
    pub tasks: Vec<Vec<Op>>,
    pub init_vars: [i32; NVARS],
    /// Per-task scheduler quantum (instructions before rotation).
    pub quantum: Vec<u8>,
}

// --- state -------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TStatus {
    Ready,
    Sleeping(u32),
    WaitLock(u8),
    WaitRecv(u8, u8),
    WaitSend(u8, i32),
    Crashed(u8),
    Halted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Msg {
    pub payload: i32,
    /// Provenance only: the step id (or input marker) that produced this message.
    pub src: u32,
}

#[derive(Clone, Debug)]
pub struct TaskState {
    pub pc: u16,
    pub regs: [i32; NREGS],
    pub status: TStatus,
    pub restarts: u8,
}

#[derive(Clone, Debug)]
pub struct LockState {
    pub owner: Option<u8>,
    pub waiters: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ChanState {
    pub q: Vec<Msg>,
}

/// Provenance sidecar: what produced each piece of state. Excluded from the
/// computational digest, included in the provenance digest.
#[derive(Clone, Debug)]
pub struct Prov {
    pub var_writer: [u32; NVARS],
    pub task_last: [u32; MAX_TASKS],
    pub lock_acq: [u32; NLOCKS],
    pub rng_last: u32,
    pub wake: [u32; MAX_TASKS],
    pub task_restart_ev: [u32; MAX_TASKS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Meltdown,
    Wedged,
    Halted,
    StepCap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// Push a command onto the CONSOLE queue.
    Cmd(u8),
    /// Replace the outcome of the decision executed exactly at `Input::at`.
    Override(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input {
    pub at: u32,
    pub kind: InputKind,
}

/// The ordered external inputs. Sorted by `at`; at most one per (step, kind-class).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub inputs: Vec<Input>,
}

impl Script {
    pub fn new(mut inputs: Vec<Input>) -> Script {
        inputs.sort_by_key(|i| (i.at, matches!(i.kind, InputKind::Override(_))));
        Script { inputs }
    }
    pub fn cmd_at(&self, step: u32) -> Option<(usize, u8)> {
        self.inputs
            .iter()
            .enumerate()
            .find_map(|(i, x)| match x.kind {
                InputKind::Cmd(c) if x.at == step => Some((i, c)),
                _ => None,
            })
    }
    pub fn override_at(&self, step: u32) -> Option<i32> {
        self.inputs.iter().find_map(|x| match x.kind {
            InputKind::Override(v) if x.at == step => Some(v),
            _ => None,
        })
    }
    pub fn has_future(&self, from: u32) -> bool {
        self.inputs.iter().any(|i| i.at >= from)
    }
    /// A copy with every input at `step` or later removed (the common prefix of a fork).
    pub fn prefix_before(&self, step: u32) -> Script {
        Script {
            inputs: self
                .inputs
                .iter()
                .copied()
                .filter(|i| i.at < step)
                .collect(),
        }
    }
}

// --- events ------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Class {
    Idle,
    Compute,
    Read,
    Write,
    Rmw,
    Decision,
    Fate,
    LockAcq,
    LockWait,
    LockRel,
    LockSteal,
    Send,
    SendWait,
    Recv,
    RecvWait,
    Sleep,
    Fail,
    Restart,
    Mark,
    Halt,
    Meltdown,
}

pub const F_INPUT: u16 = 1;
pub const F_CRASH: u16 = 2;
pub const F_CATASTROPHE: u16 = 4;
pub const F_OVERRIDDEN: u16 = 8;
pub const F_DROPPED: u16 = 16;
pub const F_HANDOFF: u16 = 32;

/// One executed step. Exactly one event is produced per global step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepEvent {
    pub step: u32,
    pub task: u8,
    pub pc: u16,
    pub class: Class,
    /// Primary object: variable / lock / channel / target task, or 255.
    pub obj: u8,
    pub before: i32,
    pub after: i32,
    /// Decision value / message payload / mark tag.
    pub aux: i32,
    pub flags: u16,
    /// 0 program order, 1 data/lock/message/rng source, 2 wake cause, 3 auxiliary.
    pub parents: [u32; 4],
}

impl StepEvent {
    fn blank(step: u32, task: u8, pc: u16, class: Class) -> StepEvent {
        StepEvent {
            step,
            task,
            pc,
            class,
            obj: 255,
            before: 0,
            after: 0,
            aux: 0,
            flags: 0,
            parents: [NONE; 4],
        }
    }
    pub fn is_decision(&self) -> bool {
        matches!(self.class, Class::Decision | Class::Fate)
    }
    pub fn is_fate(&self) -> bool {
        self.class == Class::Fate
    }
}

// --- machine -----------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Machine {
    prog: Arc<Program>,
    pub seed: u64,
    pub step: u32,
    pub rng: u64,
    pub rr: u8,
    /// Task currently holding the CPU quantum (255 = none) and its remaining slots.
    pub cur: u8,
    pub quantum_left: u8,
    pub vars: [i32; NVARS],
    pub tasks: Vec<TaskState>,
    pub locks: [LockState; NLOCKS],
    pub chans: [ChanState; NCHAN],
    pub prov: Prov,
    pub terminal: Option<Terminal>,
}

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

struct Fnv(u64);
impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn u(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn i(&mut self, v: i32) {
        self.u(v as u32 as u64);
    }
}

impl Machine {
    pub fn new(prog: Arc<Program>, seed: u64) -> Machine {
        let n = prog.tasks.len();
        let prog_init = prog.init_vars;
        let tasks = (0..n)
            .map(|_| TaskState {
                pc: 0,
                regs: [0; NREGS],
                status: TStatus::Ready,
                restarts: 0,
            })
            .collect();
        Machine {
            prog,
            seed,
            step: 0,
            rng: splitmix(seed) | 1,
            rr: 0,
            cur: 255,
            quantum_left: 0,
            vars: prog_init,
            tasks,
            locks: [
                LockState {
                    owner: None,
                    waiters: vec![],
                },
                LockState {
                    owner: None,
                    waiters: vec![],
                },
            ],
            chans: [
                ChanState { q: vec![] },
                ChanState { q: vec![] },
                ChanState { q: vec![] },
            ],
            prov: Prov {
                var_writer: [NONE; NVARS],
                task_last: [NONE; MAX_TASKS],
                lock_acq: [NONE; NLOCKS],
                rng_last: NONE,
                wake: [NONE; MAX_TASKS],
                task_restart_ev: [NONE; MAX_TASKS],
            },
            terminal: None,
        }
    }

    pub fn program(&self) -> &Arc<Program> {
        &self.prog
    }

    fn next_rng(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn clamp(v: u8, x: i32) -> i32 {
        let (lo, hi) = VAR_BOUNDS[v as usize];
        x.clamp(lo, hi)
    }

    /// Computational digest: everything that determines future *behaviour*.
    pub fn digest_computational(&self) -> u64 {
        let mut h = Fnv::new();
        h.u(self.step as u64);
        h.u(self.rng);
        h.u(self.rr as u64);
        h.u(self.cur as u64);
        h.u(self.quantum_left as u64);
        for v in self.vars {
            h.i(v);
        }
        for t in &self.tasks {
            h.u(t.pc as u64);
            for r in t.regs {
                h.i(r);
            }
            h.u(t.restarts as u64);
            match t.status {
                TStatus::Ready => h.u(1),
                TStatus::Sleeping(w) => {
                    h.u(2);
                    h.u(w as u64)
                }
                TStatus::WaitLock(l) => {
                    h.u(3);
                    h.u(l as u64)
                }
                TStatus::WaitRecv(c, r) => {
                    h.u(4);
                    h.u(c as u64);
                    h.u(r as u64)
                }
                TStatus::WaitSend(c, p) => {
                    h.u(5);
                    h.u(c as u64);
                    h.i(p)
                }
                TStatus::Crashed(c) => {
                    h.u(6);
                    h.u(c as u64)
                }
                TStatus::Halted => h.u(7),
            }
        }
        for l in &self.locks {
            h.u(l.owner.map(|o| o as u64 + 1).unwrap_or(0));
            h.u(l.waiters.len() as u64);
            for w in &l.waiters {
                h.u(*w as u64);
            }
        }
        for c in &self.chans {
            h.u(c.q.len() as u64);
            for m in &c.q {
                h.i(m.payload);
            }
        }
        h.u(match self.terminal {
            None => 0,
            Some(Terminal::Meltdown) => 1,
            Some(Terminal::Wedged) => 2,
            Some(Terminal::Halted) => 3,
            Some(Terminal::StepCap) => 4,
        });
        h.0
    }

    /// Provenance digest: *why* the state holds its values.
    pub fn digest_provenance(&self) -> u64 {
        let mut h = Fnv::new();
        for x in self.prov.var_writer {
            h.u(x as u64);
        }
        for x in self.prov.task_last {
            h.u(x as u64);
        }
        for x in self.prov.lock_acq {
            h.u(x as u64);
        }
        h.u(self.prov.rng_last as u64);
        for x in self.prov.wake {
            h.u(x as u64);
        }
        for x in self.prov.task_restart_ev {
            h.u(x as u64);
        }
        for c in &self.chans {
            for m in &c.q {
                h.u(m.src as u64);
            }
        }
        h.0
    }

    /// Whole-machine digest: "byte-identical internal state" means this matches.
    pub fn digest_full(&self) -> u64 {
        let mut h = Fnv::new();
        h.u(self.digest_computational());
        h.u(self.digest_provenance());
        h.0
    }

    /// Tasks currently participating in a wait-for cycle over locks.
    pub fn deadlocked_tasks(&self) -> Vec<u8> {
        let n = self.tasks.len();
        let waits_on = |t: usize| -> Option<usize> {
            if let TStatus::WaitLock(l) = self.tasks[t].status {
                self.locks[l as usize].owner.map(|o| o as usize)
            } else {
                None
            }
        };
        let mut out = vec![];
        for start in 0..n {
            let mut cur = start;
            let mut seen = 0u32;
            let mut cyc = false;
            for _ in 0..=n {
                match waits_on(cur) {
                    Some(nx) => {
                        if nx == start {
                            cyc = true;
                            break;
                        }
                        if seen & (1 << nx) != 0 {
                            break;
                        }
                        seen |= 1 << nx;
                        cur = nx;
                    }
                    None => break,
                }
            }
            if cyc {
                out.push(start as u8);
            }
        }
        out
    }

    fn ready_pick(&mut self) -> Option<usize> {
        let n = self.tasks.len();
        for k in 0..n {
            let i = (self.rr as usize + k) % n;
            if self.tasks[i].status == TStatus::Ready {
                self.rr = ((i + 1) % n) as u8;
                return Some(i);
            }
        }
        None
    }

    fn release_lock(&mut self, k: usize, ev_step: u32) -> bool {
        // hand the lock to the head waiter, if any; returns true when handed off
        self.locks[k].owner = None;
        if !self.locks[k].waiters.is_empty() {
            let w = self.locks[k].waiters.remove(0);
            self.locks[k].owner = Some(w);
            self.prov.lock_acq[k] = ev_step;
            self.tasks[w as usize].status = TStatus::Ready;
            self.tasks[w as usize].pc += 1; // the Lock op completes
            self.prov.wake[w as usize] = ev_step;
            true
        } else {
            self.prov.lock_acq[k] = NONE;
            false
        }
    }

    fn deliver_to_waiter(&mut self, ch: usize, ev_step: u32) {
        // A queue slot opened or a message arrived: satisfy blocked receivers / senders.
        loop {
            let mut progressed = false;
            if !self.chans[ch].q.is_empty() {
                if let Some(t) = (0..self.tasks.len())
                    .find(|&t| matches!(self.tasks[t].status, TStatus::WaitRecv(c, _) if c as usize == ch))
                {
                    if let TStatus::WaitRecv(_, r) = self.tasks[t].status {
                        let m = self.chans[ch].q.remove(0);
                        self.tasks[t].regs[r as usize] = m.payload;
                        self.tasks[t].status = TStatus::Ready;
                        self.tasks[t].pc += 1;
                        self.prov.wake[t] = ev_step;
                        // remember message provenance as an extra wake cause via task_last chain
                        progressed = true;
                    }
                }
            }
            if self.chans[ch].q.len() < CHAN_CAP {
                if let Some(t) = (0..self.tasks.len())
                    .find(|&t| matches!(self.tasks[t].status, TStatus::WaitSend(c, _) if c as usize == ch))
                {
                    if let TStatus::WaitSend(_, p) = self.tasks[t].status {
                        self.chans[ch].q.push(Msg { payload: p, src: self.prov.task_last[t] });
                        self.tasks[t].status = TStatus::Ready;
                        self.tasks[t].pc += 1;
                        self.prov.wake[t] = ev_step;
                        progressed = true;
                    }
                }
            }
            if !progressed {
                break;
            }
        }
    }

    fn crash_release(&mut self, t: usize, ev_step: u32) {
        // Restart: drop held locks (handing them off) and any wait registrations.
        for k in 0..NLOCKS {
            self.locks[k].waiters.retain(|w| *w as usize != t);
            if self.locks[k].owner == Some(t as u8) {
                self.release_lock(k, ev_step);
            }
        }
    }

    /// Execute exactly one global step. Returns `None` once the machine is terminal.
    pub fn step(&mut self, script: &Script) -> Option<StepEvent> {
        if self.terminal.is_some() {
            return None;
        }
        let s = self.step;
        if s >= MAX_STEPS {
            self.terminal = Some(Terminal::StepCap);
            return None;
        }
        let mut flags: u16 = 0;
        let mut input_src = NONE;

        // 1. external input (applied before any task runs this step)
        if let Some((_, cmd)) = script.cmd_at(s) {
            flags |= F_INPUT;
            // the injection step names the input: stable under unrelated script edits
            input_src = 0x8000_0000 | s;
            let ch = C_CONSOLE as usize;
            if self.chans[ch].q.len() < CHAN_CAP {
                self.chans[ch].q.push(Msg {
                    payload: cmd as i32,
                    src: input_src,
                });
                self.deliver_to_waiter(ch, s);
            } else {
                flags |= F_DROPPED;
            }
        }
        // 2. wake sleepers
        for t in 0..self.tasks.len() {
            if let TStatus::Sleeping(w) = self.tasks[t].status {
                if w <= s {
                    self.tasks[t].status = TStatus::Ready;
                    self.tasks[t].pc += 1;
                    self.prov.wake[t] = NONE;
                }
            }
        }
        // 3. schedule
        let pick = if self.cur != 255
            && self.quantum_left > 0
            && self.tasks[self.cur as usize].status == TStatus::Ready
        {
            self.quantum_left -= 1;
            Some(self.cur as usize)
        } else {
            self.quantum_left = 0;
            let p = self.ready_pick();
            match p {
                Some(t) => {
                    self.cur = t as u8;
                    self.quantum_left =
                        self.prog.quantum.get(t).copied().unwrap_or(QUANTUM).max(1) - 1;
                }
                None => self.cur = 255,
            }
            p
        };
        let ev = match pick {
            None => {
                let sleepers = self
                    .tasks
                    .iter()
                    .any(|t| matches!(t.status, TStatus::Sleeping(_)));
                if !sleepers && !script.has_future(s + 1) && flags & F_INPUT == 0 {
                    self.terminal = Some(Terminal::Wedged);
                }
                let mut e = StepEvent::blank(s, T_SYSTEM, 0, Class::Idle);
                e.flags = flags;
                if flags & F_INPUT != 0 {
                    e.parents[1] = input_src;
                }
                e
            }
            Some(t) => self.exec(t, s, flags, input_src, script),
        };
        self.step += 1;
        // 4. invariant: thermal runaway is terminal
        if self.vars[V_HEAT as usize] >= HEAT_LIMIT && self.terminal.is_none() {
            self.terminal = Some(Terminal::Meltdown);
            let mut e = ev;
            e.flags |= F_CATASTROPHE;
            return Some(e);
        }
        Some(ev)
    }

    fn jump(&mut self, t: usize, target: u16) {
        self.tasks[t].pc = target;
    }

    fn exec(
        &mut self,
        t: usize,
        s: u32,
        mut flags: u16,
        input_src: u32,
        script: &Script,
    ) -> StepEvent {
        let pc = self.tasks[t].pc;
        let code = &self.prog.tasks[t];
        let op = code.get(pc as usize).copied().unwrap_or(Op::Halt);
        let mut ev = StepEvent::blank(s, t as u8, pc, Class::Compute);
        ev.parents[0] = self.prov.task_last[t];
        ev.parents[2] = std::mem::replace(&mut self.prov.wake[t], NONE);
        if flags & F_INPUT != 0 {
            ev.parents[3] = input_src;
        }
        let mut next_pc = Some(pc + 1);
        let reg = |m: &Machine, r: R| m.tasks[t].regs[r as usize % NREGS];
        match op {
            Op::Nop => {}
            Op::Set(r, i) => {
                ev.before = reg(self, r);
                self.tasks[t].regs[r as usize] = i;
                ev.after = i;
            }
            Op::Add(r, i) => {
                ev.before = reg(self, r);
                let v = reg(self, r).wrapping_add(i);
                self.tasks[t].regs[r as usize] = v;
                ev.after = v;
            }
            Op::AddR(a, b) => {
                ev.before = reg(self, a);
                let v = reg(self, a).wrapping_add(reg(self, b));
                self.tasks[t].regs[a as usize] = v;
                ev.after = v;
            }
            Op::Mov(a, b) => {
                ev.before = reg(self, a);
                let v = reg(self, b);
                self.tasks[t].regs[a as usize] = v;
                ev.after = v;
            }
            Op::Ld(r, v) => {
                ev.class = Class::Read;
                ev.obj = v;
                ev.before = reg(self, r);
                let x = self.vars[v as usize];
                self.tasks[t].regs[r as usize] = x;
                ev.after = x;
                ev.parents[1] = self.prov.var_writer[v as usize];
            }
            Op::St(v, r) => {
                ev.class = Class::Write;
                ev.obj = v;
                ev.before = self.vars[v as usize];
                let x = Self::clamp(v, reg(self, r));
                self.vars[v as usize] = x;
                ev.after = x;
                ev.parents[1] = self.prov.var_writer[v as usize];
                self.prov.var_writer[v as usize] = s;
            }
            Op::Inc(v, i) => {
                ev.class = Class::Rmw;
                ev.obj = v;
                ev.before = self.vars[v as usize];
                let x = Self::clamp(v, self.vars[v as usize].saturating_add(i));
                self.vars[v as usize] = x;
                ev.after = x;
                ev.parents[1] = self.prov.var_writer[v as usize];
                self.prov.var_writer[v as usize] = s;
            }
            Op::IncR(v, r) | Op::DecR(v, r) => {
                ev.class = Class::Rmw;
                ev.obj = v;
                ev.before = self.vars[v as usize];
                let d = reg(self, r);
                let d = if matches!(op, Op::DecR(..)) { -d } else { d };
                let x = Self::clamp(v, self.vars[v as usize].saturating_add(d));
                self.vars[v as usize] = x;
                ev.after = x;
                ev.parents[1] = self.prov.var_writer[v as usize];
                self.prov.var_writer[v as usize] = s;
            }
            Op::Rnd(r, b) | Op::Fate(r, b) => {
                ev.class = if matches!(op, Op::Fate(..)) {
                    Class::Fate
                } else {
                    Class::Decision
                };
                let b = b.max(1) as u64;
                let natural = ((self.next_rng() >> 33) % b) as i32;
                let v = match script.override_at(s) {
                    Some(o) => {
                        flags |= F_OVERRIDDEN;
                        o.rem_euclid(b as i32)
                    }
                    None => natural,
                };
                ev.before = natural; // what the dice said
                ev.after = v; // what happened
                ev.aux = b as i32;
                self.tasks[t].regs[r as usize] = v;
                ev.parents[1] = self.prov.rng_last;
                self.prov.rng_last = s;
            }
            Op::Jmp(l) => next_pc = Some(l),
            Op::Jz(r, l) => {
                if reg(self, r) == 0 {
                    next_pc = Some(l)
                }
            }
            Op::Jnz(r, l) => {
                if reg(self, r) != 0 {
                    next_pc = Some(l)
                }
            }
            Op::Jlt(r, i, l) => {
                if reg(self, r) < i {
                    next_pc = Some(l)
                }
            }
            Op::Jge(r, i, l) => {
                if reg(self, r) >= i {
                    next_pc = Some(l)
                }
            }
            Op::Jeq(r, i, l) => {
                if reg(self, r) == i {
                    next_pc = Some(l)
                }
            }
            Op::Jne(r, i, l) => {
                if reg(self, r) != i {
                    next_pc = Some(l)
                }
            }
            Op::JeqR(a, b, l) => {
                if reg(self, a) == reg(self, b) {
                    next_pc = Some(l)
                }
            }
            Op::JneR(a, b, l) => {
                if reg(self, a) != reg(self, b) {
                    next_pc = Some(l)
                }
            }
            Op::Lock(k) => {
                let ki = k as usize;
                ev.obj = k;
                match self.locks[ki].owner {
                    None => {
                        ev.class = Class::LockAcq;
                        self.locks[ki].owner = Some(t as u8);
                        ev.parents[1] = self.prov.lock_acq[ki]; // NONE unless previously held
                        self.prov.lock_acq[ki] = s;
                    }
                    Some(o) => {
                        ev.class = Class::LockWait;
                        ev.aux = o as i32;
                        ev.parents[1] = self.prov.lock_acq[ki];
                        self.locks[ki].waiters.push(t as u8);
                        self.tasks[t].status = TStatus::WaitLock(k);
                        next_pc = None; // pc advanced by the hand-off
                    }
                }
            }
            Op::Unlock(k) => {
                let ki = k as usize;
                ev.class = Class::LockRel;
                ev.obj = k;
                ev.parents[1] = self.prov.lock_acq[ki];
                if self.locks[ki].owner == Some(t as u8) && self.release_lock(ki, s) {
                    ev.flags |= F_HANDOFF;
                }
            }
            Op::Steal(k) => {
                let ki = k as usize;
                ev.class = Class::LockSteal;
                ev.obj = k;
                ev.parents[1] = self.prov.lock_acq[ki];
                if let Some(o) = self.locks[ki].owner {
                    if o as usize != t {
                        ev.aux = o as i32;
                        // the victim loses the lock; it is re-queued behind the next waiter
                        self.release_lock(ki, s);
                        if self.locks[ki].owner.is_none() {
                            self.locks[ki].owner = Some(t as u8);
                            self.prov.lock_acq[ki] = s;
                        } else {
                            // ownership went to a waiter: thief queues behind them
                            self.locks[ki].waiters.push(t as u8);
                            self.tasks[t].status = TStatus::WaitLock(k);
                            next_pc = None;
                        }
                    }
                }
            }
            Op::Send(c, r) => {
                ev.obj = c;
                ev.class = Class::Send;
                let p = reg(self, r);
                ev.aux = p;
                if self.chans[c as usize].q.len() < CHAN_CAP {
                    self.chans[c as usize].q.push(Msg { payload: p, src: s });
                    self.deliver_to_waiter(c as usize, s);
                } else {
                    ev.class = Class::SendWait;
                    self.tasks[t].status = TStatus::WaitSend(c, p);
                    next_pc = None;
                }
            }
            Op::SendI(c, p) => {
                ev.obj = c;
                ev.class = Class::Send;
                ev.aux = p;
                if self.chans[c as usize].q.len() < CHAN_CAP {
                    self.chans[c as usize].q.push(Msg { payload: p, src: s });
                    self.deliver_to_waiter(c as usize, s);
                } else {
                    ev.class = Class::SendWait;
                    self.tasks[t].status = TStatus::WaitSend(c, p);
                    next_pc = None;
                }
            }
            Op::Post(c, r) => {
                ev.obj = c;
                ev.class = Class::Send;
                let p = reg(self, r);
                ev.aux = p;
                if self.chans[c as usize].q.len() < CHAN_CAP {
                    self.chans[c as usize].q.push(Msg { payload: p, src: s });
                    self.deliver_to_waiter(c as usize, s);
                } else {
                    ev.flags |= F_DROPPED;
                }
            }
            Op::Recv(c, r) => {
                ev.obj = c;
                if !self.chans[c as usize].q.is_empty() {
                    ev.class = Class::Recv;
                    let m = self.chans[c as usize].q.remove(0);
                    self.tasks[t].regs[r as usize] = m.payload;
                    ev.aux = m.payload;
                    ev.parents[1] = m.src;
                    self.deliver_to_waiter(c as usize, s);
                } else {
                    ev.class = Class::RecvWait;
                    self.tasks[t].status = TStatus::WaitRecv(c, r);
                    next_pc = None;
                }
            }
            Op::TryRecv(c, r) => {
                ev.obj = c;
                ev.class = Class::Recv;
                if !self.chans[c as usize].q.is_empty() {
                    let m = self.chans[c as usize].q.remove(0);
                    self.tasks[t].regs[r as usize] = m.payload;
                    ev.aux = m.payload;
                    ev.parents[1] = m.src;
                    self.deliver_to_waiter(c as usize, s);
                } else {
                    self.tasks[t].regs[r as usize] = -1;
                    ev.aux = -1;
                }
            }
            Op::Sleep(n) => {
                ev.class = Class::Sleep;
                ev.aux = n as i32;
                self.tasks[t].status = TStatus::Sleeping(s + n.max(1) as u32);
                next_pc = None;
            }
            Op::SleepR(r) => {
                ev.class = Class::Sleep;
                let n = reg(self, r).clamp(1, 64) as u32;
                ev.aux = n as i32;
                self.tasks[t].status = TStatus::Sleeping(s + n);
                next_pc = None;
            }
            Op::Fail(code) => {
                ev.class = Class::Fail;
                ev.aux = code as i32;
                ev.flags |= F_CRASH;
                self.tasks[t].status = TStatus::Crashed(code);
                let f = self.vars[V_FAULTS as usize];
                self.vars[V_FAULTS as usize] = Self::clamp(V_FAULTS, f + 1);
                ev.obj = V_FAULTS;
                ev.before = f;
                ev.after = self.vars[V_FAULTS as usize];
                self.prov.var_writer[V_FAULTS as usize] = s;
                next_pc = None;
            }
            Op::Restart(target) => {
                ev.class = Class::Restart;
                ev.obj = target;
                let ti = target as usize % self.tasks.len();
                ev.parents[1] = self.prov.task_last[ti];
                ev.parents[3] = self.prov.task_restart_ev[ti];
                self.crash_release(ti, s);
                let was = self.tasks[ti].status;
                let restarts = self.tasks[ti].restarts.saturating_add(1);
                self.tasks[ti] = TaskState {
                    pc: 0,
                    regs: [0; NREGS],
                    status: TStatus::Ready,
                    restarts,
                };
                self.prov.task_restart_ev[ti] = s;
                self.prov.wake[ti] = s;
                if ti == t {
                    next_pc = None;
                }
                ev.aux = matches!(was, TStatus::Crashed(_)) as i32;
            }
            Op::Alive(r, target) => {
                ev.class = Class::Read;
                ev.obj = 200 + target;
                let ti = target as usize % self.tasks.len();
                let alive = !matches!(self.tasks[ti].status, TStatus::Crashed(_)) as i32;
                ev.before = reg(self, r);
                self.tasks[t].regs[r as usize] = alive;
                ev.after = alive;
                ev.parents[1] = self.prov.task_last[ti];
            }
            Op::Mark(tag) => {
                ev.class = Class::Mark;
                ev.aux = tag as i32;
            }
            Op::Halt => {
                ev.class = Class::Halt;
                self.tasks[t].status = TStatus::Halted;
                next_pc = None;
            }
        }
        // control-flow ops keep Class::Compute; a jump taken is a pc change only
        if let Some(npc) = next_pc {
            // jumps replaced next_pc above; plain ops advance by one
            self.jump(t, npc);
        }
        ev.flags |= flags;
        self.prov.task_last[t] = s;
        ev
    }
}

/// Convenience: run `n` steps (or until terminal) collecting events.
pub fn run_steps(m: &mut Machine, script: &Script, n: u32) -> Vec<StepEvent> {
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        match m.step(script) {
            Some(e) => v.push(e),
            None => break,
        }
    }
    v
}

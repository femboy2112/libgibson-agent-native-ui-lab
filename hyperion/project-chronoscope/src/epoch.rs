//! Semantic epochs: the high-level "what kind of moment is this" classification that
//! drives ribbon colour, landmarks and the HumanMusic trace.

use crate::fixture::*;
use crate::vm::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Epoch {
    Stable = 0,
    Uncertain = 1,
    Escalating = 2,
    Deadlock = 3,
    Contradiction = 4,
    Resolution = 5,
    Convergence = 6,
    Catastrophe = 7,
}

pub const ALL_EPOCHS: [Epoch; 8] = [
    Epoch::Stable,
    Epoch::Uncertain,
    Epoch::Escalating,
    Epoch::Deadlock,
    Epoch::Contradiction,
    Epoch::Resolution,
    Epoch::Convergence,
    Epoch::Catastrophe,
];

impl Epoch {
    pub fn from_u8(v: u8) -> Epoch {
        ALL_EPOCHS[(v as usize).min(7)]
    }
    pub fn name(self) -> &'static str {
        match self {
            Epoch::Stable => "stable progression",
            Epoch::Uncertain => "uncertainty",
            Epoch::Escalating => "recursive escalation",
            Epoch::Deadlock => "deadlock",
            Epoch::Contradiction => "contradiction",
            Epoch::Resolution => "resolution",
            Epoch::Convergence => "convergence",
            Epoch::Catastrophe => "catastrophe",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Epoch::Stable => "STABLE",
            Epoch::Uncertain => "UNCERTAIN",
            Epoch::Escalating => "ESCALATE",
            Epoch::Deadlock => "DEADLOCK",
            Epoch::Contradiction => "CONTRADICT",
            Epoch::Resolution => "RESOLVE",
            Epoch::Convergence => "CONVERGE",
            Epoch::Catastrophe => "CATASTROPHE",
        }
    }
    /// One ASCII letter, used where colour is unavailable.
    pub fn letter(self) -> char {
        match self {
            Epoch::Stable => 's',
            Epoch::Uncertain => '?',
            Epoch::Escalating => '^',
            Epoch::Deadlock => 'D',
            Epoch::Contradiction => 'X',
            Epoch::Resolution => 'r',
            Epoch::Convergence => '=',
            Epoch::Catastrophe => '!',
        }
    }
    /// RGB used for the ribbon (TrueColor path; the renderer quantizes).
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Epoch::Stable => (60, 200, 255),
            Epoch::Uncertain => (240, 210, 90),
            Epoch::Escalating => (255, 140, 40),
            Epoch::Deadlock => (170, 110, 255),
            Epoch::Contradiction => (255, 70, 200),
            Epoch::Resolution => (90, 240, 150),
            Epoch::Convergence => (235, 250, 255),
            Epoch::Catastrophe => (255, 50, 50),
        }
    }
}

/// Streaming classifier. Cloneable so checkpoints can carry it; a pure function of the
/// event/state stream it has observed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochTracker {
    pub cur: Epoch,
    cand: Epoch,
    cand_n: u8,
    last_rare: i64,
    last_recovery: i64,
    was_bad: bool,
    mismatch_run: u32,
}

impl Default for EpochTracker {
    fn default() -> Self {
        EpochTracker::new()
    }
}

const PERSIST: u8 = 3;
const RESOLVE_WINDOW: i64 = 12;
const UNCERTAIN_WINDOW: i64 = 8;
const MISMATCH_PERSIST: u32 = 14;

pub fn is_recovery_event(ev: &StepEvent) -> bool {
    match ev.class {
        Class::Restart | Class::LockSteal => true,
        Class::Mark => matches!(ev.aux as u8, M_RECONCILE | M_SCRAM | M_RESTART | M_STEAL),
        _ => false,
    }
}

impl EpochTracker {
    pub fn new() -> EpochTracker {
        EpochTracker {
            cur: Epoch::Stable,
            cand: Epoch::Stable,
            cand_n: 0,
            last_rare: -1000,
            last_recovery: -1000,
            was_bad: false,
            mismatch_run: 0,
        }
    }

    /// Observe the state *after* `ev` executed.
    pub fn observe(&mut self, m: &Machine, ev: &StepEvent) -> Epoch {
        let s = ev.step as i64;
        if ev.class == Class::Fate && ev.after == 0 {
            self.last_rare = s;
        }
        if is_recovery_event(ev) {
            self.last_recovery = s;
        }
        let heat = m.vars[V_HEAT as usize];
        let dead = !m.deadlocked_tasks().is_empty();
        let crashed = m.tasks.iter().any(|t| matches!(t.status, TStatus::Crashed(_)));
        if m.vars[V_CREDITS as usize] != m.vars[V_LEDGER as usize] {
            self.mismatch_run += 1;
        } else {
            self.mismatch_run = 0;
        }
        // a ledger mismatch only counts once it outlives the (legitimate) two-step update window
        let contradiction = self.mismatch_run >= MISMATCH_PERSIST || crashed;
        let escal = m.vars[V_ESC as usize] >= 2 || heat >= 70;
        let bad = dead || contradiction || escal;
        if self.was_bad && !bad {
            self.last_recovery = s;
        }
        self.was_bad = bad;

        let raw = if m.terminal == Some(Terminal::Meltdown) || ev.flags & F_CATASTROPHE != 0 {
            Epoch::Catastrophe
        } else if dead {
            Epoch::Deadlock
        } else if contradiction {
            Epoch::Contradiction
        } else if escal {
            Epoch::Escalating
        } else if s - self.last_recovery <= RESOLVE_WINDOW {
            Epoch::Resolution
        } else if s - self.last_rare <= UNCERTAIN_WINDOW || heat >= 50 {
            Epoch::Uncertain
        } else {
            Epoch::Stable
        };

        let immediate = matches!(raw, Epoch::Catastrophe | Epoch::Deadlock)
            || matches!(self.cur, Epoch::Deadlock | Epoch::Catastrophe);
        if raw == self.cur {
            self.cand = raw;
            self.cand_n = 0;
        } else if immediate {
            self.cur = raw;
            self.cand = raw;
            self.cand_n = 0;
        } else {
            if raw == self.cand {
                self.cand_n += 1;
            } else {
                self.cand = raw;
                self.cand_n = 1;
            }
            if self.cand_n >= PERSIST {
                self.cur = raw;
                self.cand_n = 0;
            }
        }
        self.cur
    }
}

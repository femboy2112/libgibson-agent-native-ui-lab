//! Semantic projection and comparison dimensions.
//!
//! Two machine states are *byte-identical* only when [`Machine::digest_full`] matches.
//! They are *semantically equivalent* when every dimension's declared class matches. The
//! projection is a stated abstraction, not a theorem: it ignores bookkeeping counters
//! (TICKS, OUT, LEDGER's raw value) and scheduler position, and it can say "equivalent"
//! for states whose futures later diverge. The UI only ever says "≡ identical" when the
//! full digest matches, and "≈ equivalent under projection" otherwise.

use crate::vm::*;

pub const NDIM: usize = NVARS + MAX_TASKS + NLOCKS + NCHAN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimKind {
    Var(u8),
    Task(u8),
    Lock(u8),
    Chan(u8),
}

pub fn dim_kind(i: usize) -> DimKind {
    if i < NVARS {
        DimKind::Var(i as u8)
    } else if i < NVARS + MAX_TASKS {
        DimKind::Task((i - NVARS) as u8)
    } else if i < NVARS + MAX_TASKS + NLOCKS {
        DimKind::Lock((i - NVARS - MAX_TASKS) as u8)
    } else {
        DimKind::Chan((i - NVARS - MAX_TASKS - NLOCKS) as u8)
    }
}

pub fn dim_name(i: usize) -> String {
    match dim_kind(i) {
        DimKind::Var(v) => VAR_NAMES[v as usize].to_string(),
        DimKind::Task(t) => format!("task:{}", TASK_NAMES[t as usize]),
        DimKind::Lock(l) => format!("lock:{}", LOCK_NAMES[l as usize]),
        DimKind::Chan(c) => format!("chan:{}", CHAN_NAMES[c as usize]),
    }
}

/// Short label (for narrow lanes).
pub fn dim_short(i: usize) -> String {
    match dim_kind(i) {
        DimKind::Var(v) => VAR_NAMES[v as usize].to_string(),
        DimKind::Task(t) => TASK_NAMES[t as usize][..3].to_string(),
        DimKind::Lock(l) => format!("L{}", &LOCK_NAMES[l as usize][..1]),
        DimKind::Chan(c) => format!("Q{}", &CHAN_NAMES[c as usize][..1]),
    }
}

/// Dimensions that are bookkeeping, excluded from the semantic projection.
pub fn dim_is_semantic(i: usize) -> bool {
    !matches!(
        dim_kind(i),
        DimKind::Var(V_TICKS) | DimKind::Var(V_OUT) | DimKind::Var(V_LEDGER)
    )
}

fn task_code(s: TStatus) -> (i32, i32) {
    // (raw discriminant, class)
    match s {
        TStatus::Ready => (0, 0),
        TStatus::Sleeping(_) => (1, 0),
        TStatus::WaitRecv(..) => (2, 0),
        TStatus::WaitSend(..) => (3, 1),
        TStatus::WaitLock(_) => (4, 1),
        TStatus::Crashed(_) => (5, 2),
        TStatus::Halted => (6, 3),
    }
}

pub fn heat_band(h: i32) -> i32 {
    match h {
        i32::MIN..=29 => 0,
        30..=54 => 1,
        55..=74 => 2,
        75..=94 => 3,
        _ => 4,
    }
}

fn load_band(l: i32) -> i32 {
    match l {
        i32::MIN..=1 => 0,
        2..=4 => 1,
        5..=7 => 2,
        _ => 3,
    }
}

/// `(raw, class)` for every dimension of a machine state.
#[allow(clippy::needless_range_loop)]
pub fn dims(m: &Machine) -> [(i32, i32); NDIM] {
    let mut out = [(0, 0); NDIM];
    for v in 0..NVARS {
        let raw = m.vars[v];
        let class = match v as u8 {
            V_HEAT => heat_band(raw),
            V_LOAD => load_band(raw),
            V_COOL => match raw {
                i32::MIN..=2 => 0,
                3..=5 => 1,
                _ => 2,
            },
            V_CREDITS => (m.vars[V_CREDITS as usize] != m.vars[V_LEDGER as usize]) as i32,
            V_FAULTS => (raw > 0) as i32,
            V_ESC => match raw {
                i32::MIN..=0 => 0,
                1..=2 => 1,
                _ => 2,
            },
            V_MODE => raw,
            _ => 0, // LEDGER, TICKS, OUT: bookkeeping
        };
        out[v] = (raw, class);
    }
    for t in 0..MAX_TASKS {
        if let Some(ts) = m.tasks.get(t) {
            let (d, c) = task_code(ts.status);
            out[NVARS + t] = (d * 1000 + ts.pc as i32, c);
        }
    }
    for l in 0..NLOCKS {
        let owner = m.locks[l].owner.map(|o| o as i32 + 1).unwrap_or(0);
        out[NVARS + MAX_TASKS + l] = (
            owner * 10 + m.locks[l].waiters.len() as i32,
            (owner != 0) as i32,
        );
    }
    for c in 0..NCHAN {
        let n = m.chans[c].q.len() as i32;
        out[NVARS + MAX_TASKS + NLOCKS + c] = (n, (n >= 2) as i32);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel {
    /// Raw values equal.
    Same,
    /// Raw differ, class equal: equivalent under the projection.
    Equiv,
    /// Classes differ.
    Apart,
}

pub fn relate(a: (i32, i32), b: (i32, i32), semantic: bool) -> Rel {
    if a.0 == b.0 {
        Rel::Same
    } else if !semantic || a.1 == b.1 {
        Rel::Equiv
    } else {
        Rel::Apart
    }
}

/// Packed semantic class vector of a state (what "semantically equal" compares).
pub fn sem_key(d: &[(i32, i32); NDIM]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, (_, c)) in d.iter().enumerate() {
        if dim_is_semantic(i) {
            h ^= (*c as u64) + 1 + ((i as u64) << 8);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// `digest_full` equal: byte-identical internal state.
    Identical,
    /// Computational digest equal, provenance differs.
    ComputationallyEqual,
    /// Every semantic class equal, computational state differs.
    Equivalent,
    Divergent,
}

pub fn verdict(
    full_a: u64,
    full_b: u64,
    comp_a: u64,
    comp_b: u64,
    key_a: u64,
    key_b: u64,
) -> Verdict {
    if full_a == full_b {
        Verdict::Identical
    } else if comp_a == comp_b {
        Verdict::ComputationallyEqual
    } else if key_a == key_b {
        Verdict::Equivalent
    } else {
        Verdict::Divergent
    }
}

impl Verdict {
    pub fn glyph(self) -> &'static str {
        match self {
            Verdict::Identical => "≡",
            Verdict::ComputationallyEqual => "=",
            Verdict::Equivalent => "≈",
            Verdict::Divergent => "≠",
        }
    }
    pub fn ascii(self) -> &'static str {
        match self {
            Verdict::Identical => "IDENT",
            Verdict::ComputationallyEqual => "COMP=",
            Verdict::Equivalent => "EQUIV",
            Verdict::Divergent => "APART",
        }
    }
    pub fn is_converged(self) -> bool {
        !matches!(self, Verdict::Divergent)
    }
}

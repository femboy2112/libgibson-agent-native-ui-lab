//! Branching execution history with bounded memory and exact reconstruction.
//!
//! A [`Branch`] is fully determined by `(program, seed, script)`. Its per-step records,
//! checkpoints and digests are a *cache* of that replay: any branch can be thrown away down
//! to a ~1 KiB fossil (first checkpoint + summary) and rebuilt bit-for-bit by deterministic
//! replay. A fork copies nothing: the child owns only the steps from its fork position and
//! reads the shared prefix through its parent.
//!
//! Position model: a *position* `p` is "p steps have executed". The state at position `p` is
//! the state after step `p-1`. A fork at position `p` shares positions `0..=p` with its parent
//! and diverges when step `p` executes.

use crate::epoch::*;
use crate::fixture::*;
use crate::sem::*;
use crate::vm::*;
use std::sync::Arc;

pub type BranchId = u16;
pub const CKPT_EVERY: u32 = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Root branch: no edit.
    None,
    /// Replace the outcome of the decision executed at the fork position.
    Override(i32),
    /// Insert an operator command at the fork position.
    InsertCmd(u8),
    /// Remove the operator command scheduled at the fork position.
    DropCmd,
    /// Replace the operator command scheduled at the fork position.
    ReplaceCmd(u8),
}

impl Edit {
    pub fn describe(&self) -> String {
        match self {
            Edit::None => "root".into(),
            Edit::Override(v) => format!("decision := {v}"),
            Edit::InsertCmd(c) => format!("+cmd {}", cmd_name(*c)),
            Edit::DropCmd => "-cmd".into(),
            Edit::ReplaceCmd(c) => format!("cmd := {}", cmd_name(*c)),
        }
    }
}

pub fn cmd_name(c: u8) -> &'static str {
    match c {
        1 => "BOOST",
        2 => "THROTTLE",
        3 => "FLUSH",
        5 => "EMERGENCY",
        _ => "NOP",
    }
}

#[derive(Clone, Debug)]
pub struct Rec {
    pub ev: StepEvent,
    /// State after the step.
    pub vars: [i32; NVARS],
    pub epoch: Epoch,
    pub d_full: u64,
    pub d_comp: u64,
    pub sem: u64,
    pub tcode: [u8; MAX_TASKS],
    pub locks: [u8; NLOCKS],
    pub qlen: [u8; NCHAN],
    pub dead: bool,
}

fn tcode(s: TStatus) -> u8 {
    match s {
        TStatus::Ready => 0,
        TStatus::Sleeping(_) => 1,
        TStatus::WaitRecv(..) => 2,
        TStatus::WaitSend(..) => 3,
        TStatus::WaitLock(_) => 4,
        TStatus::Crashed(_) => 5,
        TStatus::Halted => 6,
    }
}

fn make_rec(m: &Machine, ev: StepEvent, epoch: Epoch) -> Rec {
    let d = dims(m);
    let mut tc = [0u8; MAX_TASKS];
    for (i, t) in m.tasks.iter().enumerate() {
        tc[i] = tcode(t.status);
    }
    Rec {
        ev,
        vars: m.vars,
        epoch,
        d_full: m.digest_full(),
        d_comp: m.digest_computational(),
        sem: sem_key(&d),
        tcode: tc,
        locks: [
            m.locks[0].owner.map(|o| o + 1).unwrap_or(0),
            m.locks[1].owner.map(|o| o + 1).unwrap_or(0),
        ],
        qlen: [
            m.chans[0].q.len() as u8,
            m.chans[1].q.len() as u8,
            m.chans[2].q.len() as u8,
        ],
        dead: !m.deadlocked_tasks().is_empty(),
    }
}

#[derive(Clone, Debug)]
pub struct Ckpt {
    pub pos: u32,
    pub m: Machine,
    pub tr: EpochTracker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Residency {
    Resident,
    Fossil,
}

#[derive(Clone, Debug)]
pub struct Branch {
    pub id: BranchId,
    pub label: String,
    pub parent: Option<BranchId>,
    pub fork_at: u32,
    pub edit: Edit,
    pub script: Script,
    pub recs: Vec<Rec>,
    pub ckpts: Vec<Ckpt>,
    pub frontier: (Machine, EpochTracker),
    pub terminal: Option<Terminal>,
    pub residency: Residency,
    pub created_seq: u32,
    pub last_used: u64,
    /// Epoch spans `(start_pos, epoch)` kept even when fossilized.
    pub spans: Vec<(u32, Epoch)>,
}

impl Branch {
    pub fn first(&self) -> u32 {
        self.fork_at
    }
    pub fn end(&self) -> u32 {
        self.frontier.0.step
    }
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }
    pub fn approx_bytes(&self) -> usize {
        self.recs.len() * std::mem::size_of::<Rec>() + self.ckpts.len() * 900 + 1200
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum HistoryError {
    NoSuchBranch,
    BadPosition,
    NotADecision,
    NoCommandThere,
    NothingToChange,
    BranchBudget,
    ValueOutOfRange,
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Retention {
    /// Maximum resident `Rec`s across all branches before LRU fossilization.
    pub max_resident_recs: usize,
    pub max_branches: usize,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            max_resident_recs: 60_000,
            max_branches: 4096,
        }
    }
}

pub struct History {
    pub prog: Arc<Program>,
    pub seed: u64,
    pub branches: Vec<Branch>,
    pub retention: Retention,
    clock: u64,
    /// Total steps ever executed by the live VM (not replays).
    pub vm_steps: u64,
    /// Total steps executed by reconstruction replays.
    pub replay_steps: u64,
    pub fossilized_total: u64,
    pub rehydrated_total: u64,
}

fn spans_push(spans: &mut Vec<(u32, Epoch)>, pos: u32, e: Epoch) {
    if spans.last().map(|s| s.1) != Some(e) {
        spans.push((pos, e));
    }
}

impl History {
    pub fn new(prog: Arc<Program>, seed: u64, script: Script) -> History {
        let m = Machine::new(prog.clone(), seed);
        let tr = EpochTracker::new();
        let root = Branch {
            id: 0,
            label: "A".into(),
            parent: None,
            fork_at: 0,
            edit: Edit::None,
            script,
            recs: vec![],
            ckpts: vec![Ckpt {
                pos: 0,
                m: m.clone(),
                tr: tr.clone(),
            }],
            frontier: (m, tr),
            terminal: None,
            residency: Residency::Resident,
            created_seq: 0,
            last_used: 0,
            spans: vec![],
        };
        History {
            prog,
            seed,
            branches: vec![root],
            retention: Retention::default(),
            clock: 0,
            vm_steps: 0,
            replay_steps: 0,
            fossilized_total: 0,
            rehydrated_total: 0,
        }
    }

    pub fn branch(&self, b: BranchId) -> &Branch {
        &self.branches[b as usize]
    }

    pub fn touch(&mut self, b: BranchId) {
        self.clock += 1;
        self.branches[b as usize].last_used = self.clock;
    }

    pub fn resident_recs(&self) -> usize {
        self.branches.iter().map(|b| b.recs.len()).sum()
    }

    pub fn approx_bytes(&self) -> usize {
        self.branches.iter().map(|b| b.approx_bytes()).sum()
    }

    /// Execute up to `n` new steps on branch `b`'s frontier. Returns steps executed.
    pub fn advance(&mut self, b: BranchId, n: u32) -> u32 {
        self.touch(b);
        self.ensure_resident(b);
        let br = &mut self.branches[b as usize];
        let mut done = 0;
        for _ in 0..n {
            if br.frontier.0.terminal.is_some() {
                break;
            }
            let (m, tr) = &mut br.frontier;
            match m.step(&br.script) {
                Some(ev) => {
                    let e = tr.observe(m, &ev);
                    let pos = m.step;
                    spans_push(&mut br.spans, ev.step, e);
                    br.recs.push(make_rec(m, ev, e));
                    if pos % CKPT_EVERY == 0 {
                        br.ckpts.push(Ckpt {
                            pos,
                            m: m.clone(),
                            tr: tr.clone(),
                        });
                    }
                    done += 1;
                }
                None => break,
            }
        }
        br.terminal = br.frontier.0.terminal;
        self.vm_steps += done as u64;
        done
    }

    /// Run branch `b` to its end (terminal or step cap).
    pub fn run_to_end(&mut self, b: BranchId) -> u32 {
        let mut total = 0;
        loop {
            let d = self.advance(b, 64);
            total += d;
            if d == 0 {
                break;
            }
        }
        total
    }

    /// Exact state at position `pos` on branch `b`, by checkpoint + deterministic replay.
    pub fn machine_at(
        &mut self,
        b: BranchId,
        pos: u32,
    ) -> Result<(Machine, EpochTracker), HistoryError> {
        if b as usize >= self.branches.len() {
            return Err(HistoryError::NoSuchBranch);
        }
        let first = self.branches[b as usize].first();
        if pos < first {
            let p = self.branches[b as usize]
                .parent
                .ok_or(HistoryError::BadPosition)?;
            return self.machine_at(p, pos);
        }
        if pos > self.branches[b as usize].end() {
            return Err(HistoryError::BadPosition);
        }
        let br = &self.branches[b as usize];
        let c = br
            .ckpts
            .iter()
            .rev()
            .find(|c| c.pos <= pos)
            .ok_or(HistoryError::BadPosition)?;
        let (mut m, mut tr) = (c.m.clone(), c.tr.clone());
        let script = br.script.clone();
        let mut n = 0u64;
        while m.step < pos {
            let ev = m.step(&script).ok_or(HistoryError::BadPosition)?;
            tr.observe(&m, &ev);
            n += 1;
        }
        self.replay_steps += n;
        Ok((m, tr))
    }

    /// Record at `step` (the step index, i.e. position `step+1` after it), reading through
    /// ancestors for the shared prefix. `None` if that range is fossilized or out of range.
    pub fn rec_at(&self, b: BranchId, step: u32) -> Option<&Rec> {
        let br = self.branches.get(b as usize)?;
        if step < br.first() {
            return self.rec_at(br.parent?, step);
        }
        if br.residency != Residency::Resident {
            return None;
        }
        br.recs.get((step - br.first()) as usize)
    }

    /// Rebuild a fossilized branch (and its ancestors) by replay. Verifies checkpoints.
    pub fn ensure_resident(&mut self, b: BranchId) {
        if self.branches[b as usize].residency == Residency::Resident {
            return;
        }
        let br = &self.branches[b as usize];
        let (mut m, mut tr) = (br.ckpts[0].m.clone(), br.ckpts[0].tr.clone());
        let end = br.end();
        let script = br.script.clone();
        let mut recs = Vec::with_capacity((end - br.first()) as usize);
        let mut spans = vec![];
        let mut ckpts = vec![br.ckpts[0].clone()];
        let mut n = 0u64;
        while m.step < end {
            let Some(ev) = m.step(&script) else { break };
            let e = tr.observe(&m, &ev);
            spans_push(&mut spans, ev.step, e);
            recs.push(make_rec(&m, ev, e));
            if m.step % CKPT_EVERY == 0 {
                ckpts.push(Ckpt {
                    pos: m.step,
                    m: m.clone(),
                    tr: tr.clone(),
                });
            }
            n += 1;
        }
        let br = &mut self.branches[b as usize];
        debug_assert_eq!(
            br.frontier.0.digest_full(),
            m.digest_full(),
            "replay must reproduce the frontier"
        );
        br.recs = recs;
        br.ckpts = ckpts;
        br.spans = spans;
        br.residency = Residency::Resident;
        self.replay_steps += n;
        self.rehydrated_total += 1;
        self.touch(b);
    }

    /// Make branch `b` and every ancestor resident (needed to read its whole timeline).
    pub fn ensure_chain(&mut self, b: BranchId) {
        let mut cur = Some(b);
        while let Some(c) = cur {
            self.ensure_resident(c);
            cur = self.branches[c as usize].parent;
        }
    }

    pub fn fossilize(&mut self, b: BranchId) {
        let br = &mut self.branches[b as usize];
        if br.residency == Residency::Fossil {
            return;
        }
        br.recs = Vec::new();
        br.ckpts.truncate(1);
        br.residency = Residency::Fossil;
        self.fossilized_total += 1;
    }

    /// Fossilize least-recently-used branches (never those in `protect`) until under budget.
    pub fn enforce_budget(&mut self, protect: &[BranchId]) {
        let mut keep: Vec<BranchId> = protect.to_vec();
        for &p in protect {
            let mut cur = self.branches[p as usize].parent;
            while let Some(c) = cur {
                keep.push(c);
                cur = self.branches[c as usize].parent;
            }
        }
        while self.resident_recs() > self.retention.max_resident_recs {
            let victim = self
                .branches
                .iter()
                .filter(|b| {
                    b.residency == Residency::Resident
                        && !keep.contains(&b.id)
                        && !b.recs.is_empty()
                })
                .min_by_key(|b| b.last_used)
                .map(|b| b.id);
            match victim {
                Some(v) => self.fossilize(v),
                None => break,
            }
        }
    }

    /// The script that results from applying `edit` at position `at` to `parent`'s script.
    pub fn apply_edit(
        &self,
        parent: BranchId,
        at: u32,
        edit: &Edit,
    ) -> Result<Script, HistoryError> {
        let p = &self.branches[parent as usize];
        let mut inputs: Vec<Input> = p
            .script
            .inputs
            .iter()
            .copied()
            // decision overrides keyed to parent steps *after* the fork are meaningless after
            // divergence; one at the fork step itself belongs to the shared prefix (inputs apply
            // before scheduling) and must survive an unrelated edit
            .filter(|i| i.at <= at || matches!(i.kind, InputKind::Cmd(_)))
            .collect();
        match edit {
            Edit::None => {}
            Edit::Override(v) => {
                let rec = self.rec_at(parent, at).ok_or(HistoryError::BadPosition)?;
                if !rec.ev.is_decision() {
                    return Err(HistoryError::NotADecision);
                }
                if *v < 0 || *v >= rec.ev.aux {
                    return Err(HistoryError::ValueOutOfRange);
                }
                inputs.retain(|i| !(i.at == at && matches!(i.kind, InputKind::Override(_))));
                inputs.push(Input {
                    at,
                    kind: InputKind::Override(*v),
                });
            }
            Edit::InsertCmd(c) => {
                inputs.retain(|i| !(i.at == at && matches!(i.kind, InputKind::Cmd(_))));
                inputs.push(Input {
                    at,
                    kind: InputKind::Cmd(*c),
                });
            }
            Edit::DropCmd => {
                let before = inputs.len();
                inputs.retain(|i| !(i.at == at && matches!(i.kind, InputKind::Cmd(_))));
                if inputs.len() == before {
                    return Err(HistoryError::NoCommandThere);
                }
            }
            Edit::ReplaceCmd(c) => {
                let mut found = false;
                for i in inputs.iter_mut() {
                    if i.at == at && matches!(i.kind, InputKind::Cmd(_)) {
                        i.kind = InputKind::Cmd(*c);
                        found = true;
                    }
                }
                if !found {
                    return Err(HistoryError::NoCommandThere);
                }
            }
        }
        Ok(Script::new(inputs))
    }

    /// Fork `parent` at position `at` with `edit`. The old future stays where it is.
    pub fn fork(
        &mut self,
        parent: BranchId,
        at: u32,
        edit: Edit,
    ) -> Result<BranchId, HistoryError> {
        if parent as usize >= self.branches.len() {
            return Err(HistoryError::NoSuchBranch);
        }
        if self.branches.len() >= self.retention.max_branches.min(BranchId::MAX as usize) {
            return Err(HistoryError::BranchBudget);
        }
        if at > self.branches[parent as usize].end() {
            return Err(HistoryError::BadPosition);
        }
        self.ensure_chain(parent);
        let script = self.apply_edit(parent, at, &edit)?;
        let (m, tr) = self.machine_at(parent, at)?;
        if m.terminal.is_some() {
            return Err(HistoryError::NothingToChange);
        }
        let id = self.branches.len() as BranchId;
        let label = branch_label(id);
        let seq = self.branches.len() as u32;
        self.clock += 1;
        let first_ckpt = Ckpt {
            pos: at,
            m: m.clone(),
            tr: tr.clone(),
        };
        self.branches.push(Branch {
            id,
            label,
            parent: Some(parent),
            fork_at: at,
            edit,
            script,
            recs: vec![],
            ckpts: vec![first_ckpt],
            frontier: (m, tr),
            terminal: None,
            residency: Residency::Resident,
            created_seq: seq,
            last_used: self.clock,
            spans: vec![],
        });
        Ok(id)
    }

    /// Epoch per step index for the *whole* timeline of `b` (prefix through ancestors).
    pub fn epoch_at(&self, b: BranchId, step: u32) -> Option<Epoch> {
        self.rec_at(b, step).map(|r| r.epoch)
    }

    /// Replay branch `b` from scratch (ignoring every cache) and return every record.
    pub fn replay_from_scratch(&self, b: BranchId) -> Vec<Rec> {
        let br = &self.branches[b as usize];
        let mut m = Machine::new(self.prog.clone(), self.seed);
        let mut tr = EpochTracker::new();
        let mut out = vec![];
        while m.step < br.end() {
            let Some(ev) = m.step(&br.script) else { break };
            let e = tr.observe(&m, &ev);
            out.push(make_rec(&m, ev, e));
        }
        out
    }

    /// Every step index of branch `b`'s timeline as `(step, &Rec)` (ancestors included).
    pub fn timeline(&self, b: BranchId) -> Vec<&Rec> {
        let end = self.branches[b as usize].end();
        (0..end).filter_map(|s| self.rec_at(b, s)).collect()
    }
}

pub fn branch_label(id: BranchId) -> String {
    // A..Z, then A1.. etc.
    let letter = (b'A' + (id % 26) as u8) as char;
    if id < 26 {
        letter.to_string()
    } else {
        format!("{letter}{}", id / 26)
    }
}

// --- landmarks ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LandmarkKind {
    Fork,
    Input,
    Fate,
    Epoch,
    Event,
    Checkpoint,
    Catastrophe,
}

#[derive(Clone, Debug)]
pub struct Landmark {
    pub pos: u32,
    pub kind: LandmarkKind,
    pub text: String,
    pub epoch: Epoch,
}

/// Semantic landmarks of a branch's whole timeline (positions are *after* the step).
pub fn landmarks(h: &History, b: BranchId) -> Vec<Landmark> {
    let br = h.branch(b);
    let mut out = vec![];
    if br.fork_at > 0 {
        out.push(Landmark {
            pos: br.fork_at,
            kind: LandmarkKind::Fork,
            text: format!(
                "fork of {} · {}",
                h.branch(br.parent.unwrap_or(0)).label,
                br.edit.describe()
            ),
            epoch: h
                .rec_at(b, br.fork_at.saturating_sub(1))
                .map(|r| r.epoch)
                .unwrap_or(Epoch::Stable),
        });
    }
    let end = br.end();
    let mut prev = Epoch::Stable;
    for s in 0..end {
        let Some(r) = h.rec_at(b, s) else { continue };
        let pos = s + 1;
        if r.epoch != prev {
            out.push(Landmark {
                pos,
                kind: LandmarkKind::Epoch,
                text: format!("→ {}", r.epoch.name()),
                epoch: r.epoch,
            });
            prev = r.epoch;
        }
        let ev = &r.ev;
        if ev.flags & F_INPUT != 0 {
            let cmd = br.script.cmd_at(s).map(|(_, c)| cmd_name(c)).unwrap_or("?");
            // a landmark at the position *before* the step: standing there, `fork` can still drop,
            // replace or insert at this very input
            out.push(Landmark {
                pos: s,
                kind: LandmarkKind::Input,
                text: format!("input {cmd}"),
                epoch: r.epoch,
            });
        }
        if ev.class == Class::Fate {
            let what = match (ev.task, ev.pc) {
                _ if ev.after == 0 && ev.aux == 8 => Some("surge rolled"),
                _ if ev.after == 0 && ev.aux == 14 => Some("pump failure rolled"),
                _ if ev.after == 0 && ev.aux == 4 => Some("racy path rolled"),
                _ => None,
            };
            if let Some(w) = what {
                out.push(Landmark {
                    pos: s,
                    kind: LandmarkKind::Fate,
                    text: format!("fate: {w}"),
                    epoch: r.epoch,
                });
            }
        }
        if ev.class == Class::Mark {
            let t = ev.aux as u8;
            if matches!(
                t,
                M_RESTART | M_STEAL | M_SCRAM | M_RECONCILE | M_AUDIT_FAIL | M_NUDGE
            ) {
                out.push(Landmark {
                    pos,
                    kind: LandmarkKind::Event,
                    text: mark_name(t).to_string(),
                    epoch: r.epoch,
                });
            }
        }
        if ev.class == Class::Fail {
            out.push(Landmark {
                pos,
                kind: LandmarkKind::Event,
                text: format!("{} crashed", TASK_NAMES[ev.task as usize % MAX_TASKS]),
                epoch: r.epoch,
            });
        }
        if ev.flags & F_CATASTROPHE != 0 {
            out.push(Landmark {
                pos,
                kind: LandmarkKind::Catastrophe,
                text: "MELTDOWN".into(),
                epoch: Epoch::Catastrophe,
            });
        }
        if pos % CKPT_EVERY == 0 {
            out.push(Landmark {
                pos,
                kind: LandmarkKind::Checkpoint,
                text: format!("checkpoint @{pos}"),
                epoch: r.epoch,
            });
        }
    }
    out.sort_by_key(|l| (l.pos, l.kind));
    out
}

// --- comparison --------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootKind {
    /// The intervention itself (overridden decision / changed input).
    Intervention,
    /// A diverged event with no diverged data-causal parent that is not the intervention:
    /// a scheduler-order consequence the data-causality graph cannot explain.
    Reorder,
}

#[derive(Clone, Debug)]
pub struct CompareRow {
    pub verdict: Verdict,
    pub rel: [Rel; NDIM],
    pub diverged_event: bool,
    pub root: Option<RootKind>,
}

#[derive(Clone, Debug)]
pub struct Compare {
    pub a: BranchId,
    pub b: BranchId,
    /// Rows for step indices `0..rows.len()`.
    pub rows: Vec<CompareRow>,
    pub first_divergence: Option<u32>,
    pub last_converged_run: Option<(u32, u32)>,
}

pub fn compare(h: &History, a: BranchId, b: BranchId) -> Option<Compare> {
    let n = h.branch(a).end().min(h.branch(b).end());
    let mut rows = Vec::with_capacity(n as usize);
    let mut first_div = None;
    let mut diverged = vec![false; n as usize];
    // the intervention is where the *scripts* differ (a changed/inserted/dropped command, an
    // overridden decision) — which can be many steps before any event looks different
    let (sa, sb) = (&h.branch(a).script, &h.branch(b).script);
    let intervention_at = |s: u32| -> bool {
        sa.cmd_at(s).map(|x| x.1) != sb.cmd_at(s).map(|x| x.1)
            || sa.override_at(s) != sb.override_at(s)
    };
    for s in 0..n {
        let ra = h.rec_at(a, s)?;
        let rb = h.rec_at(b, s)?;
        let ea = &ra.ev;
        let eb = &rb.ev;
        let dv = (
            ea.task, ea.pc, ea.class, ea.obj, ea.before, ea.after, ea.aux,
        ) != (
            eb.task, eb.pc, eb.class, eb.obj, eb.before, eb.after, eb.aux,
        ) || (ea.flags & (F_INPUT | F_OVERRIDDEN))
            != (eb.flags & (F_INPUT | F_OVERRIDDEN));
        diverged[s as usize] = dv;
        // "first divergence" is the first step whose *state* or event differs
        if (dv || ra.d_comp != rb.d_comp || intervention_at(s)) && first_div.is_none() {
            first_div = Some(s);
        }
    }
    // dimension relations need the states: use the per-step records' vars and codes for the
    // visual lanes, and the digests for the verdict.
    let mut affected = vec![false; n as usize];
    for s in 0..n {
        let ra = h.rec_at(a, s)?;
        let rb = h.rec_at(b, s)?;
        let verdict = verdict(ra.d_full, rb.d_full, ra.d_comp, rb.d_comp, ra.sem, rb.sem);
        let rel = rel_from_recs(ra, rb);
        // `affected[x]` = step x lies in the causal cone of the intervention or of a diverged
        // event (it diverged, was injected by a changed input, or has an affected parent — even if
        // the event record itself looks identical, e.g. a register that now holds another value).
        let parent_affected = |p: u32| -> bool {
            if p == NONE {
                false
            } else if p & 0x8000_0000 != 0 {
                intervention_at(p & 0x7fff_ffff)
            } else {
                (p as usize) < affected.len() && affected[p as usize]
            }
        };
        let any_parent_affected = [&ra.ev, &rb.ev]
            .iter()
            .any(|e| e.parents.iter().any(|&p| parent_affected(p)));
        let mut root = None;
        if intervention_at(s) {
            root = Some(RootKind::Intervention);
        } else if diverged[s as usize] && !any_parent_affected {
            // diverged, yet nothing in its data-causal past was touched: a scheduler-order effect
            root = Some(RootKind::Reorder);
        }
        affected[s as usize] = intervention_at(s) || diverged[s as usize] || any_parent_affected;
        rows.push(CompareRow {
            verdict,
            rel,
            diverged_event: diverged[s as usize],
            root,
        });
    }
    // last run of non-divergent verdicts after the first divergence
    let mut run: Option<(u32, u32)> = None;
    if let Some(fd) = first_div {
        let mut s = fd;
        while s < n {
            if rows[s as usize].verdict.is_converged() {
                let st = s;
                while s < n && rows[s as usize].verdict.is_converged() {
                    s += 1;
                }
                run = Some((st, s));
            } else {
                s += 1;
            }
        }
    }
    Some(Compare {
        a,
        b,
        rows,
        first_divergence: first_div,
        last_converged_run: run,
    })
}

#[allow(clippy::needless_range_loop)]
fn rel_from_recs(a: &Rec, b: &Rec) -> [Rel; NDIM] {
    let mut out = [Rel::Same; NDIM];
    for v in 0..NVARS {
        out[v] = relate(
            (a.vars[v], var_class(v, a)),
            (b.vars[v], var_class(v, b)),
            dim_is_semantic(v),
        );
    }
    for t in 0..MAX_TASKS {
        let i = NVARS + t;
        let ca = task_class(a.tcode[t]);
        let cb = task_class(b.tcode[t]);
        out[i] = if a.tcode[t] == b.tcode[t] {
            Rel::Same
        } else if ca == cb {
            Rel::Equiv
        } else {
            Rel::Apart
        };
    }
    for l in 0..NLOCKS {
        let i = NVARS + MAX_TASKS + l;
        out[i] = if a.locks[l] == b.locks[l] {
            Rel::Same
        } else if (a.locks[l] != 0) == (b.locks[l] != 0) {
            Rel::Equiv
        } else {
            Rel::Apart
        };
    }
    for c in 0..NCHAN {
        let i = NVARS + MAX_TASKS + NLOCKS + c;
        out[i] = if a.qlen[c] == b.qlen[c] {
            Rel::Same
        } else if (a.qlen[c] >= 2) == (b.qlen[c] >= 2) {
            Rel::Equiv
        } else {
            Rel::Apart
        };
    }
    out
}

fn task_class(code: u8) -> u8 {
    match code {
        0..=2 => 0,
        3 | 4 => 1,
        5 => 2,
        _ => 3,
    }
}

fn var_class(v: usize, r: &Rec) -> i32 {
    let raw = r.vars[v];
    match v as u8 {
        V_HEAT => heat_band(raw),
        V_LOAD => match raw {
            i32::MIN..=1 => 0,
            2..=4 => 1,
            5..=7 => 2,
            _ => 3,
        },
        V_COOL => match raw {
            i32::MIN..=2 => 0,
            3..=5 => 1,
            _ => 2,
        },
        V_CREDITS => (r.vars[V_CREDITS as usize] != r.vars[V_LEDGER as usize]) as i32,
        V_FAULTS => (raw > 0) as i32,
        V_ESC => match raw {
            i32::MIN..=0 => 0,
            1..=2 => 1,
            _ => 2,
        },
        V_MODE => raw,
        _ => 0,
    }
}

/// Causal ancestry of the event at `step` on `b`, breadth-first, up to `limit` events.
pub fn ancestry(h: &History, b: BranchId, step: u32, limit: usize) -> Vec<(u32, u8)> {
    let mut out: Vec<(u32, u8)> = vec![];
    let mut frontier = vec![(step, 0u8)];
    let mut seen = std::collections::BTreeSet::new();
    while let Some((s, d)) = if frontier.is_empty() {
        None
    } else {
        Some(frontier.remove(0))
    } {
        if !seen.insert(s) {
            continue;
        }
        out.push((s, d));
        if out.len() >= limit {
            break;
        }
        if let Some(r) = h.rec_at(b, s) {
            for p in r.ev.parents {
                if p != NONE && p & 0x8000_0000 == 0 {
                    frontier.push((p, d.saturating_add(1)));
                }
            }
        }
    }
    out
}

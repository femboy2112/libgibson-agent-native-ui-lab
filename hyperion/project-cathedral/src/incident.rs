//! Incident semantics: the phase machine that turns raw tick metrics into a small,
//! meaningful vocabulary of states. This is the bridge between "numbers" and both
//! the architecture and the score.

use crate::action::{Action, ActionKind};
use crate::sim::{Engine, Metrics};

/// The incident vocabulary exposed to the visuals and to HumanMusic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Normal,
    Rising,
    Overload,
    LocalFault,
    Cascade,
    Diagnosis,
    Intervention,
    PartialRecovery,
    Restored,
}

impl Phase {
    pub const ALL: [Phase; 9] = [
        Phase::Normal,
        Phase::Rising,
        Phase::Overload,
        Phase::LocalFault,
        Phase::Cascade,
        Phase::Diagnosis,
        Phase::Intervention,
        Phase::PartialRecovery,
        Phase::Restored,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Phase::Normal => "NORMAL",
            Phase::Rising => "RISING UNCERTAINTY",
            Phase::Overload => "OVERLOAD",
            Phase::LocalFault => "LOCAL FAULT",
            Phase::Cascade => "CASCADE",
            Phase::Diagnosis => "DIAGNOSIS",
            Phase::Intervention => "INTERVENTION",
            Phase::PartialRecovery => "PARTIAL RECOVERY",
            Phase::Restored => "RESTORED",
        }
    }

    /// `[0,1]` severity used by the visuals (structure) and the score (tension).
    pub fn severity(self) -> f32 {
        match self {
            Phase::Normal => 0.05,
            Phase::Rising => 0.25,
            Phase::Overload => 0.55,
            Phase::LocalFault => 0.50,
            Phase::Cascade => 1.0,
            Phase::Diagnosis => 0.60,
            Phase::Intervention => 0.70,
            Phase::PartialRecovery => 0.35,
            Phase::Restored => 0.08,
        }
    }
}

/// One phase transition in the incident timeline.
#[derive(Debug, Clone)]
pub struct IncidentRecord {
    pub frame: u32,
    pub phase: Phase,
    pub metrics: Metrics,
    /// A short human/musical reason for the transition.
    pub reason: String,
}

/// Stateful phase classifier with hysteresis.
pub struct PhaseTracker {
    pub phase: Phase,
    pub records: Vec<IncidentRecord>,
    was_troubled: bool,
    was_cascade: bool,
    calm_ticks: u32,
    last_intervention: Option<u32>,
    last_ack: Option<u32>,
    last_fault: Option<u32>,
    /// Candidate phase and how many consecutive ticks it has held.
    pending: Option<(Phase, u32)>,
}

impl PhaseTracker {
    pub fn new() -> Self {
        PhaseTracker {
            phase: Phase::Normal,
            records: Vec::new(),
            was_troubled: false,
            was_cascade: false,
            calm_ticks: 0,
            last_intervention: None,
            last_ack: None,
            last_fault: None,
            pending: None,
        }
    }

    /// Note an operator action so diagnosis/intervention phases can be represented.
    pub fn observe_action(&mut self, action: &Action) {
        match action.kind {
            ActionKind::InjectFault => self.last_fault = Some(action.frame),
            ActionKind::Acknowledge | ActionKind::Annotate | ActionKind::SetPhase => {
                self.last_ack = Some(action.frame)
            }
            ActionKind::Rollback
            | ActionKind::Isolate
            | ActionKind::Reinstate
            | ActionKind::Reroute
            | ActionKind::ShedLoad
            | ActionKind::ClearFault => self.last_intervention = Some(action.frame),
        }
    }

    /// Reclassify from the engine's current metrics.
    pub fn update(&mut self, engine: &Engine) -> Option<IncidentRecord> {
        let m = engine.metrics;
        let frame = engine.frame;
        let critical = m.frac_critical;
        let overloaded = m.frac_overloaded;
        let active_faults = engine
            .states
            .iter()
            .filter(|s| s.bad_deploy || s.fault > 0.1 || s.isolated)
            .count();

        let recent = |at: Option<u32>, window: u32| at.is_some_and(|f| frame.saturating_sub(f) <= window);

        let next = if recent(self.last_intervention, 80) && critical > 0.02 {
            Phase::Intervention
        } else if recent(self.last_ack, 150) && critical > 0.02 {
            Phase::Diagnosis
        } else if critical >= 0.10 || m.open_breakers >= 6 {
            Phase::Cascade
        } else if self.phase == Phase::Cascade && critical >= 0.04 {
            // Schmitt trigger: once cascading, only release when the failure has
            // clearly broken, not on every dip across the entry threshold.
            Phase::Cascade
        } else if self.was_cascade && critical >= 0.02 {
            Phase::PartialRecovery
        } else if overloaded >= 0.12 {
            Phase::Overload
        } else if active_faults > 0 && critical < 0.03 {
            Phase::LocalFault
        } else if overloaded >= 0.04 || m.mean_health < 0.96 || m.open_breakers > 0 {
            Phase::Rising
        } else if self.was_troubled {
            self.calm_ticks += 1;
            if self.calm_ticks > 40 {
                Phase::Restored
            } else {
                Phase::PartialRecovery
            }
        } else {
            Phase::Normal
        };

        // Apply with a small dwell so a one-tick flicker cannot thrash the score.
        if next == self.phase {
            self.pending = None;
            return None;
        }
        if next == Phase::PartialRecovery && self.phase == Phase::Intervention {
            // allow direct handoff
        }

        // Hysteresis: a differing candidate must persist for several ticks before
        // it commits, so a one-tick breaker flicker cannot thrash the phase (and
        // therefore the score). This is what makes the incident read as a
        // narrative rather than a strobe.
        const DWELL: u32 = 6;
        match self.pending {
            Some((p, n)) if p == next && n + 1 >= DWELL => {
                self.pending = None;
            }
            Some((p, n)) if p == next => {
                self.pending = Some((next, n + 1));
                return None;
            }
            _ => {
                self.pending = Some((next, 1));
                return None;
            }
        }

        match next {
            Phase::Cascade => {
                self.was_cascade = true;
                self.was_troubled = true;
                self.calm_ticks = 0;
            }
            Phase::Normal => {
                self.was_troubled = false;
                self.was_cascade = false;
            }
            Phase::Restored => {
                self.was_troubled = false;
                self.was_cascade = false;
                self.calm_ticks = 0;
            }
            _ => {
                self.was_troubled = true;
            }
        }

        let reason = explain(self.phase, next, m, active_faults);
        self.phase = next;
        let rec = IncidentRecord {
            frame,
            phase: next,
            metrics: m,
            reason,
        };
        self.records.push(rec.clone());
        Some(rec)
    }

    /// The incident's semantic checkpoint digest (phase sequence).
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for r in &self.records {
            h ^= (r.frame as u64) << 32 | r.phase as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }
}

impl Default for PhaseTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn explain(from: Phase, to: Phase, m: Metrics, active_faults: usize) -> String {
    format!(
        "{} -> {}: critical {:.3}, overloaded {:.3}, breakers {}, active faults {}",
        from.label(),
        to.label(),
        m.frac_critical,
        m.frac_overloaded,
        m.open_breakers,
        active_faults
    )
}

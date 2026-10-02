//! Operator actions and the ordered action journal.
//!
//! Every operator action is appended to a single ordered [`Journal`]. Replaying
//! `fixture + seed + journal` must reconstruct an equivalent semantic state; the
//! journal is the *only* thing the operator is allowed to inject.

use serde::{Deserialize, Serialize};

use crate::sim::fixture::ServiceId;

/// The operator vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    /// Deploy a faulty version to a service (sets `bad_deploy` + a raw fault).
    InjectFault,
    /// Roll a bad deployment back (clears `bad_deploy`, reduces `fault`).
    Rollback,
    /// Sever a service from the graph (no traffic reaches it).
    Isolate,
    /// Reconnect a previously severed service.
    Reinstate,
    /// Shift traffic off a service and boost healthy peers.
    Reroute,
    /// Drop a fraction of a service's offered load.
    ShedLoad,
    /// Remove an injected raw fault without rolling back the deployment.
    ClearFault,
    /// Acknowledge the current incident (journal-only; affects the score).
    Acknowledge,
    /// Attach a free-text annotation to the incident timeline.
    Annotate,
    /// Explicitly set the incident phase (journal-only; a diagnosis shortcut).
    SetPhase,
}

impl ActionKind {
    pub fn label(self) -> &'static str {
        match self {
            ActionKind::InjectFault => "inject-fault",
            ActionKind::Rollback => "rollback",
            ActionKind::Isolate => "isolate",
            ActionKind::Reinstate => "reinstate",
            ActionKind::Reroute => "reroute",
            ActionKind::ShedLoad => "shed-load",
            ActionKind::ClearFault => "clear-fault",
            ActionKind::Acknowledge => "acknowledge",
            ActionKind::Annotate => "annotate",
            ActionKind::SetPhase => "set-phase",
        }
    }
}

/// One ordered operator action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Frame at which the action was issued.
    pub frame: u32,
    pub kind: ActionKind,
    /// Target service (`u16::MAX` = none, for journal-only actions).
    pub target: ServiceId,
    /// Magnitude / fraction / phase-selector, action-dependent.
    pub magnitude: f32,
    /// Stable sequence number in the journal.
    pub seq: u32,
    /// Human-readable label (also used for annotations).
    pub note: String,
}

impl Action {
    pub fn new(frame: u32, seq: u32, kind: ActionKind, target: ServiceId, magnitude: f32, note: &str) -> Self {
        Action {
            frame,
            kind,
            target,
            magnitude,
            seq,
            note: note.to_string(),
        }
    }
}

/// The ordered action journal.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Journal {
    pub seed: u64,
    pub fixture: String,
    pub actions: Vec<Action>,
}

impl Journal {
    pub fn new(seed: u64, fixture: &str) -> Self {
        Journal {
            seed,
            fixture: fixture.to_string(),
            actions: Vec::new(),
        }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// The digest of the journal itself, for receipts.
    pub fn digest(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        crate::hash::sha256_hex(json.as_bytes())
    }

    /// All actions issued at exactly `frame`.
    pub fn at(&self, frame: u32) -> impl Iterator<Item = &Action> {
        self.actions.iter().filter(move |a| a.frame == frame)
    }
}

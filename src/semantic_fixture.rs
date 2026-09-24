//! A finite fake session shared by every presentation experiment.
//!
//! Source events are immutable and ordered. Explicit actions are merged by time;
//! source events win timestamp ties. The first valid permission action resolves
//! the pending request and suppresses the later scripted default. This changes
//! authorization, not the source event order. Denial withholds the final artifact.
//! No tool in this fixture accesses the filesystem, network, or a real agent.

use serde::{Deserialize, Serialize};

pub const DURATION_MS: u64 = 18_000;
pub const MAX_ACTIONS: usize = 64;
pub const AGENT_IDS: [&str; 4] = ["architect", "scout", "builder", "verify"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticEvent {
    SessionStarted,
    UserMessage {
        text: String,
    },
    AgentStarted {
        id: String,
        title: String,
    },
    PlanUpdated {
        steps: Vec<String>,
    },
    SubagentSpawned {
        parent: String,
        child: String,
    },
    ToolStarted {
        agent: String,
        tool: String,
    },
    ToolProgress {
        agent: String,
        percent: u8,
        summary: String,
    },
    ToolFinished {
        agent: String,
        tool: String,
        success: bool,
        summary: String,
    },
    AgentMessageChunk {
        agent: String,
        text: String,
    },
    Warning {
        text: String,
    },
    PermissionRequested {
        reason: String,
    },
    PermissionResolved {
        approved: bool,
    },
    ArtifactProduced {
        name: String,
    },
    AgentFinished {
        agent: String,
        summary: String,
    },
    SessionFinished,
}

impl SemanticEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionStarted => "SessionStarted",
            Self::UserMessage { .. } => "UserMessage",
            Self::AgentStarted { .. } => "AgentStarted",
            Self::PlanUpdated { .. } => "PlanUpdated",
            Self::SubagentSpawned { .. } => "SubagentSpawned",
            Self::ToolStarted { .. } => "ToolStarted",
            Self::ToolProgress { .. } => "ToolProgress",
            Self::ToolFinished { .. } => "ToolFinished",
            Self::AgentMessageChunk { .. } => "AgentMessageChunk",
            Self::Warning { .. } => "Warning",
            Self::PermissionRequested { .. } => "PermissionRequested",
            Self::PermissionResolved { .. } => "PermissionResolved",
            Self::ArtifactProduced { .. } => "ArtifactProduced",
            Self::AgentFinished { .. } => "AgentFinished",
            Self::SessionFinished => "SessionFinished",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub at_ms: u64,
    pub event: SemanticEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    ResolvePermission { approved: bool },
    ChooseScope { expedition: bool },
    Select { target: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedAction {
    pub at_ms: u64,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentState {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub progress: u8,
    pub failed: bool,
    pub finished: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub goal: String,
    pub agents: Vec<AgentState>,
    pub plan: Vec<String>,
    pub warning: Option<String>,
    pub permission: Option<bool>,
    pub permission_pending: bool,
    pub artifacts: Vec<String>,
    pub finished: bool,
    /// Timestamp + event kind, including source events superseded by an action.
    pub event_order: Vec<String>,
    pub selected: Option<String>,
    pub scope_expedition: bool,
    pub receipt: String,
    pub action_receipts: Vec<String>,
    /// A scope choice is offered by the warning, not silently selected by a view.
    pub scope_choice_available: bool,
    pub permission_explicit: bool,
}

/// Shared, ordered 18-second source trace. All strings describe local simulation.
pub fn fixture() -> Vec<Step> {
    use SemanticEvent::*;
    let s = |at_ms, event| Step { at_ms, event };
    vec![
        s(0, SessionStarted),
        s(200, UserMessage { text: "Produce an accessible night-transit review from the supplied fictional map; preserve source provenance and replay the decision.".into() }),
        s(500, AgentStarted { id: "architect".into(), title: "ARCHITECT".into() }),
        s(800, PlanUpdated { steps: vec!["Index supplied map + source receipts".into(), "Compare accessible route candidates".into(), "Recover failures; verify exact replay".into(), "Request permission; assemble local report".into()] }),
        s(1_200, SubagentSpawned { parent: "architect".into(), child: "scout".into() }),
        s(1_200, AgentStarted { id: "scout".into(), title: "SCOUT".into() }),
        s(1_200, SubagentSpawned { parent: "architect".into(), child: "builder".into() }),
        s(1_200, AgentStarted { id: "builder".into(), title: "BUILDER".into() }),
        s(1_700, ToolStarted { agent: "scout".into(), tool: "fixture.map_index".into() }),
        s(1_700, ToolStarted { agent: "builder".into(), tool: "fixture.route_compare".into() }),
        s(2_000, SubagentSpawned { parent: "architect".into(), child: "verify".into() }),
        s(2_000, AgentStarted { id: "verify".into(), title: "VERIFY".into() }),
        s(2_600, ToolProgress { agent: "scout".into(), percent: 45, summary: "128 stops indexed; 384 source-linked edges".into() }),
        s(3_100, ToolProgress { agent: "builder".into(), percent: 35, summary: "Three candidates; accessibility constraints attached".into() }),
        s(3_800, AgentMessageChunk { agent: "architect".into(), text: "Parallel map receipts and route candidates must share the same fixture revision.".into() }),
        s(4_600, ToolFinished { agent: "builder".into(), tool: "fixture.route_compare".into(), success: false, summary: "Candidate B rejected: missing lift-access receipt at stop 42".into() }),
        s(4_600, Warning { text: "A missing receipt suggests a local repair; a full architecture expedition would expand scope.".into() }),
        s(5_600, ToolFinished { agent: "scout".into(), tool: "fixture.map_index".into(), success: true, summary: "Map ready: 128 stops, 384 edges, six invariant checks".into() }),
        s(7_500, AgentFinished { agent: "scout".into(), summary: "Topology and source receipts handed to verification".into() }),
        s(7_600, ToolStarted { agent: "builder".into(), tool: "fixture.local_repair".into() }),
        s(8_000, ToolProgress { agent: "builder".into(), percent: 68, summary: "Recovery uses supplied alternate lift-access receipt".into() }),
        s(8_400, ToolFinished { agent: "builder".into(), tool: "fixture.local_repair".into(), success: true, summary: "Candidate C accepted; original failure retained as evidence".into() }),
        s(9_000, ToolStarted { agent: "verify".into(), tool: "fixture.replay".into() }),
        s(9_800, ToolProgress { agent: "verify".into(), percent: 55, summary: "13 of 24 fixtures agree; action receipts remain explicit".into() }),
        s(9_990, PermissionRequested { reason: "Approve assembly of the fictional local review artifact? No files or external tools will be touched.".into() }),
        s(10_200, AgentFinished { agent: "builder".into(), summary: "Accessible route C and recovery receipt sealed".into() }),
        s(12_000, ToolFinished { agent: "verify".into(), tool: "fixture.replay".into(), success: true, summary: "24/24 fixtures; ordered events and final state reproduce".into() }),
        s(12_400, AgentFinished { agent: "verify".into(), summary: "Replay equality witness attached to report".into() }),
        s(13_200, PermissionResolved { approved: true }),
        s(14_000, AgentMessageChunk { agent: "architect".into(), text: "Synthesis: route C preserves access, cites source receipts, and explains the rejected candidate.".into() }),
        s(14_800, ArtifactProduced { name: "night-transit-review.md (simulated)".into() }),
        s(16_500, AgentFinished { agent: "architect".into(), summary: "Synthesis complete; every claim has a fixture receipt".into() }),
        s(DURATION_MS, SessionFinished),
    ]
}

/// Validate an action against the snapshot at its actual timestamp.
pub fn validate_action(state: &Snapshot, action: &Action) -> Result<(), String> {
    match action {
        Action::ResolvePermission { .. } if !state.permission_pending => {
            Err("permission request is not pending".into())
        }
        Action::ChooseScope { .. } if !state.scope_choice_available || state.finished => {
            Err("scope choice is not available".into())
        }
        Action::Select { target } if !state.agents.iter().any(|agent| agent.id == *target) => {
            Err("unknown or not-yet-started agent target".into())
        }
        _ => Ok(()),
    }
}

/// Pure replay at an absolute time. Repeated sampling never advances the world.
///
/// At most 64 ordered actions are admitted. An oversized or out-of-order action
/// stream is rejected in its entirety, explicitly in `action_receipts`; the
/// original fixture remains available. Invalid individual actions are recorded
/// and ignored. Source events precede actions at the exact same timestamp.
pub fn snapshot(at_ms: u64, actions: &[TimedAction]) -> Snapshot {
    let mut state = Snapshot::default();
    let rejected = if actions.len() > MAX_ACTIONS {
        Some("action stream exceeds the 64-action budget")
    } else if actions.windows(2).any(|pair| pair[0].at_ms > pair[1].at_ms) {
        Some("action timestamps must be ordered")
    } else {
        None
    };
    let admitted = if rejected.is_some() { &[][..] } else { actions };
    let mut next_action = 0;
    for step in fixture().into_iter().take_while(|step| step.at_ms <= at_ms) {
        while next_action < admitted.len() && admitted[next_action].at_ms < step.at_ms {
            apply_action(&mut state, &admitted[next_action]);
            next_action += 1;
        }
        state
            .event_order
            .push(format!("{:05}:{}", step.at_ms, step.event.kind()));
        apply_event(&mut state, step.event);
    }
    while next_action < admitted.len() && admitted[next_action].at_ms <= at_ms {
        apply_action(&mut state, &admitted[next_action]);
        next_action += 1;
    }
    if let Some(reason) = rejected {
        state
            .action_receipts
            .push(format!("REJECTED INPUT: {reason}"));
    }
    state
}

fn apply_action(state: &mut Snapshot, timed: &TimedAction) {
    if let Err(reason) = validate_action(state, &timed.action) {
        state
            .action_receipts
            .push(format!("{} REJECTED: {reason}", timed.at_ms));
        return;
    }
    let receipt = match &timed.action {
        Action::ResolvePermission { approved } => {
            state.permission = Some(*approved);
            state.permission_pending = false;
            state.permission_explicit = true;
            format!(
                "Permission explicitly {}",
                if *approved { "approved" } else { "declined" }
            )
        }
        Action::ChooseScope { expedition } => {
            state.scope_expedition = *expedition;
            // This is a recorded decision about follow-up scope. It never rewrites
            // the immutable local-repair tool result in this finite session.
            format!(
                "Follow-up scope: {}",
                if *expedition {
                    "architecture expedition"
                } else {
                    "local repair only"
                }
            )
        }
        Action::Select { target } => {
            state.selected = Some(target.clone());
            format!("Selected {target}")
        }
    };
    state.receipt.clone_from(&receipt);
    state
        .action_receipts
        .push(format!("{} APPLIED: {receipt}", timed.at_ms));
}

fn apply_event(state: &mut Snapshot, event: SemanticEvent) {
    use SemanticEvent::*;
    match event {
        SessionStarted => state.receipt = "Local fixture session started".into(),
        UserMessage { text } => state.goal = text,
        AgentStarted { id, title } => state.agents.push(AgentState {
            id,
            title,
            summary: "Waiting for fixture work".into(),
            progress: 0,
            failed: false,
            finished: false,
        }),
        PlanUpdated { steps } => state.plan = steps,
        SubagentSpawned { parent, child } => {
            state.receipt = format!("{parent} delegated to {child}")
        }
        ToolStarted { agent, tool } => {
            if let Some(agent) = state.agents.iter_mut().find(|item| item.id == agent) {
                agent.summary = format!("Running {tool}");
                agent.failed = false;
            }
            state.receipt = format!("Started {tool}");
        }
        ToolProgress {
            agent,
            percent,
            summary,
        } => {
            if let Some(agent) = state.agents.iter_mut().find(|item| item.id == agent) {
                agent.progress = percent.min(100);
                agent.summary = summary;
            }
        }
        ToolFinished {
            agent,
            tool,
            success,
            summary,
        } => {
            if let Some(agent) = state.agents.iter_mut().find(|item| item.id == agent) {
                agent.failed = !success;
                agent.progress = if success { 100 } else { agent.progress };
                agent.summary = summary.clone();
            }
            state.receipt = format!("{tool}: {summary}");
        }
        AgentMessageChunk { agent, text } => {
            if let Some(agent) = state.agents.iter_mut().find(|item| item.id == agent) {
                agent.summary = text.clone();
            }
            state.receipt = text;
        }
        Warning { text } => {
            state.warning = Some(text);
            state.scope_choice_available = true;
        }
        PermissionRequested { reason } => {
            state.permission_pending = true;
            state.receipt = reason;
        }
        PermissionResolved { approved } => {
            if state.permission_explicit {
                state.action_receipts.push(
                    "13200 SUPPRESSED: fixture permission default; explicit choice retained".into(),
                );
            } else {
                state.permission = Some(approved);
                state.permission_pending = false;
                state.receipt = "Scripted permission default approved local assembly".into();
            }
        }
        ArtifactProduced { name } => {
            if state.permission == Some(true) {
                state.receipt = format!("Produced {name}");
                state.artifacts.push(name);
            } else {
                state.receipt = "Artifact withheld: permission declined".into();
            }
        }
        AgentFinished { agent, summary } => {
            if let Some(agent) = state.agents.iter_mut().find(|item| item.id == agent) {
                agent.finished = true;
                agent.progress = 100;
                agent.summary = summary;
            }
        }
        SessionFinished => {
            state.finished = true;
            state.scope_choice_available = false;
            state.receipt = if state.permission == Some(false) {
                "Session complete; artifact withheld by explicit permission decision".into()
            } else if state.scope_expedition {
                "Review assembled; architecture expedition recorded as follow-up only".into()
            } else {
                "Review assembled; local repair, replay witness, provenance retained".into()
            };
        }
    }
}

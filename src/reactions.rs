//! Original semantic reactions over an immutable fixture; all ontology is lab-local.
//!
//! The policy produces visual cues, never actions. `key` is the sole action path.
//! Painting, mode changes, and metaphor changes cannot advance semantic state.
use crate::{
    semantic_fixture::{self as sem, Action, SemanticEvent, Snapshot, Step, TimedAction},
    ui::{self, App},
};
use gibson::{
    canvas::BrailleCanvas,
    input::{KeyCode, KeyEvent, KeyModifiers},
    BorderType, Color, ColorDepth, FocusId, FocusRing, Node, Rect, Style, Surface,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    None,
    Restrained,
    Heavy,
}
impl Mode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "none" => Ok(Self::None),
            "restrained" => Ok(Self::Restrained),
            "heavy" => Ok(Self::Heavy),
            _ => Err("mode must be none, restrained, or heavy".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metaphor {
    Constellation,
    Paperwork,
}
impl Metaphor {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "constellation" => Ok(Self::Constellation),
            "paperwork" => Ok(Self::Paperwork),
            _ => Err("metaphor must be constellation or paperwork".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Dispatch,
    PlanAccepted,
    Progress,
    ToolFailure,
    RepeatedFailure,
    ScopeExplosion,
    Recovery,
    PermissionRequest,
    BoringSuccess,
    Artifact,
    Synthesis,
}
impl Role {
    fn headline(self) -> &'static str {
        match self {
            Self::Dispatch => "FOUR WITNESSES, ONE MAP",
            Self::PlanAccepted => "A PLAN. WITH AN EXIT CONDITION.",
            Self::Progress => "THE BAR IS MOVING. EVIDENCE INCLUDED.",
            Self::ToolFailure => "ONE RECEIPT HAS LEFT THE CHAT",
            Self::RepeatedFailure => "DEJA VU HAS FILED A BUG REPORT",
            Self::ScopeExplosion => "ONE MISSING RECEIPT. AN ENTIRE UNIVERSE?",
            Self::Recovery => "THE FIX FITS THROUGH THE ORIGINAL DOOR",
            Self::PermissionRequest => "THIS BUTTON IS NOT A FORMALITY",
            Self::BoringSuccess => "NOTHING EXPLODED. EXCELLENT.",
            Self::Artifact => "AN ACTUAL RECEIPT, NOT A VICTORY LAP",
            Self::Synthesis => "THE JOKE ENDS. THE EVIDENCE STAYS.",
        }
    }
    fn priority(self) -> u8 {
        match self {
            Self::PermissionRequest => 5,
            Self::ScopeExplosion | Self::RepeatedFailure => 4,
            Self::ToolFailure | Self::Recovery | Self::Artifact | Self::Synthesis => 3,
            Self::PlanAccepted | Self::Dispatch => 2,
            Self::Progress | Self::BoringSuccess => 1,
        }
    }
}

/// An external policy configuration. Unknown fields and unsupported values fail closed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionConfig {
    pub max_active: usize,
    pub cooldown_ms: u64,
    pub lifetime_ms: u64,
    pub intensity: u8,
    pub exclusions: Vec<Role>,
}
impl Default for ReactionConfig {
    fn default() -> Self {
        Self {
            max_active: 2,
            cooldown_ms: 600,
            lifetime_ms: 3_000,
            intensity: 2,
            exclusions: Vec::new(),
        }
    }
}
impl ReactionConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=3).contains(&self.max_active)
            || !(100..=5_000).contains(&self.cooldown_ms)
            || !(500..=8_000).contains(&self.lifetime_ms)
            || !(1..=3).contains(&self.intensity)
            || self.exclusions.len() > 11
        {
            return Err("reaction resource budget is outside the admitted range".into());
        }
        for (i, role) in self.exclusions.iter().enumerate() {
            if self.exclusions[..i].contains(role) {
                return Err("duplicate role exclusion".into());
            }
        }
        Ok(())
    }
    pub fn from_json(value: &str) -> Result<Self, String> {
        if value.len() > 2_048 {
            return Err("reaction configuration exceeds 2048 bytes".into());
        }
        let config: Self = serde_json::from_str(value).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub source_index: usize,
    pub at_ms: u64,
    pub role: Role,
    pub evidence: String,
}

/// Pure event-to-presentation reduction, capped at 128 source events and three cues.
/// Same-role cooldown and explicit exclusions do not suppress underlying events.
pub fn cues(
    steps: &[Step],
    at_ms: u64,
    mode: Mode,
    config: &ReactionConfig,
) -> Result<Vec<Cue>, String> {
    config.validate()?;
    if steps.len() > 128 || steps.windows(2).any(|p| p[0].at_ms > p[1].at_ms) {
        return Err("source trace exceeds budget or is out of order".into());
    }
    if mode == Mode::None {
        return Ok(Vec::new());
    }
    let mut accepted: Vec<Cue> = Vec::new();
    let mut failures = 0;
    for (source_index, step) in steps
        .iter()
        .enumerate()
        .take_while(|(_, s)| s.at_ms <= at_ms)
    {
        let (role, evidence) = match &step.event {
            SemanticEvent::SubagentSpawned { parent, child } => {
                (Role::Dispatch, format!("{parent} delegated to {child}"))
            }
            SemanticEvent::PlanUpdated { steps } => (
                Role::PlanAccepted,
                format!("{} bounded steps; one review artifact", steps.len()),
            ),
            SemanticEvent::ToolProgress { summary, .. } => (Role::Progress, summary.clone()),
            SemanticEvent::ToolFinished {
                success: false,
                summary,
                ..
            } => {
                failures += 1;
                (
                    if failures > 1 {
                        Role::RepeatedFailure
                    } else {
                        Role::ToolFailure
                    },
                    summary.clone(),
                )
            }
            SemanticEvent::Warning { text } => (Role::ScopeExplosion, text.clone()),
            SemanticEvent::ToolFinished {
                success: true,
                tool,
                summary,
                ..
            } => (
                if tool == "fixture.local_repair" {
                    Role::Recovery
                } else {
                    Role::BoringSuccess
                },
                summary.clone(),
            ),
            SemanticEvent::PermissionRequested { reason } => {
                (Role::PermissionRequest, reason.clone())
            }
            SemanticEvent::ArtifactProduced { name } => (Role::Artifact, name.clone()),
            SemanticEvent::SessionFinished => (
                Role::Synthesis,
                "Source events and explicit actions remain separate receipts".into(),
            ),
            _ => continue,
        };
        if config.exclusions.contains(&role)
            || (mode == Mode::Restrained && role.priority() < 3)
            || accepted
                .iter()
                .rev()
                .find(|c| c.role == role)
                .is_some_and(|c| step.at_ms.saturating_sub(c.at_ms) < config.cooldown_ms)
        {
            continue;
        }
        accepted.push(Cue {
            source_index,
            at_ms: step.at_ms,
            role,
            evidence,
        });
    }
    accepted.retain(|cue| at_ms.saturating_sub(cue.at_ms) <= config.lifetime_ms);
    accepted.sort_by_key(|cue| (cue.role.priority(), cue.at_ms, cue.source_index));
    accepted.reverse();
    accepted.truncate(if mode == Mode::Restrained {
        1
    } else {
        config.max_active
    });
    Ok(accepted)
}

/// Stable semantic registrations stay outside Node and outlive visual reflow.
pub const TARGETS: [(FocusId, &str); 4] = [
    (FocusId(101), "scope.local"),
    (FocusId(102), "scope.expedition"),
    (FocusId(201), "permission.allow"),
    (FocusId(202), "permission.deny"),
];

#[derive(Clone, Debug)]
pub struct Reactions {
    pub mode: Mode,
    pub metaphor: Metaphor,
    config: ReactionConfig,
    pub focus: FocusRing,
    actions: Vec<TimedAction>,
    pub input_notice: Option<String>,
}
impl Reactions {
    pub fn new(mode: Mode, metaphor: Metaphor, config: ReactionConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            mode,
            metaphor,
            config,
            focus: FocusRing::new(TARGETS.map(|(id, _)| id)),
            actions: Vec::new(),
            input_notice: None,
        })
    }
    pub fn actions(&self) -> &[TimedAction] {
        &self.actions
    }
    pub fn snapshot(&self, at_ms: u64) -> Snapshot {
        sem::snapshot(at_ms, &self.actions)
    }
    pub fn activate(&mut self, at_ms: u64, target: &str) -> Result<(), String> {
        let action = match target {
            "scope.local" => Action::ChooseScope { expedition: false },
            "scope.expedition" => Action::ChooseScope { expedition: true },
            "permission.allow" => Action::ResolvePermission { approved: true },
            "permission.deny" => Action::ResolvePermission { approved: false },
            _ => return Err("unregistered reaction action target".into()),
        };
        let limit = if matches!(action, Action::ResolvePermission { .. }) {
            sem::MAX_ACTIONS
        } else {
            sem::MAX_ACTIONS - 1
        };
        if self.actions.len() >= limit || self.actions.last().is_some_and(|a| a.at_ms > at_ms) {
            return Err("action history full or timestamp goes backwards".into());
        }
        sem::validate_action(&self.snapshot(at_ms), &action)?;
        self.actions.push(TimedAction { at_ms, action });
        Ok(())
    }
    fn target_available(state: &Snapshot, id: FocusId) -> bool {
        match id.0 {
            101 | 102 => state.scope_choice_available && !state.finished,
            201 | 202 => state.permission_pending,
            _ => false,
        }
    }
    fn next(&mut self, at_ms: u64, backwards: bool) {
        let state = self.snapshot(at_ms);
        for _ in 0..TARGETS.len() {
            let id = if backwards {
                self.focus.focus_prev()
            } else {
                self.focus.focus_next()
            };
            if id.is_some_and(|id| Self::target_available(&state, id)) {
                break;
            }
        }
    }
}
impl Default for Reactions {
    fn default() -> Self {
        Self::new(
            Mode::Restrained,
            Metaphor::Constellation,
            ReactionConfig::default(),
        )
        .unwrap()
    }
}

impl App for Reactions {
    fn key(&mut self, at_ms: u64, key: KeyEvent) {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return;
        }
        let target = match key.code {
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
                self.next(at_ms, false);
                None
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
                self.next(at_ms, true);
                None
            }
            KeyCode::Enter => TARGETS
                .iter()
                .find(|(id, _)| Some(*id) == self.focus.current())
                .map(|(_, target)| *target),
            KeyCode::Char('l' | 'L') => Some("scope.local"),
            KeyCode::Char('e' | 'E') => Some("scope.expedition"),
            KeyCode::Char('y' | 'Y') => Some("permission.allow"),
            KeyCode::Char('n' | 'N') => Some("permission.deny"),
            _ => None,
        };
        if let Some(target) = target {
            self.input_notice = self
                .activate(at_ms, target)
                .err()
                .map(|e| format!("ACTION REJECTED: {e}"));
        }
    }
    fn frame(&self, at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface {
        let (w, h) = ui::dimensions(w, h);
        let mut out = Surface::new(w, h);
        if w < 40 || h < 20 {
            ui::put(
                &mut out,
                0,
                0,
                "REACTIONS · enlarge to 40x20",
                Style::new().bold(),
            );
            return ui::finish(out, depth);
        }
        let state = self.snapshot(at_ms);
        let mut active = cues(&sem::fixture(), at_ms, self.mode, &self.config)
            .expect("validated config and fixed trace");
        // A denied artifact must never acquire a congratulatory visual claim.
        active.retain(|cue| {
            (cue.role != Role::Artifact || !state.artifacts.is_empty())
                && (cue.role != Role::PermissionRequest || state.permission_pending)
        });
        ui::put(
            &mut out,
            1,
            0,
            "REACTION DESK / semantics first, editorial second",
            Style::new().fg(Color::Cyan).bold(),
        );
        ui::put(
            &mut out,
            1,
            1,
            &format!(
                "{} · {} ordered events · {} explicit actions",
                if self.mode == Mode::None {
                    "None / plain affordances".into()
                } else {
                    format!("{:?} / {:?}", self.mode, self.metaphor)
                },
                state.event_order.len(),
                self.actions.len()
            ),
            Style::new().dim(),
        );
        if let Some(notice) = &self.input_notice {
            ui::put(
                &mut out,
                1,
                2,
                notice,
                Style::new().fg(Color::Yellow).bold(),
            );
        }
        let live = Node::panel(
            "THE ACTUAL SESSION",
            BorderType::Rounded,
            Style::new().fg(Color::Cyan),
        )
        .child(ui::text(&state.receipt, Color::White));
        ui::mount(&mut out, live, Rect::new(1, 3, w - 2, 5));
        let theater = Rect::new(1, 9, w - 2, h - 17);
        if let Some(cue) = active.first() {
            render_cue(
                &mut out,
                theater,
                cue,
                self.metaphor,
                at_ms,
                self.config.intensity,
            );
            if active.len() > 1 && theater.height >= 10 {
                ui::put(
                    &mut out,
                    3,
                    theater.y + theater.height - 2,
                    &format!("also / {}", active[1].role.headline()),
                    Style::new().dim(),
                );
            }
        } else {
            let quiet = Node::panel(
                "NO EDITORIAL OVERLAY",
                BorderType::Rounded,
                Style::new().dim(),
            )
            .child(ui::text(
                "The session remains fully operable. A reaction never owns your keyboard.",
                Color::White,
            ))
            .child(ui::text(
                format!(
                    "{} workers / {} artifacts / permission {:?}",
                    state.agents.len(),
                    state.artifacts.len(),
                    state.permission
                ),
                Color::Cyan,
            ));
            ui::mount(&mut out, quiet, theater);
        }
        let y = h - 7;
        if state.scope_choice_available || state.finished {
            ui::put(
                &mut out,
                2,
                y - 1,
                if state.scope_expedition {
                    "FOLLOW-UP DECISION: architecture expedition"
                } else if self
                    .actions
                    .iter()
                    .any(|a| a.at_ms <= at_ms && matches!(a.action, Action::ChooseScope { .. }))
                {
                    "FOLLOW-UP DECISION: local repair only"
                } else {
                    "FOLLOW-UP DECISION: not selected"
                },
                Style::new().fg(Color::Yellow).bold(),
            );
        }
        ui::put(
            &mut out,
            1,
            y,
            "EXPLICIT ACTIONS / same choices in every mode",
            Style::new().fg(Color::Yellow).bold(),
        );
        for (i, (id, target)) in TARGETS.iter().enumerate() {
            let available = Self::target_available(&state, *id);
            let label = match *target {
                "scope.local" => "L local repair follow-up",
                "scope.expedition" => "E architecture expedition",
                "permission.allow" => "Y allow local artifact",
                _ => "N deny local artifact",
            };
            let selected = self.focus.current() == Some(*id);
            let style = if available && selected {
                Style::new().bold().reverse().fg(Color::Yellow)
            } else if available {
                Style::new().fg(Color::White)
            } else {
                Style::new().dim()
            };
            ui::put(
                &mut out,
                2,
                y + 1 + i as u16,
                &format!(
                    "{} {label}{}",
                    if selected { ">" } else { " " },
                    if available { "" } else { " [unavailable]" }
                ),
                style,
            );
        }
        ui::put(
            &mut out,
            1,
            h - 1,
            "Tab/←/→ focus · Enter act · L/E scope · Y/N permit",
            Style::new().dim(),
        );
        ui::finish(out, depth)
    }
}

fn render_cue(
    out: &mut Surface,
    rect: Rect,
    cue: &Cue,
    metaphor: Metaphor,
    at_ms: u64,
    intensity: u8,
) {
    let color = if matches!(
        cue.role,
        Role::ToolFailure | Role::RepeatedFailure | Role::ScopeExplosion
    ) {
        Color::Magenta
    } else {
        Color::Green
    };
    let headline = if cue.role == Role::ScopeExplosion && metaphor == Metaphor::Paperwork {
        "REQUEST TO REQUEST A LARGER REQUEST"
    } else {
        cue.role.headline()
    };
    let header = Node::panel(
        format!("{:?} / source #{}", cue.role, cue.source_index),
        BorderType::Double,
        Style::new().fg(color),
    )
    .child(ui::text(headline, color))
    .child(ui::text(&cue.evidence, Color::White));
    ui::mount(out, header, rect);
    if rect.height < 9 {
        return;
    }
    let chart = Rect::new(rect.x + 2, rect.y + 5, rect.width - 4, rect.height - 7);
    let phase = (at_ms.saturating_sub(cue.at_ms).min(8_000) as f32 / 700.).sin();
    if cue.role == Role::ScopeExplosion && metaphor == Metaphor::Paperwork {
        // The same choice as the cosmic diagram, now a tiny bureaucratic machine.
        let mut paper = Surface::new(chart.width, chart.height);
        let n = (chart.height.saturating_sub(1) as usize).min(3);
        for i in 0..n {
            ui::put(
                &mut paper,
                i as u16 * 3,
                i as u16,
                "┌─ REQUEST TO REQUEST A LARGER REQUEST ─┐",
                Style::new().fg(color),
            );
        }
        ui::put(
            &mut paper,
            1,
            chart.height - 1,
            "[LOCAL REPAIR]    stamp: SCOPE NOT YET APPROVED",
            Style::new().bold().fg(Color::Yellow),
        );
        out.blit_transparent_clipped(&paper, chart.x as i32, chart.y as i32, chart);
        return;
    }
    let mut ink = BrailleCanvas::new(chart.width, chart.height);
    let cx = ink.pixel_width() as i32 / 2;
    let cy = ink.pixel_height() as i32 / 2;
    let radius = (cy - 1).max(2);
    let branches = match cue.role {
        Role::ScopeExplosion => 10,
        Role::Dispatch => 4,
        Role::PermissionRequest => 2,
        _ => 6,
    };
    for i in 0..branches {
        let theta = i as f32 * std::f32::consts::TAU / branches as f32;
        let rx = (cx as f32 * 0.72 + phase * intensity as f32).max(2.);
        let x = cx + (theta.cos() * rx) as i32;
        let y = cy + (theta.sin() * radius as f32) as i32;
        ink.line(cx, cy, x, y);
        ink.circle(x, y, 2);
    }
    ink.ellipse(cx, cy, (4. + phase.abs() * 3.) as i32, 2);
    out.blit_transparent_clipped(
        &ink.to_surface(Style::new().fg(color)),
        chart.x as i32,
        chart.y as i32,
        chart,
    );
    ui::put(
        out,
        chart.x + 1,
        chart.y + chart.height - 1,
        if cue.role == Role::ScopeExplosion {
            "ONE RECEIPT  ···  POSSIBLE NEW UNIVERSE"
        } else {
            "source event → visual emphasis; no hidden action"
        },
        Style::new().bold().fg(Color::Yellow),
    );
}

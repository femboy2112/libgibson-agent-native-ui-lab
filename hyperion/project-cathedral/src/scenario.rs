//! The deterministic catastrophic scenario ("the WTF moment"), driven by
//! LibGibson's `story` director.
//!
//! The story owns the *narrative timing*: a prologue, a deliberately minor
//! deployment fault, an unassisted cascade, then an operator response and a
//! recovery. Because the story is advanced with a fixed per-tick `Duration` and
//! reacts only to facts it sets itself, the whole scenario is deterministic and
//! journal-compatible: each beat's directive becomes an ordinary [`Action`]
//! appended to the same ordered journal a human operator would write.
//!
//! The story does **not** know about services, queues or circuit breakers. It
//! names *what kind* of move to make; the application chooses the target from the
//! live topology and performs it through the real simulation.

use std::time::Duration;

use gibson::story::{Beat, Facts, Story, StoryAction, StoryDirector};

use crate::action::ActionKind;
use crate::sim::{Engine, ServiceId};

/// One tick of simulated time at the default cadence.
pub const TICK: Duration = Duration::from_millis(50);

/// A directive produced by entering a story beat.
#[derive(Debug, Clone, Copy)]
pub struct Directive {
    pub kind: ActionKind,
    pub target: ServiceId,
}

/// The scripted incident director.
pub struct ScenarioDirector {
    story: Story,
    director: StoryDirector,
    last_beat: String,
    pub target: ServiceId,
    pub directives: Vec<Directive>,
}

impl ScenarioDirector {
    pub fn new(engine: &Engine) -> Self {
        let target = choose_target(engine);
        let story = Self::story();
        let director = story.start();
        let mut s = ScenarioDirector {
            story,
            director,
            last_beat: String::new(),
            target,
            directives: Vec::new(),
        };
        // Enter the prologue's facts.
        s.observe_beat();
        s
    }

    fn story() -> Story {
        Story::new("prologue")
            .beat(
                Beat::new("prologue")
                    .label("quiet operations")
                    .after(Duration::from_millis(3000), "trigger"),
            )
            .beat(
                Beat::new("trigger")
                    .label("a minor deployment lands")
                    .on_enter(StoryAction::set_text("scenario-action", "inject-fault"))
                    .after(Duration::from_millis(50), "cascade"),
            )
            .beat(
                Beat::new("cascade")
                    .label("unassisted propagation")
                    .after(Duration::from_millis(12_000), "diagnose"),
            )
            .beat(
                Beat::new("diagnose")
                    .label("operator acknowledges")
                    .on_enter(StoryAction::set_text("scenario-action", "acknowledge"))
                    .after(Duration::from_millis(2_000), "intervene"),
            )
            .beat(
                Beat::new("intervene")
                    .label("roll the deployment back")
                    .on_enter(StoryAction::set_text("scenario-action", "rollback"))
                    .after(Duration::from_millis(1_000), "reroute"),
            )
            .beat(
                Beat::new("reroute")
                    .label("shift traffic off the damaged region")
                    .on_enter(StoryAction::set_text("scenario-action", "reroute"))
                    .after(Duration::from_millis(12_000), "resolution"),
            )
            .beat(
                Beat::new("resolution")
                    .label("stability restored")
                    .on_enter(StoryAction::set_text("scenario-action", "acknowledge"))
                    .on_enter(StoryAction::set_bool("scenario-resolved", true))
                    .terminal(),
            )
    }

    /// Advance the story by one tick and return any new directives.
    pub fn tick(&mut self) -> Vec<Directive> {
        self.director.update(TICK, &[]);
        self.observe_beat();
        std::mem::take(&mut self.directives)
    }

    fn observe_beat(&mut self) {
        let beat = self.director.current_beat().to_string();
        if beat == self.last_beat {
            return;
        }
        self.last_beat = beat.clone();
        let facts: &Facts = self.director.facts();
        let action = facts.text("scenario-action").to_string();
        let kind = match action.as_str() {
            "inject-fault" => Some(ActionKind::InjectFault),
            "rollback" => Some(ActionKind::Rollback),
            "reroute" => Some(ActionKind::Reroute),
            "acknowledge" => Some(ActionKind::Acknowledge),
            "isolate" => Some(ActionKind::Isolate),
            "shed-load" => Some(ActionKind::ShedLoad),
            "" => None,
            _ => None,
        };
        if let Some(kind) = kind {
            self.directives.push(Directive {
                kind,
                target: self.target,
            });
        }
        // The intent is one-shot: clear it so that the *next* beat transition does
        // not re-emit the previous beat's action. (Facts otherwise persist for the
        // life of the director.)
        if !action.is_empty() {
            self.director.facts_mut().set_text("scenario-action", "");
        }
    }

    pub fn beat(&self) -> &str {
        self.director.current_beat()
    }

    pub fn beat_label(&self) -> &str {
        self.director.beat_label()
    }

    pub fn facts(&self) -> &Facts {
        self.director.facts()
    }

    pub fn resolved(&self) -> bool {
        self.director.is_finished() || self.facts().bool("scenario-resolved")
    }

    pub fn beat_sequence(&self) -> Vec<&str> {
        self.director.trace().beat_sequence()
    }

    pub fn elapsed(&self) -> Duration {
        self.director.elapsed()
    }

    pub fn _story(&self) -> &Story {
        &self.story
    }
}

/// Choose the deliberately minor-fault target: the deepest, most depended-upon
/// service (deterministically, lowest id on ties). A small perturbation there has
/// the widest structural reach.
pub fn choose_target(engine: &Engine) -> ServiceId {
    let mut indegree = vec![0u32; engine.len()];
    for s in &engine.fixture.services {
        for &d in &s.deps {
            indegree[d as usize] += 1;
        }
    }
    let mut best: ServiceId = 0;
    let mut best_score = -1i64;
    for s in &engine.fixture.services {
        // Prefer deep, highly depended-upon services; a "minor" fault with reach.
        let score = (indegree[s.id as usize] as i64) * 10 + s.tier as i64;
        if score > best_score {
            best_score = score;
            best = s.id;
        }
    }
    best
}

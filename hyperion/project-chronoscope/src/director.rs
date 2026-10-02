//! The atmosphere director: LibGibson's `Story` + `Scene` driven by *history time*.
//!
//! `StoryDirector` has no seek, no snapshot API and no inverse of `update`. Time travel is
//! therefore done the only way the public surface allows: the director is a pure function of
//! `(story, ordered (dt, events) steps)`, so the director *at position p of branch b* is
//! rebuilt from the nearest cloned checkpoint plus replay of the branch's own events. The
//! story clock is the history clock (one step = [`STEP_DT`]); wall-clock presentation (camera
//! glide, fork bloom) lives elsewhere and never enters the director.

use crate::epoch::*;
use crate::history::*;
use crate::vm::*;
use gibson::cell::Style;
use gibson::node::Node;
use gibson::scene::{
    Easing, Effect, EffectBundle, Presentation, Scene, SceneEntity, SceneId, SceneTarget,
};
use gibson::story::{Beat, Condition, Story, StoryAction, StoryDirector, StoryEvent};
use gibson::surface_fx::SurfaceFx;
use gibson::{Color, Surface};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

pub const STEP_DT: Duration = Duration::from_millis(100);
pub const STORY_CKPT_EVERY: u32 = 16;
pub const MAX_STORY_CKPTS: usize = 600;

fn beat_id(e: Epoch) -> &'static str {
    match e {
        Epoch::Stable => "stable",
        Epoch::Uncertain => "uncertain",
        Epoch::Escalating => "escalating",
        Epoch::Deadlock => "deadlock",
        Epoch::Contradiction => "contradiction",
        Epoch::Resolution => "resolution",
        Epoch::Convergence => "convergence",
        Epoch::Catastrophe => "catastrophe",
    }
}

pub fn epoch_of_beat(id: &str) -> Epoch {
    ALL_EPOCHS
        .iter()
        .copied()
        .find(|e| beat_id(*e) == id)
        .unwrap_or(Epoch::Stable)
}

#[derive(Clone, Copy, Debug)]
pub struct Ids {
    pub viewport: SceneId,
    pub banner: SceneId,
    /// The compare label lives on its own entity: nothing in the story targets it, so an
    /// input-flash reveal at the cursor's step cannot blink it out.
    pub compare: SceneId,
}

/// A shake that *settles*: `Effect::Shake` ignores its `duration` and never stops (see
/// FRICTION.md), so the same decaying jolt is composed from translations that end at (0, 0).
pub fn settling_shake(target: SceneTarget, amp: f32) -> Effect {
    let pts = [
        (1.0, -1.0),
        (-1.0, 1.0),
        (0.8, 0.6),
        (-0.6, -0.4),
        (0.3, 0.2),
        (0.0, 0.0),
    ];
    let mut prev = (0.0f32, 0.0f32);
    let mut seq = vec![];
    for (i, (x, y)) in pts.iter().enumerate() {
        let k = amp * (1.0 - i as f32 / pts.len() as f32);
        let next = (x * k, y * k);
        seq.push(Effect::translate(
            target,
            prev,
            next,
            Duration::from_millis(80),
        ));
        prev = next;
    }
    Effect::sequence(seq)
}

/// Build the one story: a beat per epoch, an arrow between every pair, in-beat reactions
/// for inputs / interventions / forks, and bundles for the transient flashes.
pub fn build_story(ids: Ids) -> Story {
    let vp = SceneTarget::Id(ids.viewport);
    let bn = SceneTarget::Id(ids.banner);
    let mut story = Story::new("stable");
    for &e in ALL_EPOCHS.iter() {
        let id = beat_id(e);
        let mut beat = Beat::new(id)
            .label(e.name())
            .on_enter(StoryAction::set_text("epoch", id));
        beat = beat.on_enter(StoryAction::set_bool(
            "catastrophe",
            e == Epoch::Catastrophe,
        ));
        beat = match e {
            Epoch::Stable => beat,
            Epoch::Uncertain => beat.effect(
                Effect::scanline(vp, Style::new().dim(), Duration::from_millis(2600)).looping(),
            ),
            Epoch::Escalating => beat.effect(
                Effect::jitter(
                    vp,
                    0.6,
                    Duration::from_millis(220),
                    Duration::from_millis(2200),
                )
                .looping(),
            ),
            Epoch::Deadlock => beat
                .effect(
                    Effect::scanline(vp, Style::new().dim().italic(), Duration::from_millis(4200))
                        .looping(),
                )
                .effect(
                    Effect::reveal(bn, 0.0, 1.0, Duration::from_millis(500)).eased(Easing::EaseOut),
                ),
            Epoch::Contradiction => beat
                .effect(Effect::post_process(
                    vp,
                    SurfaceFx::RowShift {
                        amount: 0,
                        seed: 0xC0DE,
                    },
                ))
                .effect(
                    Effect::reveal(bn, 0.0, 1.0, Duration::from_millis(300)).eased(Easing::EaseOut),
                ),
            Epoch::Resolution => beat
                .effect(
                    Effect::dissolve(vp, 0.35, 1.0, 7, Duration::from_millis(1100))
                        .eased(Easing::EaseOut),
                )
                .effect(
                    Effect::reveal(bn, 0.0, 1.0, Duration::from_millis(400)).eased(Easing::EaseOut),
                ),
            Epoch::Convergence => beat.effect(
                Effect::reveal(bn, 0.0, 1.0, Duration::from_millis(400)).eased(Easing::EaseOut),
            ),
            Epoch::Catastrophe => beat.effect(settling_shake(vp, 2.0)).effect(
                Effect::reveal(bn, 0.0, 1.0, Duration::from_millis(250)).eased(Easing::EaseOut),
            ),
        };
        for &other in ALL_EPOCHS.iter() {
            if other != e {
                beat = beat.transition(
                    Condition::on(StoryEvent::custom(format!("epoch:{}", beat_id(other)))),
                    beat_id(other),
                );
            }
        }
        beat = beat
            .reaction(
                Condition::on(StoryEvent::custom("input")),
                [StoryAction::mount("input-flash")],
            )
            .reaction(
                Condition::on(StoryEvent::custom("override")),
                [StoryAction::mount("override-flash")],
            )
            .reaction(
                Condition::on(StoryEvent::custom("fork")),
                [
                    StoryAction::mount("fork-birth"),
                    StoryAction::set_bool("forked", true),
                ],
            );
        story = story.beat(beat);
    }
    story = story
        .bundle(EffectBundle::new("input-flash").effect(
            Effect::reveal(bn, 0.0, 1.0, Duration::from_millis(250)).eased(Easing::EaseOut),
        ))
        .bundle(EffectBundle::new("override-flash").effect(
            Effect::dissolve(vp, 0.5, 1.0, 11, Duration::from_millis(600)).eased(Easing::EaseOut),
        ))
        .bundle(EffectBundle::new("fork-birth").effect(
            Effect::dissolve(vp, 0.15, 1.0, 3, Duration::from_millis(1200)).eased(Easing::EaseOut),
        ));
    story
}

/// Story events for step `s` of branch `b` (what the director is told happened).
pub fn events_for(h: &History, b: BranchId, s: u32) -> Vec<StoryEvent> {
    let mut out = vec![];
    let Some(r) = h.rec_at(b, s) else { return out };
    let prev = if s == 0 {
        Epoch::Stable
    } else {
        h.rec_at(b, s - 1).map(|p| p.epoch).unwrap_or(Epoch::Stable)
    };
    if r.epoch != prev {
        out.push(StoryEvent::custom(format!("epoch:{}", beat_id(r.epoch))));
    }
    if r.ev.flags & F_INPUT != 0 {
        out.push(StoryEvent::custom("input"));
    }
    if r.ev.flags & F_OVERRIDDEN != 0 {
        out.push(StoryEvent::custom("override"));
    }
    let br = h.branch(b);
    if br.parent.is_some() && s == br.fork_at {
        out.push(StoryEvent::custom("fork"));
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct DirectorStats {
    pub replays: u64,
    pub replayed_updates: u64,
    pub clones: u64,
    pub checkpoint_hits: u64,
    pub checkpoints: u64,
    pub evicted: u64,
}

pub struct Atmosphere {
    pub scene: Scene,
    pub ids: Ids,
    story: Story,
    /// Checkpoints with a last-used stamp (evicted least-recently-used, never by branch id).
    ckpts: BTreeMap<(BranchId, u32), (StoryDirector, u64)>,
    clock: u64,
    pub stats: DirectorStats,
}

impl Atmosphere {
    pub fn new() -> Atmosphere {
        let mut scene = Scene::new();
        let viewport = scene.add(
            SceneEntity::new("viewport", Node::surface(Arc::new(Surface::new(1, 1))))
                .tag("world")
                .z(0),
        );
        let banner = scene.add(
            SceneEntity::new("banner", Node::text("", Style::new().bold()))
                .tag("hud")
                .z(5),
        );
        let compare = scene.add(
            SceneEntity::new(
                "compare",
                Node::text(" COMPARE ", Style::new().bold().reverse()),
            )
            .tag("hud")
            .z(6),
        );
        if let Some(e) = scene.entity_mut(compare) {
            e.visible = false;
        }
        let ids = Ids {
            viewport,
            banner,
            compare,
        };
        let story = build_story(ids);
        story.validate().expect("story is well-formed");
        Atmosphere {
            scene,
            ids,
            story,
            ckpts: BTreeMap::new(),
            clock: 0,
            stats: DirectorStats::default(),
        }
    }

    pub fn fresh(&self) -> StoryDirector {
        self.story.start()
    }

    /// The director at *position* `pos` of branch `b`: `pos` updates have been applied.
    /// Exact by construction; cost is bounded by [`STORY_CKPT_EVERY`] plus one clone.
    pub fn director_at(&mut self, h: &History, b: BranchId, pos: u32) -> StoryDirector {
        let br = h.branch(b);
        if pos < br.fork_at {
            return self.director_at(
                h,
                br.parent.expect("a branch with fork_at>0 has a parent"),
                pos,
            );
        }
        let want = pos - pos % STORY_CKPT_EVERY;
        // nearest checkpoint at or below `pos` on this branch, else the fork point's parent state
        self.clock += 1;
        let stamp = self.clock;
        let start = self
            .ckpts
            .range_mut((b, br.fork_at)..=(b, pos))
            .next_back()
            .map(|(k, (d, used))| {
                *used = stamp;
                (k.1, d.clone())
            });
        let (mut at, mut d) = match start {
            Some((p, d)) => {
                self.stats.checkpoint_hits += 1;
                self.stats.clones += 1;
                (p, d)
            }
            None => {
                let base = match br.parent {
                    Some(p) if br.fork_at > 0 => self.director_at(h, p, br.fork_at),
                    _ => self.story.start(),
                };
                (br.fork_at, base)
            }
        };
        self.stats.replays += 1;
        while at < pos {
            let evs = events_for(h, b, at);
            d.update(STEP_DT, &evs);
            self.stats.replayed_updates += 1;
            at += 1;
            if at % STORY_CKPT_EVERY == 0
                && at <= want.max(pos)
                && !self.ckpts.contains_key(&(b, at))
            {
                self.ckpts.insert((b, at), (d.clone(), stamp));
                self.stats.checkpoints += 1;
                self.stats.clones += 1;
                // bounded: least-recently-used first (a checkpoint is only a cache of replay)
                while self.ckpts.len() > MAX_STORY_CKPTS {
                    let victim = self
                        .ckpts
                        .iter()
                        .min_by_key(|(_, (_, used))| *used)
                        .map(|(k, _)| *k)
                        .expect("non-empty");
                    self.ckpts.remove(&victim);
                    self.stats.evicted += 1;
                }
            }
        }
        d
    }

    /// Drop checkpoints of a branch (called when a branch is fossilized).
    pub fn forget(&mut self, b: BranchId) {
        let keys: Vec<_> = self
            .ckpts
            .range((b, 0)..=(b, u32::MAX))
            .map(|(k, _)| *k)
            .collect();
        for k in keys {
            self.ckpts.remove(&k);
        }
    }

    pub fn checkpoint_count(&self) -> usize {
        self.ckpts.len()
    }

    pub fn presentation(&self, d: &StoryDirector) -> Presentation {
        d.presentation(&self.scene)
    }

    /// Compose the scene for this frame: viewport surface + banner, effects applied.
    pub fn compose(
        &mut self,
        d: &StoryDirector,
        viewport: Arc<Surface>,
        banner: &str,
        comparing: bool,
        w: u16,
        h: u16,
    ) -> (Node, Presentation) {
        let p = self.presentation(d);
        self.scene
            .set_node(self.ids.viewport, Node::surface(viewport));
        let bx = (w as i32 - banner.chars().count() as i32) / 2;
        self.scene.set_node(
            self.ids.banner,
            Node::text(banner.to_string(), Style::new().bold().reverse()),
        );
        if let Some(e) = self.scene.entity_mut(self.ids.banner) {
            e.offset = (bx.max(0), 0);
            e.visible = !banner.is_empty() && !comparing;
        }
        const COMPARE_LABEL: &str = " COMPARE ";
        if let Some(e) = self.scene.entity_mut(self.ids.compare) {
            e.offset = ((w as i32 - COMPARE_LABEL.chars().count() as i32) / 2, 0);
            e.visible = comparing;
        }
        let node = self.scene.to_node(&p, w as f32, h as f32);
        (node, p)
    }
}

impl Default for Atmosphere {
    fn default() -> Self {
        Atmosphere::new()
    }
}

pub fn banner_for(e: Epoch, facts_forked: bool) -> String {
    match e {
        Epoch::Deadlock => " DEADLOCK ".into(),
        Epoch::Contradiction => " CONTRADICTION ".into(),
        Epoch::Catastrophe => " M E L T D O W N ".into(),
        Epoch::Resolution => " RESOLVED ".into(),
        Epoch::Convergence => " CONVERGED ".into(),
        _ if facts_forked => String::new(),
        _ => String::new(),
    }
}

#[allow(dead_code)]
fn _unused(_: Color) {}

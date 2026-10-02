//! Capability probes: what LibGibson v0.4.0's public API does when pushed backwards.
//!
//! Each test asserts the *observed* behaviour and says which FRICTION.md entry it backs.
//! Some assert behaviour we consider a defect or a gap: they pass today and will fail the day
//! upstream changes it, which is the point (the failure message tells you what to re-read).

use gibson::audio::human_music::semantic::*;
use gibson::audio::human_music::{compose, MusicWorld, WorldId};
use gibson::node::Node;
use gibson::scene::*;
use gibson::ui::prelude::{button, screen, skins, UiEnvironment, UiRuntime};
use gibson::{Context, RenderMode, Style};
use project_chronoscope::director::*;
use std::time::{Duration, Instant};

fn scene_with_one() -> (Scene, SceneId) {
    let mut sc = Scene::new();
    let id = sc.add(SceneEntity::new(
        "v",
        Node::text("hello world", Style::new()),
    ));
    (sc, id)
}

/// FRICTION: PROBABLE LIBGIBSON DEFECT — `Effect::Shake`/`Effect::Jitter` carry a `duration`
/// but never settle: evaluated after the duration they keep displacing the entity forever.
#[test]
fn finite_shake_and_jitter_never_settle() {
    let (sc, id) = scene_with_one();
    let t = SceneTarget::Id(id);
    let d = Duration::from_millis(500);
    let shake = Effect::shake(t, 2.0, Duration::from_millis(70), d);
    let jitter = Effect::jitter(t, 2.0, Duration::from_millis(70), d);
    let mut nonzero_shake = 0;
    let mut nonzero_jitter = 0;
    for ms in (5_000..5_700).step_by(7) {
        let mut p = Presentation::new();
        shake.eval(Duration::from_millis(ms), &sc, &mut p);
        nonzero_shake += (p.offset_of(&sc, id) != (0, 0)) as u32;
        let mut p = Presentation::new();
        jitter.eval(Duration::from_millis(ms), &sc, &mut p);
        nonzero_jitter += (p.displacement_of(id) != (0, 0)) as u32;
    }
    assert!(
        nonzero_shake > 50,
        "a 500 ms shake still displaces the entity 10 s later ({nonzero_shake}/100 samples)"
    );
    assert!(
        nonzero_jitter > 50,
        "a 500 ms jitter still displaces the entity 10 s later ({nonzero_jitter}/100 samples)"
    );
    // the finite effects that *do* settle, for contrast
    let reveal = Effect::reveal(t, 0.0, 1.0, d);
    let mut p = Presentation::new();
    reveal.eval(Duration::from_secs(10), &sc, &mut p);
    assert_eq!(p.raw_visibility(id), Some(1.0));
}

/// FRICTION: POSITIVE CAPABILITY — effects are pure functions of time, and `Effect::Reverse`
/// plays one backwards: repositioning an animation needs no state.
#[test]
fn effects_reposition_and_reverse_exactly() {
    let (sc, id) = scene_with_one();
    let t = SceneTarget::Id(id);
    let fwd = Effect::translate(t, (0.0, 0.0), (10.0, 4.0), Duration::from_millis(1000));
    let rev = Effect::Reverse(Box::new(fwd.clone()));
    for ms in [0u64, 100, 400, 777, 1000] {
        let mut a = Presentation::new();
        fwd.eval(Duration::from_millis(1000 - ms), &sc, &mut a);
        let mut b = Presentation::new();
        rev.eval(Duration::from_millis(ms), &sc, &mut b);
        assert_eq!(
            a.offset_of(&sc, id),
            b.offset_of(&sc, id),
            "reverse(t={ms}) == forward(d-t)"
        );
    }
}

/// FRICTION: GENERIC PRIMITIVE GAP — a `Scene` can only grow. There is no `remove`; the only
/// way to retire an entity is to hide it, and every hidden entity is still evaluated (and its
/// node cloned) on every frame.
#[test]
fn scene_entities_cannot_be_removed_and_cost_accumulates() {
    let mut sc = Scene::new();
    let mut times = vec![];
    for n in [10usize, 400, 2000] {
        while sc.len() < n {
            let i = sc.len();
            let id = sc.add(SceneEntity::new(
                format!("branch-{i}"),
                Node::text(format!("ghost {i}"), Style::new()),
            ));
            // the best a consumer can do to "remove" it:
            sc.entity_mut(id).unwrap().visible = false;
        }
        let p = Presentation::new();
        let t0 = Instant::now();
        for _ in 0..10 {
            let _ = sc.to_node(&p, 80.0, 24.0);
        }
        times.push((n, t0.elapsed() / 10));
    }
    assert_eq!(sc.len(), 2000, "hidden entities are still entities");
    let evaluated = sc.evaluate(&Presentation::new()).len();
    assert_eq!(evaluated, 2000, "…and still evaluated every frame");
    println!("scene to_node cost by entity count: {times:?}");
    assert!(
        times[2].1 > times[0].1 * 20,
        "cost grows with every retired entity"
    );
}

/// FRICTION: DELIBERATE SAFETY BOUNDARY — `UiRuntime` clamps time backwards: presentation
/// time is monotone, so history time must never be fed to it.
#[test]
fn ui_runtime_time_is_monotone_by_design() {
    let mut rt: UiRuntime<()> = UiRuntime::new(skins::BLACK_ICE);
    let env = UiEnvironment::default();
    let tree = screen::<()>().child(button("x").key("x").on_press(()));
    rt.frame(&tree, env, Duration::from_secs(5)).unwrap();
    let cx = rt.build_cx(env, Duration::from_secs(1));
    assert_eq!(
        cx.time,
        Duration::from_secs(5),
        "a backwards sample is clamped to the previous time"
    );
}

/// FRICTION: POSITIVE — the story director is cheap to snapshot (so checkpoint/replay rewind
/// is practical), but it has no seek/restore/inverse of its own.
#[test]
fn story_director_snapshots_are_cheap_but_there_is_no_restore_api() {
    let a = Atmosphere::new();
    let mut d = a.fresh();
    for _ in 0..640 {
        d.update(STEP_DT, &[]);
    }
    let t0 = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(d.clone());
    }
    let per = t0.elapsed() / 200;
    println!("StoryDirector::clone after 640 updates: {per:?}");
    assert!(
        per < Duration::from_micros(500),
        "clone is cheap enough to checkpoint every 16 steps ({per:?})"
    );
    // draining the trace (the only bounded-memory tool) makes replay-from-zero impossible
    let drained = d.drain_trace();
    assert_eq!(drained.steps.len(), 640);
    assert!(
        !d.trace().is_complete(),
        "after drain_trace the live trace is not a from-zero record"
    );
}

/// FRICTION: GENERIC ERGONOMIC GAP — what an outside observer can read back about a frame.
#[test]
fn context_exposes_text_and_counters_but_not_cell_styles() {
    use gibson::context::Context as Ctx;
    let mut ctx = Ctx::headless(RenderMode::Fullscreen, 40, 6);
    ctx.set_root(Node::col().child(Node::text(
        "hello",
        Style::new().fg(gibson::Color::Rgb(255, 0, 0)),
    )));
    ctx.set_capture_damage(true);
    ctx.render_now().unwrap();
    let lines: Vec<String> = ctx.last_frame_lines();
    assert!(lines[0].contains("hello"));
    let rep = ctx.last_frame_report();
    assert!(rep.bytes_emitted > 0 && rep.exact_changed_cells >= 5 && rep.total_cells == 40 * 6);
    assert!(!ctx.last_dirty_cells().is_empty());
    let stats = ctx.stats();
    assert_eq!(stats.frames, 1);
    // colours/attributes of the composed frame are only observable by parsing the wire bytes
    let mut vt = vt100::Parser::new(6, 40, 0);
    vt.process(ctx.rendered_bytes());
    let cell = vt.screen().cell(0, 0).unwrap();
    assert_eq!(
        cell.fgcolor(),
        vt100::Color::Rgb(255, 0, 0),
        "style is recoverable only through a VT emulator"
    );
}

/// FRICTION: POSITIVE — headless contexts can be resized at will, which is what makes
/// "resize during scrub/fork" testable without a PTY.
#[test]
fn headless_context_resizes() {
    let mut ctx = Context::headless(RenderMode::Fullscreen, 80, 24);
    assert_eq!(ctx.session.terminal_size(), (80, 24));
    ctx.session.set_terminal_size(42, 15);
    assert_eq!(ctx.session.terminal_size(), (42, 15));
}

/// FRICTION: POSITIVE (sampled, not proven) — HumanMusic composition survives arbitrary
/// semantic traces: time travel produces traces nobody hand-authored.
#[test]
fn compose_survives_random_semantic_traces() {
    struct R(u64);
    impl R {
        fn b(&mut self, k: u64) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) % k
        }
    }
    let tones = [
        Tone::Neutral,
        Tone::Accent,
        Tone::Info,
        Tone::Success,
        Tone::Warning,
        Tone::Danger,
    ];
    let emph = [
        Emphasis::Faint,
        Emphasis::Muted,
        Emphasis::Normal,
        Emphasis::Strong,
    ];
    let dens = [Density::Compact, Density::Normal, Density::Spacious];
    let elev = [Elevation::Flat, Elevation::Raised, Elevation::Overlay];
    let kinds = [
        EventKind::Prolong,
        EventKind::ToneShift,
        EventKind::FocusAcquired,
        EventKind::SectionResolved,
        EventKind::ActChanged,
        EventKind::ModalEntered,
        EventKind::Impact,
        EventKind::Confirmation,
    ];
    let worlds = [WorldId::BlackIce, WorldId::Vapor95, WorldId::SwissSignal];
    let mut r = R(0xF00D_1234_5678_9ABC);
    for _ in 0..600 {
        let n = 1 + r.b(8) as usize;
        let total = 0.5 + r.b(1200) as f64 / 20.0;
        let ev: Vec<SemanticEvent> = (0..n)
            .map(|_| SemanticEvent {
                at_beat: r.b((total * 20.0) as u64 + 1) as f64 / 20.0,
                state: SemanticState {
                    tone: tones[r.b(6) as usize],
                    emphasis: emph[r.b(4) as usize],
                    density: dens[r.b(3) as usize],
                    elevation: elev[r.b(3) as usize],
                },
                kind: kinds[r.b(8) as usize],
            })
            .collect();
        let w = MusicWorld::from_id(worlds[r.b(3) as usize]);
        let score = compose(&SemanticTrace::new(ev, total), &w, r.b(u64::MAX));
        score.validate().expect("every composed score validates");
    }
}

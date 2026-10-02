// Minimal reproduction: Effect::Shake / Effect::Jitter carry a `duration` but never settle.
// libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0" }
use gibson::cell::Style;
use gibson::node::Node;
use gibson::scene::{Effect, Presentation, Scene, SceneEntity, SceneTarget};
use std::time::Duration;

fn main() {
    let mut scene = Scene::new();
    let id = scene.add(SceneEntity::new("e", Node::text("x", Style::new())));
    let t = SceneTarget::Id(id);
    let shake = Effect::shake(t, 2.0, Duration::from_millis(70), Duration::from_millis(500));
    let jitter = Effect::jitter(t, 2.0, Duration::from_millis(70), Duration::from_millis(500));
    let reveal = Effect::reveal(t, 0.0, 1.0, Duration::from_millis(500));
    for ms in [0u64, 250, 500, 5_000, 60_000] {
        let (mut a, mut b, mut c) = (Presentation::new(), Presentation::new(), Presentation::new());
        shake.eval(Duration::from_millis(ms), &scene, &mut a);
        jitter.eval(Duration::from_millis(ms), &scene, &mut b);
        reveal.eval(Duration::from_millis(ms), &scene, &mut c);
        println!(
            "t={ms:>6} ms  shake offset={:?}  jitter displacement={:?}  reveal visibility={:?}",
            a.offset_of(&scene, id),
            b.displacement_of(id),
            c.raw_visibility(id)
        );
    }
    // Expected (per the variant docs "overriding position for `duration`" and per every other
    // finite effect, which clamps progress): at t >= 500 ms shake/jitter contribute nothing.
    // Observed: still non-zero at 60 s.
}

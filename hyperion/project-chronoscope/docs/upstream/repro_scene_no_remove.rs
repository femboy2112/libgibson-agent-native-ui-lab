// Minimal reproduction: a Scene can only grow. There is no way to retire an entity; hiding it
// does not stop it being evaluated (and its Node cloned) on every frame.
use gibson::cell::Style;
use gibson::node::Node;
use gibson::scene::{Presentation, Scene, SceneEntity};
use std::time::Instant;

fn main() {
    let mut scene = Scene::new();
    for n in [10usize, 400, 2_000, 5_000] {
        while scene.len() < n {
            let i = scene.len();
            let id = scene.add(SceneEntity::new(format!("e{i}"), Node::text(format!("entity {i}"), Style::new())));
            scene.entity_mut(id).unwrap().visible = false; // the closest thing to "removed"
        }
        let p = Presentation::new();
        let t0 = Instant::now();
        for _ in 0..20 {
            std::hint::black_box(scene.to_node(&p, 80.0, 24.0));
        }
        println!(
            "{n:>5} retired entities: scene.len()={} evaluate().len()={} to_node = {:?}/call",
            scene.len(),
            scene.evaluate(&p).len(),
            t0.elapsed() / 20
        );
    }
    // Expected: some way to remove an entity (or at least to exclude hidden ones from
    // evaluate()). Observed: len() and evaluate() keep every entity; cost is linear and then
    // some (2,000 hidden entities ≈ 3 ms per to_node; one frame budget at 60 fps is 16 ms).
}

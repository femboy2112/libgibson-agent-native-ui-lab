//! Developer tool: how long does one composed frame take (analysis already cached)?
use pulsar::render::{compose, Env};
use pulsar::session::{secs_to_samples, Cmd, Model, View};
use std::time::Instant;
fn main() {
    let mut m = Model::new(pulsar::scenario::DEFAULT_SEED);
    m.engine.checkpoint(pulsar::scenario::N_CP); // warm the analysis
    for (w, h) in [(120u16, 40u16), (80, 24), (42, 15)] {
        for v in View::ALL {
            m.apply(Cmd::Seek(secs_to_samples(300.0)));
            m.apply(Cmd::SetView(v));
            m.apply(Cmd::Frame(255));
            let env = Env::new(w, h);
            let t0 = Instant::now();
            let n = 20;
            for i in 0..n {
                m.apply(Cmd::Advance(i));
                let _ = compose(&m, &env);
            }
            println!("{w}x{h} {:9}: {:6.2} ms/frame", v.name(), t0.elapsed().as_secs_f64() * 1000.0 / n as f64);
        }
    }
}

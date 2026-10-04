//! Developer tool: print one composed frame (no terminal needed).
//!   cargo run --release --example peek -- seed t view [WxH] [mono] [select=a|b|c] [focus]
use pulsar::render::{compose, Env};
use pulsar::session::{Cmd, Model, View};
use pulsar::scenario::SigId;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: u64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(62);
    let t: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(200.0);
    let view = View::parse(a.get(3).map(|s| s.as_str()).unwrap_or("stream")).unwrap();
    let (w, h) = a.get(4).and_then(|s| s.split_once('x')).map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())).unwrap_or((120, 40));
    let mut env = Env::new(w, h);
    let mut m = Model::new(seed);
    m.apply(Cmd::Seek(pulsar::session::secs_to_samples(t)));
    m.apply(Cmd::SetView(view));
    m.apply(Cmd::Frame(99));
    for x in a.iter().skip(5) {
        match x.as_str() {
            "mono" => env = env.mono(),
            "focus" => m.apply(Cmd::ToggleFocus),
            "compare" => m.apply(Cmd::ToggleCompare),
            "inspect" => m.apply(Cmd::ToggleCursor),
            "truth" => m.apply(Cmd::ToggleTruth),
            "select=b" => m.apply(Cmd::Select(SigId::Beta)),
            "select=c" => m.apply(Cmd::Select(SigId::Gamma)),
            s if s.starts_with("zoom") => m.apply(Cmd::Zoom(1)),
            _ => {}
        }
    }
    let f = compose(&m, &env);
    for l in f.status.to_visible_lines() { println!("{l}"); }
    for l in f.hero.to_visible_lines() { println!("{l}"); }
    for l in f.hint.to_visible_lines() { println!("{l}"); }
}

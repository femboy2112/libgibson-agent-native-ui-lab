//! Developer tool: write the raw ANSI stream of one frame (for rendering to an image
//! outside the project, to *look* at colour). `cargo run --example ansi_dump -- T VIEW WxH [script]`
use pulsar::app::render_frame;
use pulsar::render::Env;
use pulsar::session::{parse_script, secs_to_samples, Cmd, Model, View};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t: f64 = a[1].parse().unwrap();
    let view = View::parse(&a[2]).unwrap();
    let (w, h) = a[3].split_once('x').map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())).unwrap();
    let mut m = Model::new(62);
    m.apply(Cmd::Seek(secs_to_samples(t)));
    m.apply(Cmd::SetView(view));
    if let Some(s) = a.get(4) { for c in parse_script(s).unwrap() { m.apply(c); } }
    m.apply(Cmd::Frame(255));
    print!("{}", render_frame(&m, &Env::new(w, h)).unwrap().ansi);
}

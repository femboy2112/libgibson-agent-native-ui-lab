#![allow(dead_code)]
use pulsar::render::{compose, Env, Frame, Probe, PlotProbe};
use pulsar::scenario::{SigId, DEFAULT_SEED};
use pulsar::session::{secs_to_samples, Cmd, Model, View};

pub const SEED: u64 = DEFAULT_SEED;

pub fn model_at(seed: u64, t: f64, view: View) -> Model {
    let mut m = Model::new(seed);
    m.apply(Cmd::Seek(secs_to_samples(t)));
    m.apply(Cmd::SetView(view));
    m.apply(Cmd::Frame(255));
    m
}

pub fn frame(m: &Model, w: u16, h: u16) -> Frame {
    compose(m, &Env::new(w, h))
}

pub fn text(f: &Frame) -> String {
    let mut s = String::new();
    for l in f.status.to_visible_lines().iter().chain(f.hero.to_visible_lines().iter()).chain(f.hint.to_visible_lines().iter()) {
        s.push_str(l);
        s.push('\n');
    }
    s
}

pub fn plots(f: &Frame) -> Vec<&PlotProbe> {
    f.probes
        .iter()
        .filter_map(|p| match p {
            Probe::Plot(pp) => Some(pp),
            _ => None,
        })
        .collect()
}

pub fn plot<'a>(f: &'a Frame, name: &str) -> &'a PlotProbe {
    plots(f)
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no plot probe `{name}`"))
}

pub fn ids() -> [SigId; 3] {
    SigId::ALL
}

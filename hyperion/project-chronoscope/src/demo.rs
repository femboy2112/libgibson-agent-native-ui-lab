//! `--demo`: the guided "WTF moment", driven through the same commands a user would type.
//! Any key press hands control back immediately.
//!
//! Run to the catastrophe, rewind (the camera turns to face the past), change ONE input, watch a
//! second future grow beside the dead one, compare them (dimensions pull apart, the cause plane
//! glows) and A/B the two musical histories.

use crate::app::*;
use crate::history::Edit;

#[derive(Clone, Debug)]
pub enum DemoStep {
    Toast(&'static str),
    Wait(f32),
    Cmd(Cmd),
    /// Run forward at `speed` index until the end of the branch.
    RunToEnd(usize),
    /// Scrub backwards continuously (steps per second) until the position is reached.
    RewindTo(u32, f32),
    /// Scrub forwards continuously until the position is reached.
    ScrubTo(u32, f32),
    Fork(Edit),
}

pub struct Demo {
    steps: Vec<DemoStep>,
    idx: usize,
    wait: f32,
    acc: f32,
}

impl Demo {
    pub fn new() -> Demo {
        use DemoStep::*;
        let steps = vec![
            Toast("CHRONOSCOPE · a reactor colony, recorded. Any key takes over."),
            Wait(1.8),
            Cmd(crate::app::Cmd::Jump(Jump::Start)),
            RunToEnd(3),
            Wait(2.6),
            Toast("catastrophe at step 344. Rewinding…"),
            RewindTo(50, 70.0),
            Wait(1.2),
            Toast("the first BOOST set this off? change ONE input: BOOST → THROTTLE"),
            Wait(1.6),
            Fork(Edit::ReplaceCmd(2)),
            Wait(2.2),
            RunToEnd(3),
            Wait(1.4),
            Cmd(crate::app::Cmd::Jump(Jump::Start)),
            Toast("compare: identical dimensions fuse, divergent ones pull apart"),
            Cmd(crate::app::Cmd::ToggleCompare),
            ScrubTo(330, 60.0),
            Wait(2.0),
            Cmd(crate::app::Cmd::ToggleAb),
            Wait(1.5),
            Toast("A/B the two histories (a)  ·  fork again with f  ·  q quits"),
        ];
        Demo {
            steps,
            idx: 0,
            wait: 0.0,
            acc: 0.0,
        }
    }

    pub fn finished(&self) -> bool {
        self.idx >= self.steps.len()
    }

    /// Advance by `dt` presentation seconds, issuing commands to the model.
    pub fn tick(&mut self, m: &mut Model, dt: f32) {
        if self.wait > 0.0 {
            self.wait -= dt;
            return;
        }
        let Some(step) = self.steps.get(self.idx).cloned() else {
            return;
        };
        match step {
            DemoStep::Toast(t) => {
                m.toast(t);
                self.idx += 1;
            }
            DemoStep::Wait(s) => {
                self.wait = s;
                self.idx += 1;
            }
            DemoStep::Cmd(c) => {
                m.do_cmd(c);
                self.idx += 1;
            }
            DemoStep::RunToEnd(speed) => {
                if m.speed != speed {
                    m.speed = speed;
                }
                if m.cursor >= m.end() {
                    self.idx += 1;
                } else if !m.playing {
                    m.do_cmd(Cmd::Play);
                }
            }
            DemoStep::RewindTo(p, rate) => {
                m.playing = false;
                self.acc += rate * dt;
                let n = self.acc as i32;
                self.acc -= n as f32;
                if m.cursor > p {
                    m.step(-(n.max(1)).min((m.cursor - p) as i32));
                }
                if m.cursor <= p {
                    self.idx += 1;
                }
            }
            DemoStep::ScrubTo(p, rate) => {
                m.playing = false;
                self.acc += rate * dt;
                let n = self.acc as i32;
                self.acc -= n as f32;
                if m.cursor < p {
                    m.step((n.max(1)).min((p - m.cursor) as i32));
                }
                if m.cursor >= p {
                    self.idx += 1;
                }
            }
            DemoStep::Fork(e) => {
                if let Err(err) = m.fork_with(e) {
                    m.toast(format!("demo: cannot fork here ({err})"));
                }
                self.idx += 1;
            }
        }
    }
}

impl Default for Demo {
    fn default() -> Self {
        Demo::new()
    }
}

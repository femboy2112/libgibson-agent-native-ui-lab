//! The sustained deterministic workload: thousands of rendered frames through the real
//! model/view/runtime/renderer stack while an autopilot scrubs, jumps, forks, switches,
//! compares, resizes and opens modals. Memory stays bounded by the history budget (old branches
//! are fossilized and rebuilt by deterministic replay).

use crate::app::*;
use crate::driver::*;
use crate::history::*;
use gibson::{ColorDepth, SubcellGlyphMode};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct SustainedConfig {
    pub frames: u64,
    pub seed: u64,
    pub sizes: Vec<(u16, u16)>,
    /// Render real HumanMusic audio for every Nth new branch (0 = never).
    pub audio_every: u32,
    pub max_resident_recs: usize,
    pub depth: ColorDepth,
    pub max_branches: usize,
}

impl Default for SustainedConfig {
    fn default() -> Self {
        SustainedConfig {
            frames: 12_000,
            seed: 0xC40_0001,
            sizes: vec![(42, 15), (60, 20), (80, 24), (120, 40), (160, 50)],
            audio_every: 0,
            max_resident_recs: 30_000,
            depth: ColorDepth::TrueColor,
            max_branches: 4096,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Percentiles {
    pub p50: f64,
    pub p90: f64,
    pub p99: f64,
    pub max: f64,
    pub mean: f64,
}

pub fn pct(v: &mut [f64]) -> Percentiles {
    if v.is_empty() {
        return Percentiles::default();
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| v[((v.len() as f64 - 1.0) * q).round() as usize];
    Percentiles {
        p50: at(0.5),
        p90: at(0.9),
        p99: at(0.99),
        max: *v.last().unwrap(),
        mean: v.iter().sum::<f64>() / v.len() as f64,
    }
}

#[derive(Clone, Debug, Default)]
pub struct RssSample {
    pub frame: u64,
    pub rss_kb: u64,
    pub hwm_kb: u64,
}

pub fn rss() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| -> u64 {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|x| x.parse().ok())
            .unwrap_or(0)
    };
    (get("VmRSS:"), get("VmHWM:"))
}

#[derive(Clone, Debug, Default)]
pub struct SustainedReport {
    pub frames: u64,
    pub wall_secs: f64,
    pub frame_total_us: Percentiles,
    pub build_us: Percentiles,
    pub gen_us: Percentiles,
    pub write_us: Percentiles,
    pub bytes_per_frame: Percentiles,
    pub total_bytes: u64,
    pub exact_changed: Percentiles,
    pub full_repaints: u64,
    pub segments_per_frame: Percentiles,
    pub branches: usize,
    pub forks_made: u64,
    pub fork_failures: u64,
    pub max_resident_recs_seen: usize,
    pub resident_recs_end: usize,
    pub hist_bytes_end: usize,
    pub fossilized_total: u64,
    pub rehydrated_total: u64,
    pub vm_steps: u64,
    pub replay_steps: u64,
    pub story_replays: u64,
    pub story_replayed_updates: u64,
    pub story_checkpoints: usize,
    pub rss: Vec<RssSample>,
    pub actions: std::collections::BTreeMap<&'static str, u64>,
    pub resizes: u64,
    pub audio_builds: u64,
    pub audio_resident: usize,
    pub audio_bytes: usize,
    pub audio_render_ms: f64,
    pub audio_secs: f64,
    pub digest: u64,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n.max(1)
    }
}

pub fn run_sustained(cfg: &SustainedConfig) -> SustainedReport {
    let (w0, h0) = cfg.sizes[cfg.sizes.len() / 2];
    let opts = Options {
        audio: if cfg.audio_every > 0 {
            AudioMode::Silent
        } else {
            AudioMode::Off
        },
        threaded_audio: false,
        ..Options::default()
    };
    let mut rig = Rig::headless(w0, h0, cfg.depth, SubcellGlyphMode::Braille2x4, opts);
    rig.model.hist.retention.max_resident_recs = cfg.max_resident_recs;
    rig.model.hist.retention.max_branches = cfg.max_branches;
    // audio is rendered lazily and only when asked for: the sustained workload is about the
    // history/render path; real audio is sampled via `audio_every`.
    let mut rng = Rng(cfg.seed | 1);
    let mut rep = SustainedReport::default();
    let mut frame_us = vec![];
    let mut build_us = vec![];
    let mut gen_us = vec![];
    let mut write_us = vec![];
    let mut bytes = vec![];
    let mut changed = vec![];
    let mut segs = vec![];
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let t0 = Instant::now();
    let mut forks_since_audio = 0u32;
    let (r0, h0k) = rss();
    rep.rss.push(RssSample {
        frame: 0,
        rss_kb: r0,
        hwm_kb: h0k,
    });
    let count =
        |rep: &mut SustainedReport, k: &'static str| *rep.actions.entry(k).or_insert(0) += 1;

    for f in 0..cfg.frames {
        // an action every few frames: the autopilot behaves like a (very) restless user
        if f % 3 == 0 {
            let m = &mut rig.model;
            let end = m.end();
            match rng.below(100) {
                0..=17 => {
                    let d = rng.below(41) as i32 - 20;
                    m.do_cmd(Cmd::Step(d));
                    count(&mut rep, "step");
                }
                18..=25 => {
                    m.do_cmd(Cmd::GoTo(rng.below(end as u64 + 1) as u32));
                    count(&mut rep, "goto");
                }
                26..=37 => {
                    let j = [
                        Jump::NextLandmark,
                        Jump::PrevLandmark,
                        Jump::NextDecision,
                        Jump::PrevDecision,
                        Jump::NextEpoch,
                        Jump::PrevEpoch,
                        Jump::NextInput,
                        Jump::PrevInput,
                    ][rng.below(8) as usize];
                    m.do_cmd(Cmd::Jump(j));
                    count(&mut rep, "jump");
                }
                38..=43 => {
                    m.do_cmd(Cmd::TogglePlay);
                    count(&mut rep, "play");
                }
                44..=47 => {
                    m.do_cmd(Cmd::Speed(if rng.below(2) == 0 { 1 } else { -1 }));
                    count(&mut rep, "speed");
                }
                48..=61 => {
                    // fork at the cursor with a random available edit
                    let opts = m.fork_options();
                    if opts.is_empty() {
                        m.do_cmd(Cmd::GoTo(rng.below(end as u64 / 2 + 1) as u32));
                    } else {
                        let (_, e) = opts[rng.below(opts.len() as u64) as usize].clone();
                        match m.fork_with(e) {
                            Ok(_) => {
                                rep.forks_made += 1;
                                forks_since_audio += 1;
                                count(&mut rep, "fork");
                            }
                            Err(_) => rep.fork_failures += 1,
                        }
                    }
                }
                62..=71 => {
                    let n = m.hist.branches.len() as u64;
                    let b = rng.below(n) as u16;
                    m.do_cmd(Cmd::SwitchBranch(b));
                    count(&mut rep, "switch");
                }
                72..=77 => {
                    m.do_cmd(Cmd::ToggleCompare);
                    count(&mut rep, "compare");
                }
                78..=79 => {
                    m.do_cmd(Cmd::CycleCompare);
                    count(&mut rep, "compare-next");
                }
                80..=83 => {
                    m.do_cmd(Cmd::ToggleCollapse);
                    count(&mut rep, "collapse");
                }
                84..=86 => {
                    m.do_cmd(Cmd::Turn);
                    count(&mut rep, "turn");
                }
                87..=90 => {
                    m.do_cmd(Cmd::Inspect);
                    count(&mut rep, "inspect");
                }
                91..=92 => {
                    m.do_cmd(Cmd::OpenFork);
                    count(&mut rep, "open-fork");
                }
                93..=94 => {
                    m.do_cmd(Cmd::CloseModal);
                    count(&mut rep, "close");
                }
                95..=96 => {
                    m.do_cmd(Cmd::Jump(Jump::End));
                    count(&mut rep, "end");
                }
                _ => {
                    // resize across the matrix, mid-scrub
                    let (w, h) = cfg.sizes[rng.below(cfg.sizes.len() as u64) as usize];
                    rig.resize(w, h);
                    rep.resizes += 1;
                    count(&mut rep, "resize");
                }
            }
        }
        if cfg.audio_every > 0 && forks_since_audio >= cfg.audio_every {
            forks_since_audio = 0;
            let b = rig.model.cur_b;
            let Model { hist, audio, .. } = &mut rig.model;
            audio.request(hist, b);
            audio.enforce_budget(&[b]);
        }
        let t = Instant::now();
        let info = rig.frame().expect("headless frame");
        frame_us.push(t.elapsed().as_micros() as f64);
        build_us.push(info.build_us as f64);
        gen_us.push(info.gen_us as f64);
        write_us.push(info.write_us as f64);
        bytes.push(info.bytes as f64);
        changed.push(info.exact_changed as f64);
        segs.push(info.segments as f64);
        if info.full_repaint {
            rep.full_repaints += 1;
        }
        // fold the visible state into a determinism digest
        digest ^= rig.model.cursor as u64
            ^ ((rig.model.cur_b as u64) << 24)
            ^ (info.bytes as u64) << 8
            ^ (info.exact_changed as u64) << 40;
        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
        rep.max_resident_recs_seen = rep
            .max_resident_recs_seen
            .max(rig.model.hist.resident_recs());
        if f % 1000 == 0 || f + 1 == cfg.frames {
            let (r, hk) = rss();
            rep.rss.push(RssSample {
                frame: f + 1,
                rss_kb: r,
                hwm_kb: hk,
            });
        }
    }
    rep.frames = cfg.frames;
    rep.wall_secs = t0.elapsed().as_secs_f64();
    rep.frame_total_us = pct(&mut frame_us);
    rep.build_us = pct(&mut build_us);
    rep.gen_us = pct(&mut gen_us);
    rep.write_us = pct(&mut write_us);
    rep.bytes_per_frame = pct(&mut bytes);
    rep.total_bytes = rig.total_bytes;
    rep.exact_changed = pct(&mut changed);
    rep.segments_per_frame = pct(&mut segs);
    let m = &rig.model;
    rep.branches = m.hist.branches.len();
    rep.resident_recs_end = m.hist.resident_recs();
    rep.hist_bytes_end = m.hist.approx_bytes();
    rep.fossilized_total = m.hist.fossilized_total;
    rep.rehydrated_total = m.hist.rehydrated_total;
    rep.vm_steps = m.hist.vm_steps;
    rep.replay_steps = m.hist.replay_steps;
    rep.story_replays = m.atmo.stats.replays;
    rep.story_replayed_updates = m.atmo.stats.replayed_updates;
    rep.story_checkpoints = m.atmo.checkpoint_count();
    rep.audio_builds = m.audio.stats.builds + m.audio.stats.rebuilds;
    rep.audio_resident = m.audio.resident_count();
    rep.audio_bytes = m.audio.resident_bytes();
    rep.audio_render_ms = m.audio.stats.total_render_ms;
    rep.audio_secs = m.audio.stats.total_audio_secs;
    rep.digest = digest;
    rep
}

impl SustainedReport {
    pub fn to_markdown(&self) -> String {
        let p = |x: &Percentiles| {
            format!(
                "p50 {:.0} · p90 {:.0} · p99 {:.0} · max {:.0} · mean {:.0}",
                x.p50, x.p90, x.p99, x.max, x.mean
            )
        };
        let mut s = String::new();
        s += &format!(
            "frames: {}  wall: {:.1}s  ({:.0} frames/s)\n",
            self.frames,
            self.wall_secs,
            self.frames as f64 / self.wall_secs.max(1e-9)
        );
        s += &format!("frame total µs: {}\n", p(&self.frame_total_us));
        s += &format!("view build µs: {}\n", p(&self.build_us));
        s += &format!("renderer generation µs: {}\n", p(&self.gen_us));
        s += &format!("renderer write µs: {}\n", p(&self.write_us));
        s += &format!(
            "bytes/frame: {}   total {} B ({:.1} B/frame mean)\n",
            p(&self.bytes_per_frame),
            self.total_bytes,
            self.total_bytes as f64 / self.frames.max(1) as f64
        );
        s += &format!("exact changed cells/frame: {}\n", p(&self.exact_changed));
        s += &format!(
            "full repaints: {}   wire segments/frame: {}\n",
            self.full_repaints,
            p(&self.segments_per_frame)
        );
        s += &format!(
            "branches: {} ({} forks made, {} refused)   resizes: {}\n",
            self.branches, self.forks_made, self.fork_failures, self.resizes
        );
        s += &format!(
            "history: peak resident recs {}  end {}  ≈{} KiB   fossilized {}  rehydrated {}\n",
            self.max_resident_recs_seen,
            self.resident_recs_end,
            self.hist_bytes_end / 1024,
            self.fossilized_total,
            self.rehydrated_total
        );
        s += &format!("vm steps executed {}   replay steps {}   story replays {} ({} updates, {} checkpoints retained)\n", self.vm_steps, self.replay_steps, self.story_replays, self.story_replayed_updates, self.story_checkpoints);
        s += &format!(
            "audio: {} performances built ({:.1}s audio, {:.0} ms render)  resident {} ≈{} KiB\n",
            self.audio_builds,
            self.audio_secs,
            self.audio_render_ms,
            self.audio_resident,
            self.audio_bytes / 1024
        );
        s += "RSS (kB) by frame: ";
        for r in &self.rss {
            s += &format!("{}:{}/{} ", r.frame, r.rss_kb, r.hwm_kb);
        }
        s += &format!(
            "\nactions: {:?}\ndeterminism digest: {:016x}\n",
            self.actions, self.digest
        );
        s
    }
}

#[allow(dead_code)]
fn _kinds(_: Edit) {}

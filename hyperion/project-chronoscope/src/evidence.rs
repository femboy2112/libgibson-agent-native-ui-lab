//! Reproducible evidence generators (`--evidence=audio|matrix`). They print markdown that is
//! pasted into docs/evidence/ and EXPERIMENT_REPORT.md; nothing here is asserted, only measured.

use crate::app::*;
use crate::audio::*;
use crate::driver::*;
use crate::fixture::*;
use crate::history::*;
use crate::ui::layout;
use gibson::audio::human_music::WorldId;
use gibson::{ColorDepth, SubcellGlyphMode};
use std::time::Instant;

fn rms(p: &[i16]) -> f64 {
    (p.iter().map(|x| (*x as f64 / 32768.0).powi(2)).sum::<f64>() / p.len().max(1) as f64).sqrt()
}

pub fn audio_evidence() -> String {
    let mut out = String::new();
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).expect("fork");
    h.run_to_end(f);
    out += "## HumanMusic as a time-travel instrument — measurements\n\n";
    out += &format!(
        "Fixture: seed {DEMO_SEED}; root = {} steps to {:?}; fork B = {} steps ({:?}). 1 step = 1/{:.0} beat; world tempo 88 BPM; 48 kHz stereo.\n\n",
        h.branch(0).end(),
        h.branch(0).terminal,
        h.branch(f).end(),
        h.branch(f).terminal,
        STEPS_PER_BEAT
    );
    out += "| world | strategy | branch | audio s | compose ms | render ms | × realtime | peak | RMS | voices | notes | PCM MiB | hash |\n|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n";
    for world in [WorldId::BlackIce, WorldId::Vapor95, WorldId::SwissSignal] {
        for strat in [Strategy::FutureOnly, Strategy::FullTrace] {
            if world != WorldId::BlackIce && strat == Strategy::FullTrace {
                continue;
            }
            let mut s = AudioStore::new(world, strat, false);
            for b in [0, f] {
                s.request(&mut h, b);
                let p = s.get(b).unwrap();
                out += &format!(
                    "| {:?} | {:?} | {} | {:.1} | {:.0} | {:.0} | {:.0} | {:.2} | {:.3} | {} | {} | {:.1} | `{:016x}` |\n",
                    world,
                    strat,
                    h.branch(b).label,
                    p.stats.audio_secs,
                    p.stats.compose_ms,
                    p.stats.render_ms,
                    p.stats.rtf,
                    p.stats.peak,
                    p.stats.rms,
                    p.stats.max_voices,
                    p.stats.notes,
                    p.pcm.len() as f64 * 2.0 / 1048576.0,
                    p.hash
                );
            }
        }
    }
    // determinism + immutable past + seam
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, 0);
    s.request(&mut h, f);
    let first = s.get(f).unwrap().hash;
    s.evict(f);
    s.request(&mut h, f);
    out += &format!(
        "\n**Rebuild after eviction**: first hash `{first:016x}`, rebuilt `{:016x}` → {}\n",
        s.get(f).unwrap().hash,
        if first == s.get(f).unwrap().hash {
            "bit-identical"
        } else {
            "DIFFERENT"
        }
    );
    let fork_s = s.clock.sample_of(50) as usize;
    let pa = s.fetch(&h, 0, 0, fork_s).unwrap();
    let pb = s.fetch(&h, f, 0, fork_s).unwrap();
    out += &format!(
        "**Immutable past**: {} samples before the fork sample: parent == child → {}\n",
        fork_s,
        pa == pb
    );
    // after-fork divergence: how long until the child no longer equals the parent?
    let n = 48_000 * 6;
    let ca = s.fetch(&h, 0, fork_s as u64, n).unwrap();
    let cb = s.fetch(&h, f, fork_s as u64, n).unwrap();
    let first_diff = ca
        .iter()
        .zip(cb.iter())
        .position(|(a, b)| a != b)
        .map(|i| i / 2);
    out += &format!(
        "**Divergence after the fork**: first differing frame at +{:?} frames ({:?} ms)\n",
        first_diff,
        first_diff.map(|d| d as f64 / 48.0)
    );
    let win = 2400;
    let seam = s
        .fetch(&h, f, (fork_s - win) as u64, 2 * win + 24_000)
        .unwrap();
    let max_step = seam
        .chunks(2)
        .zip(seam.chunks(2).skip(1))
        .map(|(a, b)| (a[0] as i32 - b[0] as i32).abs())
        .max()
        .unwrap();
    let base_step = {
        let q = s.fetch(&h, 0, (fork_s - 40_000) as u64, 24_000).unwrap();
        q.chunks(2)
            .zip(q.chunks(2).skip(1))
            .map(|(a, b)| (a[0] as i32 - b[0] as i32).abs())
            .max()
            .unwrap()
    };
    out += &format!(
        "**Seam** (crossfade {:.2} s after the fork): max sample step across the seam window {} / 32767 (a pure-parent window of equal length: {}); RMS before {:.4}, after {:.4}\n",
        XFADE_SECS,
        max_step,
        base_step,
        rms(&seam[..2 * win]),
        rms(&seam[2 * win + 24_000..])
    );
    // harmonic continuity at the fork, from the public Score chords
    {
        use gibson::audio::human_music::{compose, MusicWorld};
        let w = MusicWorld::from_id(WorldId::BlackIce);
        let seed_a = seed_for(&h, 0);
        let seed_b = seed_for(&h, f);
        let a = compose(&trace_of(&h, 0, 0, h.branch(0).end()), &w, seed_a);
        let fo = compose(&trace_of(&h, f, 50, h.branch(f).end()), &w, seed_b);
        let fu = compose(&trace_of(&h, f, 0, h.branch(f).end()), &w, seed_b);
        let at = |sc: &gibson::audio::human_music::score::Score, beat: f64| {
            sc.chords
                .iter()
                .find(|c| c.start_beat <= beat && beat < c.start_beat + c.dur_beats as f64)
                .map(|c| format!("{}{:?}", c.chord.root_pc, c.chord.quality))
        };
        let fork_beat = 50.0 / STEPS_PER_BEAT;
        out += &format!(
            "\n**Harmony at the fork** (beat {fork_beat}): parent chord {:?}; child FutureOnly beat 0 {:?}; child FullTrace at the fork beat {:?}.\n",
            at(&a, fork_beat),
            at(&fo, 0.0),
            at(&fu, fork_beat)
        );
        let n = (fork_beat as usize).max(1);
        let same = (0..n)
            .filter(|b| at(&a, *b as f64 + 0.5) == at(&fu, *b as f64 + 0.5))
            .count();
        out += &format!(
            "**Prefix harmony stability** (full-trace child vs parent, per beat over the shared {n} beats): {same}/{n} beats carry the same chord; tempo {} vs {} BPM; total beats {} vs {}.\n",
            a.tempo_bpm, fu.tempo_bpm, a.total_beats, fu.total_beats
        );
    }
    // is chord agreement at the fork systematic or luck? sample many fork points
    {
        use gibson::audio::human_music::{compose, MusicWorld};
        let w = MusicWorld::from_id(WorldId::BlackIce);
        let mut h2 = History::new(colony(), DEMO_SEED, demo_script());
        h2.run_to_end(0);
        let sa = compose(
            &trace_of(&h2, 0, 0, h2.branch(0).end()),
            &w,
            seed_for(&h2, 0),
        );
        let at = |sc: &gibson::audio::human_music::score::Score, beat: f64| {
            sc.chords
                .iter()
                .find(|c| c.start_beat <= beat && beat < c.start_beat + c.dur_beats as f64)
                .map(|c| c.chord)
        };
        let (mut n, mut same_full, mut same_future, mut pc_full, mut pc_future) =
            (0, 0, 0, 0.0f64, 0.0f64);
        let jacc = |a: gibson::audio::human_music::theory::Chord,
                    b: gibson::audio::human_music::theory::Chord| {
            let (pa, pb) = (a.pitch_classes(), b.pitch_classes());
            let inter = pa.iter().filter(|x| pb.contains(x)).count() as f64;
            inter / (pa.len() + pb.len()) as f64 * 2.0
        };
        for at_step in (40u32..=300).step_by(10) {
            let Ok(c) = h2.fork(0, at_step, Edit::InsertCmd(2)) else {
                continue;
            };
            h2.run_to_end(c);
            let beat = at_step as f64 / STEPS_PER_BEAT;
            let parent = at(&sa, beat);
            let fu = compose(
                &trace_of(&h2, c, 0, h2.branch(c).end()),
                &w,
                seed_for(&h2, c),
            );
            let fo = compose(
                &trace_of(&h2, c, at_step, h2.branch(c).end()),
                &w,
                seed_for(&h2, c),
            );
            let (full_c, fut_c) = (at(&fu, beat), at(&fo, 0.0));
            if let (Some(p), Some(a), Some(b)) = (parent, full_c, fut_c) {
                n += 1;
                same_full += (p == a) as u32;
                same_future += (p == b) as u32;
                pc_full += jacc(p, a);
                pc_future += jacc(p, b);
            }
        }
        out += &format!(
            "\n**Chord agreement at the fork over {n} fork points** (steps 40..300): FullTrace child == parent chord at {same_full}/{n}, mean pitch-class overlap {:.2}; FutureOnly child's first chord == parent chord at {same_future}/{n}, overlap {:.2}.\n",
            pc_full / n as f64,
            pc_future / n as f64
        );
    }
    // seek cost: PCM offset vs deterministic re-render of the prefix
    let t0 = Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(s.fetch(&h, 0, 48_000 * 40, 4096));
    }
    let fetch_us = t0.elapsed().as_secs_f64() * 1e6 / 1000.0;
    let world = gibson::audio::human_music::MusicWorld::from_id(WorldId::BlackIce);
    let spec = spec_for(&h, 0, WorldId::BlackIce, Strategy::FutureOnly);
    let score = gibson::audio::human_music::compose(&spec.trace, &world, spec.seed);
    let mut rec = Vec::new();
    for secs in [10.0f64, 20.0, 40.0, 80.0] {
        let t = Instant::now();
        let mut syn = gibson::audio::human_music::HumanMusicSynth::new(&score, &world, SR);
        let mut sink = gibson::audio::StereoBlock::new(512);
        let target = (secs * 48_000.0) as u64;
        let mut pos = 0u64;
        while pos < target.min(syn.total_samples()) {
            sink.clear();
            gibson::audio::render::AudioSource::render(
                &mut syn,
                &mut sink,
                &gibson::audio::render::RenderCtx {
                    sr: SR,
                    start: gibson::audio::SampleTime(pos),
                },
            );
            pos += 512;
        }
        rec.push((secs, t.elapsed().as_secs_f64() * 1e3));
    }
    out += &format!("\n**Seek**: PCM offset fetch of 4096 frames = {fetch_us:.1} µs. Honest reconstruction (fresh synth, render-and-discard the prefix): ");
    for (secs, ms) in rec {
        out += &format!("to {secs:.0} s = {ms:.0} ms; ");
    }
    out += "\n";
    out += &format!(
        "\nAudio store after these builds: {} resident, {:.1} MiB, builds {}, rebuilds {}, evictions {}, rebuild hash mismatches {}.\n",
        s.resident_count(),
        s.resident_bytes() as f64 / 1048576.0,
        s.stats.builds,
        s.stats.rebuilds,
        s.stats.evictions,
        s.stats.hash_mismatch_on_rebuild
    );
    out
}

pub fn matrix_evidence() -> String {
    let sizes = [(42u16, 15u16), (60, 20), (80, 24), (120, 40), (160, 50)];
    let depths = [
        ColorDepth::TrueColor,
        ColorDepth::Ansi256,
        ColorDepth::Ansi16,
        ColorDepth::Mono,
    ];
    let scenes: [(&str, &str); 6] = [
        ("start", "settle"),
        ("catastrophe", "end; settle"),
        (
            "fork+ghost",
            "end; goto 50; fork replace2; branch 0; goto 120; settle",
        ),
        (
            "compare",
            "end; goto 50; fork replace2; goto 215; compare; settle",
        ),
        ("looking-back", "end; goto 300; step -8; settle"),
        ("fork-modal", "end; goto 50; openfork; settle"),
    ];
    let mut out = String::from("## Render matrix (every number is for ONE full paint of the settled scene on a fresh headless Context; µs are the renderer's own generation/write accounting, one sample per cell, not a benchmark)\n\n");
    out += "| size | layout (view+strip+side) | depth | scene | full-paint bytes | changed cells / total | gen µs | write µs | wire segments | ink glyphs |\n|---|---|---|---|---:|---:|---:|---:|---:|---:|\n";
    for &(w, h) in &sizes {
        let lay = layout(w, h);
        for &d in &depths {
            for (name, script) in scenes {
                let mut r = Rig::headless(
                    w,
                    h,
                    d,
                    SubcellGlyphMode::Braille2x4,
                    Options {
                        audio: AudioMode::Off,
                        ..Options::default()
                    },
                );
                run_script(&mut r, script).unwrap();
                // a fresh full paint of the settled scene
                r.settle().unwrap();
                let (stream, info) = r.snapshot_full().unwrap();
                let bytes = stream.len();
                let ink = r
                    .lines()
                    .iter()
                    .flat_map(|l| l.chars())
                    .filter(|c| ('\u{2800}'..='\u{28FF}').contains(c) || "▀▄█▓▒░".contains(*c))
                    .count();
                out += &format!(
                    "| {w}×{h} | {}×{}+{}+{} | {} | {name} | {bytes} | {}/{} | {} | {} | {} | {ink} |\n",
                    lay.view_w,
                    lay.view_h,
                    lay.strip_h,
                    lay.side_w,
                    depth_name(d),
                    info.exact_changed,
                    info.total_cells,
                    info.gen_us,
                    info.write_us,
                    info.segments
                );
            }
        }
    }
    out
}

/// How much of every divergence the single intervention explains (data-causal cone).
pub fn causal_evidence() -> String {
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    let mut out = String::from("## Causal attribution of divergence (one intervention per fork)\n\n| fork | aligned steps | diverged events | intervention roots | reorder roots | first state divergence | first event divergence | outcome |\n|---|---:|---:|---:|---:|---:|---:|---|\n");
    let mut edits: Vec<(u32, Edit)> = vec![
        (50, Edit::ReplaceCmd(2)),
        (50, Edit::DropCmd),
        (62, Edit::DropCmd),
        (74, Edit::DropCmd),
        (30, Edit::InsertCmd(2)),
        (100, Edit::InsertCmd(2)),
        (200, Edit::InsertCmd(5)),
        (300, Edit::InsertCmd(2)),
    ];
    // every rare fate outcome in the root run, flipped
    let fates: Vec<u32> = (0..h.branch(0).end())
        .filter(|&s| {
            h.rec_at(0, s)
                .map(|r| r.ev.is_fate() && r.ev.after == 0)
                .unwrap_or(false)
        })
        .collect();
    for s in fates {
        edits.push((s, Edit::Override(1)));
    }
    for (at, e) in edits {
        let Ok(f) = h.fork(0, at, e.clone()) else {
            continue;
        };
        h.run_to_end(f);
        let Some(c) = compare(&h, 0, f) else { continue };
        let dv = c.rows.iter().filter(|r| r.diverged_event).count();
        let ints = c
            .rows
            .iter()
            .filter(|r| r.root == Some(RootKind::Intervention))
            .count();
        let reo = c
            .rows
            .iter()
            .filter(|r| r.root == Some(RootKind::Reorder))
            .count();
        let first_ev = c.rows.iter().position(|r| r.diverged_event);
        out += &format!(
            "| {}@{at} {} | {} | {dv} | {ints} | {reo} | {:?} | {:?} | {:?}@{} |\n",
            h.branch(f).label,
            e.describe(),
            c.rows.len(),
            c.first_divergence,
            first_ev,
            h.branch(f).terminal,
            h.branch(f).end()
        );
    }
    out
}

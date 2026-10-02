//! Audio lifecycle under time travel: determinism, immutable past, seam, rebuild-after-evict.

use gibson::audio::human_music::semantic::*;
use gibson::audio::human_music::synth::HumanMusicSynth;
use gibson::audio::human_music::{compose, MusicWorld, WorldId};
use gibson::audio::render::{AudioSource, OfflineRenderer, RenderCtx};
use gibson::audio::{SampleRate, SampleTime, StereoBlock};
use project_chronoscope::audio::*;
use project_chronoscope::fixture::*;
use project_chronoscope::history::*;

fn demo_with_fork() -> (History, BranchId) {
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    let f = h.fork(0, 50, Edit::ReplaceCmd(2)).unwrap();
    h.run_to_end(f);
    (h, f)
}

#[test]
fn performance_is_a_pure_function_of_branch_lineage() {
    let (mut h, f) = demo_with_fork();
    let mut a = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    let mut b = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    a.request(&mut h, f);
    b.request(&mut h, f);
    let (pa, pb) = (a.get(f).unwrap().clone(), b.get(f).unwrap().clone());
    assert_eq!(pa.hash, pb.hash);
    assert_eq!(pa.pcm, pb.pcm);
    assert!(!pa.stats.nonfinite);
    assert!(pa.stats.error.is_none());
    assert!(pa.stats.peak > 0.05, "something audible was rendered");
    assert_eq!(pa.env.len() as u32, pa.to_step - pa.from_step);
    assert!(pa.env.iter().all(|e| e.is_finite()));
}

#[test]
fn distinct_futures_get_distinct_music() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, 0);
    s.request(&mut h, f);
    assert_ne!(s.get(0).unwrap().hash, s.get(f).unwrap().hash);
    assert_ne!(seed_for(&h, 0), seed_for(&h, f));
}

#[test]
fn the_past_is_immutable_scrollback_across_a_fork() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, 0);
    s.request(&mut h, f);
    let fork_sample = s.clock.sample_of(50);
    let parent = s.fetch(&h, 0, 0, fork_sample as usize).unwrap();
    let child = s.fetch(&h, f, 0, fork_sample as usize).unwrap();
    assert_eq!(
        parent, child,
        "every sample before the fork is the parent's, bit for bit"
    );
    // and after the crossfade the child sounds like the child, not like the parent
    let after = fork_sample + (XFADE_SECS * 48_000.0) as u64 + 4800;
    let pa = s.fetch(&h, 0, after, 4800).unwrap();
    let ca = s.fetch(&h, f, after, 4800).unwrap();
    assert_ne!(pa, ca);
    let raw = &s.get(f).unwrap().pcm;
    let rel = (after - fork_sample) as usize;
    assert_eq!(
        &ca[..],
        &raw[2 * rel..2 * rel + 2 * 4800],
        "post-crossfade audio is the raw child performance"
    );
}

#[test]
fn seam_metrics_are_finite_and_recorded() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, 0);
    s.request(&mut h, f);
    let fork_sample = s.clock.sample_of(50) as usize;
    let win = 2400;
    let seam = s
        .fetch(&h, f, (fork_sample - win) as u64, 2 * win + 24_000)
        .unwrap();
    let max_step = seam
        .chunks(2)
        .zip(seam.chunks(2).skip(1))
        .map(|(a, b)| (a[0] as i32 - b[0] as i32).abs())
        .max()
        .unwrap();
    let rms_before = rms(&seam[..2 * win]);
    let rms_after = rms(&seam[2 * win + 2 * 12_000..]);
    println!("SEAM max |dL| = {max_step} / 32767, rms before {rms_before:.4} after {rms_after:.4}");
    assert!(max_step < 30_000, "no full-scale click at the seam");
}

fn rms(p: &[i16]) -> f64 {
    (p.iter().map(|x| (*x as f64 / 32768.0).powi(2)).sum::<f64>() / p.len().max(1) as f64).sqrt()
}

#[test]
fn evicted_performance_rebuilds_bit_identically() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, f);
    let first = s.get(f).unwrap().hash;
    s.evict(f);
    assert!(s.get(f).is_none());
    s.request(&mut h, f);
    assert_eq!(s.get(f).unwrap().hash, first);
    assert_eq!(s.stats.rebuilds, 1);
    assert_eq!(s.stats.hash_mismatch_on_rebuild, 0);
    assert_eq!(s.stats.rebuild_hashes_checked, 1);
}

#[test]
fn byte_budget_evicts_lru_but_not_protected() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, 0);
    s.request(&mut h, f);
    s.budget_bytes = 1 << 20;
    s.enforce_budget(&h, &[f]);
    // f's audible past is its parent's PCM: the lineage is protected, nothing may be evicted
    assert!(s.get(f).is_some() && s.get(0).is_some());
    assert_eq!(s.stats.evictions, 0);
    // an unrelated branch is fair game
    let other = h.fork(0, 62, Edit::DropCmd).unwrap();
    h.run_to_end(other);
    s.request(&mut h, other);
    s.budget_bytes = 1 << 20;
    s.enforce_budget(&h, &[f]);
    assert!(s.get(f).is_some() && s.get(0).is_some());
    assert!(
        s.get(other).is_none(),
        "the least-recently-used unprotected branch was evicted"
    );
    assert_eq!(s.stats.evictions, 1);
}

#[test]
fn threaded_store_matches_inline_store() {
    let (mut h, f) = demo_with_fork();
    let mut inline = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    inline.request(&mut h, f);
    let mut bg = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, true);
    bg.request(&mut h, f);
    assert!(bg.is_pending(f) || bg.get(f).is_some());
    bg.wait(&mut h, f);
    assert_eq!(bg.get(f).unwrap().hash, inline.get(f).unwrap().hash);
}

#[test]
fn full_trace_strategy_is_a_different_but_valid_performance() {
    let (mut h, f) = demo_with_fork();
    let mut a = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    let mut b = AudioStore::new(WorldId::BlackIce, Strategy::FullTrace, false);
    a.request(&mut h, f);
    b.request(&mut h, f);
    let (pa, pb) = (a.get(f).unwrap(), b.get(f).unwrap());
    assert_ne!(pa.hash, pb.hash);
    assert!(pb.stats.error.is_none());
    assert!(pb.frames() > 0);
    println!(
        "future-only {} frames vs full-trace {} frames; render {:.0} ms vs {:.0} ms",
        pa.frames(),
        pb.frames(),
        pa.stats.render_ms,
        pb.stats.render_ms
    );
}

#[test]
fn every_world_composes_every_epoch_without_panic() {
    // the semantic mapping must be total: one trace touching every epoch under every world
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    for w in [WorldId::BlackIce, WorldId::Vapor95, WorldId::SwissSignal] {
        let spec = spec_for(&h, 0, w, Strategy::FutureOnly);
        let p = render_spec(&spec);
        assert!(p.stats.error.is_none(), "{w:?}");
        assert!(!p.stats.nonfinite, "{w:?}");
        assert!(p.frames() > 48_000, "{w:?}");
    }
}

#[test]
fn short_futures_are_held_to_a_phrase_not_rejected() {
    // a fork just before the terminal leaves < 16 beats of future; composition must cope
    let mut h = History::new(colony(), DEMO_SEED, demo_script());
    h.run_to_end(0);
    let late = h.fork(0, 330, Edit::InsertCmd(2)).unwrap();
    h.run_to_end(late);
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, late);
    let p = s.get(late).unwrap();
    assert!(p.stats.error.is_none());
    assert!(p.frames() > 0);
}

// --- consumer-evidence probes of the *public* audio API ------------------------------------

fn st(tone: Tone, e: Emphasis, d: Density, el: Elevation) -> SemanticState {
    SemanticState {
        tone,
        emphasis: e,
        density: d,
        elevation: el,
    }
}
fn ev(b: f64, s: SemanticState, k: EventKind) -> SemanticEvent {
    SemanticEvent {
        at_beat: b,
        state: s,
        kind: k,
    }
}

#[test]
fn probe_synth_cannot_seek_and_jumped_ctx_start_is_not_a_seek() {
    let world = MusicWorld::from_id(WorldId::BlackIce);
    let sr = SampleRate::STUDIO;
    let trace = SemanticTrace::new(
        vec![
            ev(
                0.0,
                st(
                    Tone::Neutral,
                    Emphasis::Muted,
                    Density::Spacious,
                    Elevation::Flat,
                ),
                EventKind::ActChanged,
            ),
            ev(
                16.0,
                st(
                    Tone::Info,
                    Emphasis::Normal,
                    Density::Normal,
                    Elevation::Raised,
                ),
                EventKind::FocusAcquired,
            ),
            ev(
                32.0,
                st(
                    Tone::Warning,
                    Emphasis::Strong,
                    Density::Compact,
                    Elevation::Raised,
                ),
                EventKind::ToneShift,
            ),
        ],
        48.0,
    );
    let score = compose(&trace, &world, 7);
    let mut truth_synth = HumanMusicSynth::new(&score, &world, sr);
    let n = truth_synth.total_samples();
    let truth = OfflineRenderer::new(sr, 512).render(&mut truth_synth, n);
    let spb = sr.as_f64() * 60.0 / world.tempo_bpm as f64;
    let start = (20.0 * spb) as u64;
    let want =
        |from: u64| -> Vec<f32> { truth.audio.left[from as usize..from as usize + 4096].to_vec() };

    // (1) jumping ctx.start on a fresh synth: NOT the true audio
    let mut fresh = HumanMusicSynth::new(&score, &world, sr);
    let mut blk = StereoBlock::new(4096);
    fresh.render(
        &mut blk,
        &RenderCtx {
            sr,
            start: SampleTime(start),
        },
    );
    assert_ne!(
        blk.left,
        want(start),
        "a jumped RenderCtx::start must not equal the real audio"
    );
    assert!(
        fresh.active_voices() > 8,
        "the skipped events fire at once: {} voices",
        fresh.active_voices()
    );

    // (2) rewind() resets scheduling only: the DSP tail survives, so a re-render is not identical
    let mut s2 = HumanMusicSynth::new(&score, &world, sr);
    let mut sink = StereoBlock::new(512);
    let mut pos = 0u64;
    while pos < start {
        sink.clear();
        s2.render(
            &mut sink,
            &RenderCtx {
                sr,
                start: SampleTime(pos),
            },
        );
        pos += 512;
    }
    s2.rewind();
    let mut again = StereoBlock::new(4096);
    s2.render(
        &mut again,
        &RenderCtx {
            sr,
            start: SampleTime(0),
        },
    );
    assert_ne!(
        again.left,
        truth.audio.left[..4096].to_vec(),
        "rewind() is a transport rewind, not a DSP reset"
    );

    // (3) the honest reconstruction: fresh synth + render-and-discard the prefix is exact
    let mut s3 = HumanMusicSynth::new(&score, &world, sr);
    let mut pos = 0u64;
    while pos + 4096 <= start {
        sink.clear();
        s3.render(
            &mut sink,
            &RenderCtx {
                sr,
                start: SampleTime(pos),
            },
        );
        pos += 512;
    }
    let mut out = StereoBlock::new(4096);
    s3.render(
        &mut out,
        &RenderCtx {
            sr,
            start: SampleTime(pos),
        },
    );
    assert_eq!(
        out.left,
        want(pos),
        "reconstruction by deterministic re-render is exact"
    );
}

#[test]
fn probe_compose_is_not_prefix_causal() {
    let world = MusicWorld::from_id(WorldId::BlackIce);
    let sr = SampleRate::STUDIO;
    let prefix = vec![
        ev(
            0.0,
            st(
                Tone::Neutral,
                Emphasis::Muted,
                Density::Spacious,
                Elevation::Flat,
            ),
            EventKind::ActChanged,
        ),
        ev(
            16.0,
            st(
                Tone::Info,
                Emphasis::Normal,
                Density::Normal,
                Elevation::Raised,
            ),
            EventKind::FocusAcquired,
        ),
        ev(
            32.0,
            st(
                Tone::Warning,
                Emphasis::Strong,
                Density::Compact,
                Elevation::Raised,
            ),
            EventKind::ToneShift,
        ),
    ];
    let mut a = prefix.clone();
    a.push(ev(
        48.0,
        st(
            Tone::Danger,
            Emphasis::Strong,
            Density::Compact,
            Elevation::Overlay,
        ),
        EventKind::Impact,
    ));
    let mut b = prefix;
    b.push(ev(
        48.0,
        st(
            Tone::Success,
            Emphasis::Normal,
            Density::Spacious,
            Elevation::Raised,
        ),
        EventKind::Confirmation,
    ));
    let render = |ev: Vec<SemanticEvent>| {
        let trace = SemanticTrace::new(ev, 80.0);
        let score = compose(&trace, &world, 7);
        let mut syn = HumanMusicSynth::new(&score, &world, sr);
        let n = syn.total_samples();
        OfflineRenderer::new(sr, 512).render(&mut syn, n).audio
    };
    let (ra, rb) = (render(a), render(b));
    let spb = sr.as_f64() * 60.0 / world.tempo_bpm as f64;
    // traces are identical for the first 48 beats; audio is not, even for the first 8 beats
    let n8 = (8.0 * spb) as usize;
    assert_ne!(
        ra.left[..n8],
        rb.left[..n8],
        "the future changes the past: compose() is non-causal"
    );
}

#[test]
fn a_child_is_unplayable_without_its_parent_and_requesting_the_child_rebuilds_the_lineage() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FutureOnly, false);
    s.request(&mut h, f);
    assert!(
        s.get(0).is_some(),
        "asking for a child asks for its ancestors"
    );
    let before = s.get(0).unwrap().hash;
    s.evict(0);
    assert!(
        s.fetch(&h, f, 0, 4800).is_none(),
        "the child's past is the parent's PCM: no parent, no audio (and no pretending)"
    );
    s.request(&mut h, f);
    assert!(s.fetch(&h, f, 0, 4800).is_some());
    assert_eq!(
        s.get(0).unwrap().hash,
        before,
        "the rebuilt parent is bit-identical"
    );
}

// The app's default strategy is FullTrace; the tests above pin FutureOnly. These pin the default.

#[test]
fn full_trace_is_a_pure_function_of_branch_lineage() {
    let (mut h, f) = demo_with_fork();
    let mut a = AudioStore::new(WorldId::BlackIce, Strategy::FullTrace, false);
    let mut b = AudioStore::new(WorldId::BlackIce, Strategy::FullTrace, false);
    a.request(&mut h, f);
    b.request(&mut h, f);
    let (pa, pb) = (a.get(f).unwrap().clone(), b.get(f).unwrap().clone());
    assert_eq!(pa.hash, pb.hash);
    assert_eq!(pa.pcm, pb.pcm);
    assert!(pa.stats.error.is_none() && !pa.stats.nonfinite);
}

#[test]
fn full_trace_keeps_the_past_immutable_and_the_seam_bounded() {
    let (mut h, f) = demo_with_fork();
    let mut s = AudioStore::new(WorldId::BlackIce, Strategy::FullTrace, false);
    s.request(&mut h, 0);
    s.request(&mut h, f);
    let fork_sample = s.clock.sample_of(50) as usize;
    let parent = s.fetch(&h, 0, 0, fork_sample).unwrap();
    let child = s.fetch(&h, f, 0, fork_sample).unwrap();
    assert_eq!(
        parent, child,
        "FullTrace: the child's past is the parent's, bit for bit"
    );
    // the future really is another performance
    let after = (fork_sample + (XFADE_SECS * 48_000.0) as usize + 4800) as u64;
    assert_ne!(
        s.fetch(&h, 0, after, 4800).unwrap(),
        s.fetch(&h, f, after, 4800).unwrap()
    );
    let win = 2400;
    let seam = s
        .fetch(&h, f, (fork_sample - win) as u64, 2 * win + 24_000)
        .unwrap();
    let max_step = seam
        .chunks(2)
        .zip(seam.chunks(2).skip(1))
        .map(|(a, b)| (a[0] as i32 - b[0] as i32).abs())
        .max()
        .unwrap();
    println!("FULLTRACE SEAM max |dL| = {max_step} / 32767");
    assert!(
        max_step < 30_000,
        "no full-scale click at the FullTrace seam"
    );
}

// Minimal reproduction: AudioSource::render takes an absolute `RenderCtx::start`, but
// HumanMusicSynth is a forward-only state machine: a non-contiguous start silently fires every
// skipped event at once and returns audio that is not the audio at that position.
use gibson::audio::human_music::semantic::*;
use gibson::audio::human_music::synth::HumanMusicSynth;
use gibson::audio::human_music::{compose, MusicWorld, WorldId};
use gibson::audio::render::{AudioSource, OfflineRenderer, RenderCtx};
use gibson::audio::{SampleRate, SampleTime, StereoBlock};

fn ev(b: f64, tone: Tone, kind: EventKind) -> SemanticEvent {
    SemanticEvent {
        at_beat: b,
        state: SemanticState { tone, emphasis: Emphasis::Normal, density: Density::Normal, elevation: Elevation::Raised },
        kind,
    }
}

fn main() {
    let world = MusicWorld::from_id(WorldId::BlackIce);
    let sr = SampleRate::STUDIO;
    let trace = SemanticTrace::new(
        vec![ev(0.0, Tone::Neutral, EventKind::ActChanged), ev(16.0, Tone::Info, EventKind::FocusAcquired), ev(32.0, Tone::Warning, EventKind::ToneShift)],
        48.0,
    );
    let score = compose(&trace, &world, 7);
    // ground truth: render everything sequentially
    let mut a = HumanMusicSynth::new(&score, &world, sr);
    let n = a.total_samples();
    let truth = OfflineRenderer::new(sr, 512).render(&mut a, n);
    let start = (20.0 * sr.as_f64() * 60.0 / world.tempo_bpm as f64) as u64; // beat 20
    let want = &truth.audio.left[start as usize..start as usize + 4096];

    // "seek": a fresh synth asked for the block at `start`
    let mut b = HumanMusicSynth::new(&score, &world, sr);
    let mut blk = StereoBlock::new(4096);
    b.render(&mut blk, &RenderCtx { sr, start: SampleTime(start) });
    println!("block at beat 20 equals the sequentially rendered audio: {}", blk.left == want);
    println!("active voices after the jump: {} (sequential render peaks at {})", b.active_voices(), truth.max_active_voices);
    // Expected by the trait doc ("Fill `out` with audio for [ctx.start, ctx.start + out.frames())"):
    // either that audio, or an explicit refusal. Observed: different audio, every event with
    // at <= start triggered in the same sample, no error.
}

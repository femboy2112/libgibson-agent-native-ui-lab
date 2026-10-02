//! Dataset generation: fresh HumanMusic songs with hidden truth, in a dev/holdout split.

use gibson::audio::human_music::form::{SectionKind, BEATS_PER_BAR};
use gibson::audio::human_music::functor::{perform, Composition};
use gibson::audio::human_music::performance::PerformanceOptions;
use gibson::audio::human_music::score::{DrumVoice, Role, Score};
use gibson::audio::human_music::semantic::{self, SemanticTrace};
use gibson::audio::human_music::song::SongMap;
use gibson::audio::human_music::synth::HumanMusicSynth;
use gibson::audio::human_music::theory::Quality;
use gibson::audio::human_music::world::{MusicWorld, WorldId};
use gibson::audio::render::OfflineRenderer;
use gibson::audio::time::SampleRate;
use serde::{Deserialize, Serialize};

use sai_core::truth::{TruthChord, TruthNote, TruthSection, TruthStroke, TruthTrack, TRUTH_SCHEMA};

/// The reviewed LibGibson merge this harness consumes through public APIs.
pub const ANCHOR_REV: &str = "c2f6483d92fe2b351e6cd50936a97d8cdf73cb79";
/// Canonical render sample rate for the experiment.
pub const SAMPLE_RATE: u32 = 48_000;
/// Offline render block size.
pub const HARNESS_BLOCK: usize = 512;

/// Which half of the dataset an item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Dev,
    Holdout,
}

impl Split {
    pub fn as_str(self) -> &'static str {
        match self {
            Split::Dev => "dev",
            Split::Holdout => "holdout",
        }
    }
}

/// The semantic-trace shapes used to seed fresh songs. Each is a public LibGibson constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    Demo,
    RiseUnresolved,
    FalseClimax,
    CalmLoop,
    DeflectedLift,
}

impl TraceKind {
    pub const ALL: [Self; 5] = [
        Self::Demo,
        Self::RiseUnresolved,
        Self::FalseClimax,
        Self::CalmLoop,
        Self::DeflectedLift,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Demo => "demo",
            Self::RiseUnresolved => "rise-unresolved",
            Self::FalseClimax => "false-climax",
            Self::CalmLoop => "calm-loop",
            Self::DeflectedLift => "deflected-lift",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "demo" => Self::Demo,
            "rise-unresolved" => Self::RiseUnresolved,
            "false-climax" => Self::FalseClimax,
            "calm-loop" => Self::CalmLoop,
            "deflected-lift" => Self::DeflectedLift,
            _ => return None,
        })
    }

    pub fn trace(self, total_beats: f64) -> SemanticTrace {
        match self {
            Self::Demo => semantic::demo_trace(total_beats),
            Self::RiseUnresolved => semantic::rise_unresolved(total_beats),
            Self::FalseClimax => semantic::false_climax(total_beats),
            Self::CalmLoop => semantic::calm_loop(total_beats),
            Self::DeflectedLift => semantic::deflected_lift_trace(total_beats),
        }
    }
}

/// One generated item's specification.
#[derive(Debug, Clone)]
pub struct ItemSpec {
    pub id: String,
    pub world: WorldId,
    pub seed: u64,
    pub kind: TraceKind,
    pub total_beats: f64,
    pub split: Split,
}

impl ItemSpec {
    pub fn canonical(
        world: WorldId,
        kind: TraceKind,
        seed: u64,
        total_beats: f64,
        split: Split,
    ) -> Self {
        let world_name = match world {
            WorldId::BlackIce => "black-ice",
            WorldId::Vapor95 => "vapor95",
            WorldId::SwissSignal => "swiss-signal",
        };
        let id = format!("{}-{}-{:06}-{}", world_name, kind.name(), seed, split.as_str());
        Self {
            id,
            world,
            seed,
            kind,
            total_beats,
            split,
        }
    }

    pub fn world(&self) -> MusicWorld {
        MusicWorld::from_id(self.world)
    }
}

/// A rendered score and its stereo audio.
pub struct Rendered {
    pub audio: gibson::audio::StereoBlock,
    pub sr: SampleRate,
    pub frames: usize,
}

impl Rendered {
    pub fn duration_seconds(&self) -> f64 {
        self.frames as f64 / self.sr.get() as f64
    }
}

/// Compose `spec` into a full [`Composition`] (song + performance + score).
pub fn build_composition(spec: &ItemSpec) -> Composition {
    let trace = spec.kind.trace(spec.total_beats);
    let world = spec.world();
    let song = SongMap::build(&trace, spec.seed, None);
    perform(&song, &world, PerformanceOptions::default())
}

/// Re-synthesize an existing [`Score`] (used for score-level mutations).
pub fn render_score(score: &Score, world: &MusicWorld, sr: SampleRate) -> Rendered {
    let mut synth = HumanMusicSynth::new(score, world, sr);
    let frames = synth.total_samples() as usize;
    let out = OfflineRenderer::new(sr, HARNESS_BLOCK).render(&mut synth, frames as u64);
    Rendered {
        audio: out.audio,
        sr,
        frames,
    }
}

fn quality_str(q: Quality) -> String {
    match q {
        Quality::Maj => "maj",
        Quality::Min => "min",
        Quality::Dim => "dim",
        Quality::Aug => "aug",
        Quality::Maj7 => "maj7",
        Quality::Min7 => "min7",
        Quality::Dom7 => "dom7",
        Quality::Min7b5 => "min7b5",
        Quality::Dim7 => "dim7",
        Quality::MinMaj7 => "minmaj7",
        Quality::Sus4 => "sus4",
        Quality::Sus2 => "sus2",
        Quality::Maj9 => "maj9",
        Quality::Min9 => "min9",
        Quality::Dom9 => "dom9",
        Quality::Add9 => "add9",
        Quality::Maj6 => "maj6",
        Quality::Min6 => "min6",
    }
    .to_string()
}

fn quality_family(q: Quality) -> &'static str {
    match q {
        Quality::Maj | Quality::Maj7 | Quality::Dom7 | Quality::Maj9 | Quality::Dom9
        | Quality::Add9 | Quality::Maj6 => "major",
        Quality::Min | Quality::Min7 | Quality::MinMaj7 | Quality::Min9 | Quality::Min6 => "minor",
        Quality::Dim | Quality::Min7b5 | Quality::Dim7 => "diminished",
        Quality::Aug => "augmented",
        Quality::Sus4 | Quality::Sus2 => "suspended",
    }
}

fn drum_voice(v: DrumVoice) -> &'static str {
    match v {
        DrumVoice::Kick => "kick",
        DrumVoice::Snare => "snare",
        DrumVoice::ClosedHat => "hat",
        DrumVoice::OpenHat => "open-hat",
        DrumVoice::Clap => "clap",
    }
}

fn section_family(kind: SectionKind) -> &'static str {
    kind.label()
}

/// Convert a hidden [`Score`] into neutral ground truth, given the rendered-source hashes.
pub fn truth_from_score(
    spec: &ItemSpec,
    score: &Score,
    duration_seconds: f64,
    wav_sha256: String,
) -> TruthTrack {
    let bpm = score.tempo_bpm as f64;
    let interval = 60.0 / bpm.max(1e-9);
    let beat_sec = |b: f64| b * interval;

    let notes: Vec<TruthNote> = score
        .notes
        .iter()
        .map(|n| TruthNote {
            onset_beat: n.start_beat,
            dur_beats: n.dur_beats as f64,
            pitch_midi: n.pitch as f64,
            velocity: n.velocity as f64,
            role: n.role.label().to_string(),
            onset_second: beat_sec(n.start_beat),
            offset_second: beat_sec(n.start_beat + n.dur_beats as f64),
        })
        .collect();

    let chords: Vec<TruthChord> = score
        .chords
        .iter()
        .map(|c| TruthChord {
            at_beat: c.start_beat,
            end_beat: c.start_beat + c.dur_beats as f64,
            root_pc: c.chord.root_pc,
            quality: quality_str(c.chord.quality),
            family: quality_family(c.chord.quality).to_string(),
        })
        .collect();

    let drums: Vec<TruthStroke> = score
        .drums
        .iter()
        .map(|d| TruthStroke {
            at_beat: d.start_beat,
            second: beat_sec(d.start_beat),
            voice: drum_voice(d.voice).to_string(),
        })
        .collect();

    // Section families by `SectionKind` label, in first-appearance order.
    let mut families: Vec<(String, usize)> = Vec::new();
    let sections: Vec<TruthSection> = score
        .sections
        .iter()
        .map(|s| {
            let label = section_family(s.kind).to_string();
            let family = match families.iter().find(|(l, _)| *l == label) {
                Some((_, id)) => *id,
                None => {
                    let id = families.len();
                    families.push((label, id));
                    id
                }
            };
            TruthSection {
                start_beat: s.start_bar as f64 * BEATS_PER_BAR,
                end_beat: s.end_bar() as f64 * BEATS_PER_BAR,
                family,
            }
        })
        .collect();

    let mut established = Vec::new();
    if notes.iter().any(|n| n.role == "lead") {
        established.push("motif".to_string());
    }
    if !chords.is_empty() {
        established.push("harmony".to_string());
    }
    if drums.iter().any(|d| d.voice == "kick" || d.voice == "snare") {
        established.push("groove".to_string());
    }
    if !sections.is_empty() {
        established.push("form".to_string());
    }
    established.push("orchestration".to_string());

    TruthTrack {
        schema: TRUTH_SCHEMA.into(),
        id: spec.id.clone(),
        world: format!("{:?}", spec.world),
        seed: spec.seed,
        split: spec.split.as_str().into(),
        tempo_bpm: bpm,
        beats_per_bar: score.beats_per_bar,
        total_beats: score.total_beats,
        duration_seconds,
        notes,
        chords,
        drums,
        sections,
        established_axes: established,
        wav_sha256,
        pcm_sha256: None,
    }
}

/// A convenience for the CLI: the role set is stable.
pub fn lead_role() -> Role {
    Role::Lead
}

/// One dataset entry in the on-disk manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestItem {
    pub id: String,
    pub world: String,
    pub kind: String,
    pub seed: u64,
    pub total_beats: f64,
    pub split: String,
    pub wav_sha256: String,
    pub truth_sha256: String,
    pub duration_seconds: f64,
    pub established_axes: Vec<String>,
}

/// The dataset manifest: the sealed index of what was generated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub anchor_rev: String,
    pub sample_rate_hz: u32,
    pub items: Vec<ManifestItem>,
}

impl Manifest {
    pub const SCHEMA: &'static str = "sai.dataset/v1";

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_builds_and_truth_is_consistent() {
        let spec = ItemSpec::canonical(WorldId::BlackIce, TraceKind::Demo, 1, 32.0, Split::Dev);
        let comp = build_composition(&spec);
        let truth = truth_from_score(&spec, &comp.score, 16.0, "d".repeat(64));
        truth.validate().unwrap();
        assert!(!truth.notes.is_empty());
        assert!(truth.established_axes.contains(&"motif".to_string()));
        assert!(truth.tempo_bpm > 40.0);
    }
}

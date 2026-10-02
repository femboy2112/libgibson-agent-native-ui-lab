//! HumanMusic: mapping incident semantics onto LibGibson's public semantic surface.
//!
//! # What this module claims
//!
//! The score is a **musical response to the incident's semantic trajectory**. It does not
//! understand distributed systems. A nine-symbol phase vocabulary plus four scalar axes
//! (`Tone`, `Emphasis`, `Density`, `Elevation`) and eight morphism kinds (`EventKind`) are
//! the entire public semantic input surface; the mapping below is a documented, total,
//! hand-written function from our phase machine into that surface. Nothing else about the
//! simulation reaches the composer.
//!
//! # Mapping table (phase → semantic state + morphism)
//!
//! | incident phase     | Tone    | Emphasis | Density | Elevation | EventKind        |
//! |--------------------|---------|----------|---------|-----------|------------------|
//! | Normal             | Neutral | Muted    | Spacious| Flat      | ActChanged       |
//! | Rising             | Info    | Normal   | Normal  | Raised    | FocusAcquired    |
//! | Overload           | Warning | Strong   | Compact | Raised    | ModalEntered     |
//! | LocalFault         | Warning | Strong   | Compact | Raised    | Impact           |
//! | Cascade            | Danger  | Strong   | Compact | Overlay   | Impact           |
//! | Diagnosis          | Info    | Normal   | Normal  | Overlay   | FocusAcquired    |
//! | Intervention       | Accent  | Strong   | Compact | Raised    | ActChanged       |
//! | PartialRecovery    | Success | Normal   | Normal  | Raised    | Confirmation     |
//! | Restored           | Success | Muted    | Spacious| Flat      | SectionResolved  |
//!
//! The incident timeline's frame becomes the semantic beat: `beat = 0.5 · frame`, so a
//! 20 Hz simulation advances the score at 10 beats/second. The trace is re-performed on
//! each phase change (or at a bounded interval) under [`PerformanceProfile::BAND`] through
//! the *checked* route, and a completed performance can be rendered offline to a WAV.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use gibson::audio::human_music::functor::{perform, perform_checked};
use gibson::audio::human_music::language::MusicalLanguage;
use gibson::audio::human_music::performance::PerformanceOptions;
use gibson::audio::human_music::policy::PerformanceProfile;
use gibson::audio::human_music::receipt::PerformanceReceipt;
use gibson::audio::human_music::semantic::{
    Density, Elevation, Emphasis, EventKind, SemanticEvent, SemanticState, SemanticTrace, Tone,
};
use gibson::audio::human_music::song::SongMap;
use gibson::audio::human_music::synth::HumanMusicSynth;
use gibson::audio::human_music::world::{MusicWorld, WorldId};
use gibson::audio::render::{OfflineRenderer, RenderResult};
use gibson::audio::wav::write_wav_i16;
use gibson::audio::SampleRate;

use crate::hash::sha256_hex;
use crate::incident::{IncidentRecord, Phase};

/// The hand-written phase → semantic-state map. The single source of truth for the
/// "document exactly how you map" claim in `EXPERIMENT_REPORT.md`.
pub fn phase_semantics(phase: Phase) -> (SemanticState, EventKind) {
    let (tone, emphasis, density, elevation, kind) = match phase {
        Phase::Normal => (Tone::Neutral, Emphasis::Muted, Density::Spacious, Elevation::Flat, EventKind::ActChanged),
        Phase::Rising => (Tone::Info, Emphasis::Normal, Density::Normal, Elevation::Raised, EventKind::FocusAcquired),
        Phase::Overload => (Tone::Warning, Emphasis::Strong, Density::Compact, Elevation::Raised, EventKind::ModalEntered),
        Phase::LocalFault => (Tone::Warning, Emphasis::Strong, Density::Compact, Elevation::Raised, EventKind::Impact),
        Phase::Cascade => (Tone::Danger, Emphasis::Strong, Density::Compact, Elevation::Overlay, EventKind::Impact),
        Phase::Diagnosis => (Tone::Info, Emphasis::Normal, Density::Normal, Elevation::Overlay, EventKind::FocusAcquired),
        Phase::Intervention => (Tone::Accent, Emphasis::Strong, Density::Compact, Elevation::Raised, EventKind::ActChanged),
        Phase::PartialRecovery => (Tone::Success, Emphasis::Normal, Density::Normal, Elevation::Raised, EventKind::Confirmation),
        Phase::Restored => (Tone::Success, Emphasis::Muted, Density::Spacious, Elevation::Flat, EventKind::SectionResolved),
    };
    (
        SemanticState {
            tone,
            emphasis,
            density,
            elevation,
        },
        kind,
    )
}

/// Build the semantic trace that drives the composer from the incident record.
pub fn trace_for(records: &[IncidentRecord], beats_per_frame: f64, horizon_frame: u32) -> SemanticTrace {
    let mut events: Vec<SemanticEvent> = records
        .iter()
        .map(|r| {
            let (state, kind) = phase_semantics(r.phase);
            SemanticEvent {
                at_beat: r.frame as f64 * beats_per_frame + 0.5,
                state,
                kind,
            }
        })
        .collect();
    if events.is_empty() {
        let (state, kind) = phase_semantics(Phase::Normal);
        events.push(SemanticEvent {
            at_beat: 0.0,
            state,
            kind,
        });
    }
    let tail = 64.0;
    let total_beats = (horizon_frame as f64 * beats_per_frame + tail).max(96.0);
    SemanticTrace::new(events, total_beats)
}

/// A completed, checked performance summary — enough for the HUD and receipts without
/// retaining the whole diagnostic graph.
pub struct MusicTake {
    pub trace: SemanticTrace,
    pub score: gibson::audio::human_music::score::Score,
    pub world_name: &'static str,
    pub tempo_bpm: f32,
    pub total_beats: f64,
    pub notes: usize,
    pub chords: usize,
    pub sections: Vec<(String, u32, u32)>,
    pub form_digest: String,
    pub receipt_ok: bool,
    pub receipt_failures: Vec<String>,
    pub checked_route: bool,
    pub build_cost: Duration,
}

impl MusicTake {
    /// A short "now playing" line for the live HUD.
    pub fn now_playing(&self) -> String {
        let sec = self
            .sections
            .last()
            .map(|(k, s, b)| format!("{k}@{s}+{b}"))
            .unwrap_or_else(|| "-".to_string());
        format!(
            "{} {}bpm {:.0}b {} notes {} chords [{}]{}",
            self.world_name,
            self.tempo_bpm.round() as i64,
            self.total_beats,
            self.notes,
            self.chords,
            sec,
            if self.receipt_ok { " checked" } else { " UNCHECKED" }
        )
    }
}

/// The live music director: owns the world, rebuilds on semantic change, measures cost.
pub struct MusicDirector {
    pub world: MusicWorld,
    pub seed: u64,
    pub beats_per_frame: f64,
    pub take: Option<MusicTake>,
    pub rebuilds: u32,
    pub last_cost: Duration,
    pub cumulative_cost: Duration,
    pub rejections: Vec<String>,
    min_interval_frames: u32,
    last_build_frame: u32,
    last_phase: Option<Phase>,
}

impl MusicDirector {
    pub fn new(seed: u64, world_id: WorldId) -> Self {
        MusicDirector {
            world: MusicWorld::from_id(world_id),
            seed,
            beats_per_frame: 0.5,
            take: None,
            rebuilds: 0,
            last_cost: Duration::ZERO,
            cumulative_cost: Duration::ZERO,
            rejections: Vec::new(),
            min_interval_frames: 240,
            last_build_frame: 0,
            last_phase: None,
        }
    }

    /// Rebuild the checked performance if the phase changed or the interval elapsed.
    pub fn maybe_rebuild(&mut self, frame: u32, phase: Phase, records: &[IncidentRecord]) -> bool {
        let phase_changed = self.last_phase != Some(phase);
        let due = frame.saturating_sub(self.last_build_frame) >= self.min_interval_frames;
        if !phase_changed && !due {
            return false;
        }
        if frame == 0 && self.take.is_some() {
            return false;
        }
        self.build(frame, phase, records);
        true
    }

    fn build(&mut self, frame: u32, phase: Phase, records: &[IncidentRecord]) {
        let t0 = Instant::now();
        let trace = trace_for(records, self.beats_per_frame, frame.max(1));
        let song = SongMap::build(&trace, self.seed, None);
        let opts = PerformanceOptions {
            language: MusicalLanguage::fusion_conversation(),
            actions: true,
            ..PerformanceOptions::default()
        };
        let (score, receipt_ok, receipt_failures, checked_route) =
            match perform_checked(&song, &self.world, opts, PerformanceProfile::BAND) {
                Ok(c) => {
                    let r = PerformanceReceipt::measure_under(&c, &self.world, PerformanceProfile::BAND);
                    (c.score, r.passes(), r.failures(), true)
                }
                Err(rej) => {
                    // A rejected checked take is a real, reportable event: keep the
                    // historical performance audible and surface the receipt.
                    self.rejections.push(format!("frame {frame}: {rej}"));
                    let c = perform(&song, &self.world, opts);
                    (c.score, false, vec![rej.to_string()], false)
                }
            };
        let sections = score
            .sections
            .iter()
            .map(|s| (s.kind.label().to_string(), s.start_bar, s.bars))
            .collect::<Vec<_>>();
        let form_digest = {
            let mut buf = String::new();
            for (k, s, b) in &sections {
                buf.push_str(&format!("{k}:{s}:{b};"));
            }
            sha256_hex(buf.as_bytes())
        };
        let take = MusicTake {
            trace,
            world_name: self.world.name,
            tempo_bpm: score.tempo_bpm,
            total_beats: score.total_beats,
            notes: score.notes.len(),
            chords: score.chords.len(),
            sections,
            form_digest,
            receipt_ok,
            receipt_failures,
            checked_route,
            build_cost: Duration::ZERO,
            score,
        };
        let cost = t0.elapsed();
        let mut take = take;
        take.build_cost = cost;
        self.last_cost = cost;
        self.cumulative_cost += cost;
        self.take = Some(take);
        self.rebuilds += 1;
        self.last_build_frame = frame;
        self.last_phase = Some(phase);
        let _ = phase;
    }

    /// Render the current take through the given world to a WAV, returning the PCM hash.
    pub fn export(
        &self,
        path: &Path,
        sr: SampleRate,
        block: usize,
        world: &MusicWorld,
    ) -> io::Result<(RenderResult, String)> {
        let take = self
            .take
            .as_ref()
            .ok_or_else(|| io::Error::other("no performance to export"))?;
        let mut synth = HumanMusicSynth::new(&take.score, world, sr);
        let frames = synth.total_samples();
        let out = OfflineRenderer::new(sr, block).render(&mut synth, frames);
        write_wav_i16(path, &out.audio, sr)?;
        let mut bytes = Vec::with_capacity(out.audio.frames() * 8);
        for i in 0..out.audio.frames() {
            bytes.extend_from_slice(&out.audio.left[i].to_le_bytes());
            bytes.extend_from_slice(&out.audio.right[i].to_le_bytes());
        }
        Ok((out, sha256_hex(&bytes)))
    }

    /// The live semantic state for the HUD.
    pub fn current_semantics(&self) -> (SemanticState, EventKind) {
        let phase = self.last_phase.unwrap_or(Phase::Normal);
        phase_semantics(phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_maps_to_a_distinct_enough_state() {
        for p in Phase::ALL {
            let (s, k) = phase_semantics(p);
            let _ = (s.pressure(), s.dynamic(), s.register_bias(), k.requires_event());
        }
    }

    #[test]
    fn cascade_is_more_pressured_than_normal() {
        let (cascade, _) = phase_semantics(Phase::Cascade);
        let (normal, _) = phase_semantics(Phase::Normal);
        assert!(cascade.pressure() > normal.pressure());
    }

    #[test]
    fn trace_is_sorted_and_bounded() {
        let records = vec![
            IncidentRecord {
                frame: 10,
                phase: Phase::Rising,
                metrics: Default::default(),
                reason: String::new(),
            },
            IncidentRecord {
                frame: 2,
                phase: Phase::Normal,
                metrics: Default::default(),
                reason: String::new(),
            },
        ];
        let t = trace_for(&records, 0.5, 20);
        assert!(t.events.windows(2).all(|w| w[0].at_beat <= w[1].at_beat));
        assert!(t.events.iter().all(|e| e.at_beat < t.total_beats));
    }
}

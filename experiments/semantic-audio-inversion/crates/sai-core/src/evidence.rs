//! The neutral, versioned evidence IR.
//!
//! This is the boundary between measurement instruments and the semantic core. Nothing in
//! this module knows about `CoverMap`; nothing here assumes a global fixed BPM, a key, or a
//! score. Every inferred record carries [`Provenance`], and every value is optional or an
//! explicit refusal, so **Unknown and Ambiguous are representable**.
//!
//! ## Levels
//!
//! The forward map distinguishes source observation from derived analysis. The inverse map
//! has no direct access to the score, so its coarsest honest level is *acoustic observation*
//! of PCM. Anything a transcription/beat/tonal model emits is a
//! [`EvidenceLevel::DerivedMusicalEvent`] or [`EvidenceLevel::DerivedStructuralInterpretation`]
//! derived from that observation. We never promote a model's emission to `Observed`.

use serde::{Deserialize, Serialize};

/// Schema identifier; bump when the wire format changes incompatibly.
pub const SCHEMA: &str = "sai.evidence/v1";

/// How directly a datum observes the signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceLevel {
    /// A measured property of the canonical PCM itself (e.g. an onset-strength peak time).
    AcousticObservation,
    /// A musical event inferred from acoustic observation (a note, a beat, a chord).
    DerivedMusicalEvent,
    /// A structural or semantic interpretation built on musical events (a section, a motif).
    DerivedStructuralInterpretation,
}

/// Alias kept for callers that read "analysis level" in the protocol.
pub type AnalysisLevel = EvidenceLevel;

/// The instrument that produced a datum, and the chain it was derived through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Stable analyzer id, e.g. `sai-baseline-dsp` or `external:essentia`.
    pub analyzer_id: String,
    /// Exact analyzer version / package version / model identifier.
    pub analyzer_version: String,
    /// The epistemic level of this datum.
    pub level: EvidenceLevel,
    /// Named method, e.g. `spectral-flux+dp-beat/v1`.
    pub method: String,
    /// The evidence this was derived from, e.g. `["pcm"]`, `["timing:sai-baseline-dsp"]`.
    pub derived_from: Vec<String>,
}

impl Provenance {
    /// Construct a derived-musical provenance.
    pub fn derived(analyzer: &str, version: &str, method: &str, from: &[&str]) -> Self {
        Self {
            analyzer_id: analyzer.to_string(),
            analyzer_version: version.to_string(),
            level: EvidenceLevel::DerivedMusicalEvent,
            method: method.to_string(),
            derived_from: from.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Construct an acoustic-observation provenance.
    pub fn acoustic(analyzer: &str, version: &str, method: &str) -> Self {
        Self {
            analyzer_id: analyzer.to_string(),
            analyzer_version: version.to_string(),
            level: EvidenceLevel::AcousticObservation,
            method: method.to_string(),
            derived_from: vec!["pcm".to_string()],
        }
    }
}

/// One analyzer execution recorded in the artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerRun {
    pub id: String,
    pub version: String,
    /// Free-form configuration actually used.
    #[serde(default)]
    pub config: serde_json::Value,
    /// Wall-clock seconds actually spent.
    #[serde(default)]
    pub wall_seconds: Option<f64>,
    /// Model/weights identifier where applicable.
    #[serde(default)]
    pub weights: Option<String>,
    /// True when the run was served from a cache rather than freshly executed.
    #[serde(default)]
    pub cached: bool,
}

/// The canonical audio boundary receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceReceipt {
    pub sha256: String,
    /// Basename only (never a leaking absolute path in committed artifacts).
    #[serde(default)]
    pub path_hint: Option<String>,
    pub duration_seconds: f64,
    /// Sample rate as decoded from the container.
    pub sample_rate_hz: u32,
    pub channels: u8,
    /// e.g. `ffmpeg 6.1.1-3ubuntu5`.
    pub decoder: String,
    /// Canonical decode target.
    pub canonical_format: String,
    pub canonical_sample_rate_hz: u32,
    pub canonical_channels: u8,
    /// True when canonicalization changed the sample rate.
    pub resampled: bool,
    /// `none`, `stereo`, or `mono-downmix`.
    pub channel_conversion: String,
    /// SHA-256 of the canonical interleaved f32le PCM actually analyzed.
    #[serde(default)]
    pub canonical_pcm_sha256: Option<String>,
    /// License / custody note.
    #[serde(default)]
    pub license: String,
}

/// A tempo hypothesis segment. Seconds coordinates are authoritative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoSegment {
    pub at_second: f64,
    pub bpm: f64,
    pub confidence: f64,
}

/// A beat event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatEvent {
    pub second: f64,
    pub confidence: f64,
    #[serde(default)]
    pub is_downbeat: bool,
}

/// A meter hypothesis: beats per bar with a phase offset in beats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterHypothesis {
    pub beats_per_bar: u32,
    pub phase_beats: f64,
    pub support: f64,
}

/// Timing / beat evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingEvidence {
    pub provenance: Provenance,
    #[serde(default)]
    pub tempo_bpm: Option<f64>,
    #[serde(default)]
    pub tempo_map: Vec<TempoSegment>,
    #[serde(default)]
    pub beats: Vec<BeatEvent>,
    #[serde(default)]
    pub downbeats: Vec<f64>,
    #[serde(default)]
    pub meter_hypotheses: Vec<MeterHypothesis>,
    #[serde(default)]
    pub ambiguity: Vec<String>,
}

/// A note-event's candidate instrument/role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleCandidate {
    pub role: String,
    pub confidence: f64,
}

/// A transcribed note event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteEvidence {
    pub onset_second: f64,
    #[serde(default)]
    pub offset_second: Option<f64>,
    pub pitch_midi: f64,
    #[serde(default)]
    pub role_candidates: Vec<RoleCandidate>,
    pub confidence: f64,
    pub provenance: Provenance,
}

/// A chroma / HPCP frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChromaFrame {
    pub center_second: f64,
    pub hop_seconds: f64,
    /// 12 bins, pitch-class 0 = C.
    pub values: [f64; 12],
}

/// A key hypothesis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyCandidate {
    pub tonic_pc: i32,
    /// `major` or `minor` (modal names are not claimed by the audio route).
    pub mode: String,
    pub confidence: f64,
}

/// A chord hypothesis over a span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChordCandidate {
    pub at_second: f64,
    pub end_second: f64,
    pub root_pc: i32,
    /// `maj`, `min`, `dim`, `aug`, `sus`, `dom7`, `maj7`, `min7`, `min7b5`, `unknown`.
    pub quality: String,
    /// `root`, `first`, `second`, or `unknown`.
    #[serde(default = "default_inversion")]
    pub inversion: String,
    pub confidence: f64,
}

fn default_inversion() -> String {
    "unknown".to_string()
}

/// Tonal / harmonic evidence. Independent of note transcription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TonalEvidence {
    pub provenance: Provenance,
    #[serde(default)]
    pub chroma_frames: Vec<ChromaFrame>,
    #[serde(default)]
    pub key_candidates: Vec<KeyCandidate>,
    #[serde(default)]
    pub chord_candidates: Vec<ChordCandidate>,
}

/// A percussion family candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyCandidate {
    pub family: String,
    pub confidence: f64,
}

/// An onset / transient event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnsetEvidence {
    pub second: f64,
    pub strength: f64,
    #[serde(default)]
    pub family_candidates: Vec<FamilyCandidate>,
    pub provenance: Provenance,
}

/// A recurrence link between two times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurrenceLink {
    pub at_second: f64,
    pub to_second: f64,
    pub similarity: f64,
}

/// Recurrence / self-similarity evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurrenceEvidence {
    pub provenance: Provenance,
    #[serde(default)]
    pub links: Vec<RecurrenceLink>,
    #[serde(default)]
    pub summary: String,
}

/// A candidate section span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionSpan {
    pub start_second: f64,
    pub end_second: f64,
    /// A *candidate* label; never asserted as a human section name.
    #[serde(default)]
    pub label_candidate: Option<String>,
}

/// Section / form evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionEvidence {
    pub provenance: Provenance,
    #[serde(default)]
    pub boundaries: Vec<f64>,
    #[serde(default)]
    pub sections: Vec<SectionSpan>,
}

/// An explicit refusal by an analyzer or the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub what: String,
    pub reason: String,
}

/// The complete evidence artifact for one source + one analyzer set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceArtifact {
    pub schema: String,
    pub source: SourceReceipt,
    #[serde(default)]
    pub analyzers: Vec<AnalyzerRun>,
    #[serde(default)]
    pub timing: Option<TimingEvidence>,
    #[serde(default)]
    pub notes: Vec<NoteEvidence>,
    #[serde(default)]
    pub tonal: Option<TonalEvidence>,
    #[serde(default)]
    pub onsets: Vec<OnsetEvidence>,
    #[serde(default)]
    pub recurrence: Option<RecurrenceEvidence>,
    #[serde(default)]
    pub sections: Option<SectionEvidence>,
    #[serde(default)]
    pub refusals: Vec<Refusal>,
    #[serde(default)]
    pub unknowns: Vec<String>,
}

impl EvidenceArtifact {
    /// Validate every domain invariant and fail closed on the first violation.
    pub fn validate(&self) -> crate::SaiResult<()> {
        use crate::SaiError;
        if self.schema != SCHEMA {
            return Err(SaiError::Schema(format!(
                "unexpected schema '{}', expected '{SCHEMA}'",
                self.schema
            )));
        }
        let s = &self.source;
        if s.sha256.len() != 64 || !s.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(SaiError::Schema("source.sha256 must be 64 hex chars".into()));
        }
        if s.canonical_sample_rate_hz == 0 || s.sample_rate_hz == 0 {
            return Err(SaiError::Schema("zero sample rate".into()));
        }
        if !(1..=8).contains(&s.channels) {
            return Err(SaiError::Schema("source.channels out of range".into()));
        }
        crate::error::finite(s.duration_seconds, "source.duration_seconds")?;
        if s.duration_seconds < 0.0 {
            return Err(SaiError::Schema("negative duration".into()));
        }
        if let Some(t) = &self.timing {
            if let Some(bpm) = t.tempo_bpm {
                crate::error::in_range(bpm, 1.0, 1000.0, "timing.tempo_bpm")?;
            }
            let mut last = f64::NEG_INFINITY;
            for b in &t.beats {
                crate::error::in_range(b.second, -1e-6, s.duration_seconds + 1e-6, "beat.second")?;
                crate::error::in_range(b.confidence, 0.0, 1.0, "beat.confidence")?;
                if b.second < last {
                    return Err(SaiError::Schema("beats not time-ordered".into()));
                }
                last = b.second;
            }
            for d in &t.downbeats {
                crate::error::in_range(*d, -1e-6, s.duration_seconds + 1e-6, "downbeat")?;
            }
        }
        for n in &self.notes {
            crate::error::in_range(n.onset_second, -1e-6, s.duration_seconds + 1e-6, "note.onset")?;
            crate::error::in_range(n.pitch_midi, 0.0, 127.999, "note.pitch_midi")?;
            crate::error::in_range(n.confidence, 0.0, 1.0, "note.confidence")?;
            if let Some(off) = n.offset_second {
                crate::error::in_range(off, -1e-6, s.duration_seconds + 1e-6, "note.offset")?;
                if off < n.onset_second - 1e-9 {
                    return Err(SaiError::Schema("note offset before onset".into()));
                }
            }
        }
        for o in &self.onsets {
            crate::error::in_range(o.second, -1e-6, s.duration_seconds + 1e-6, "onset.second")?;
            crate::error::in_range(o.strength, 0.0, 1.0, "onset.strength")?;
        }
        if let Some(t) = &self.tonal {
            for f in &t.chroma_frames {
                crate::error::in_range(
                    f.center_second,
                    -1e-6,
                    s.duration_seconds + 1e-6,
                    "chroma.center",
                )?;
                crate::error::finite(f.hop_seconds, "chroma.hop")?;
                if f.hop_seconds <= 0.0 {
                    return Err(SaiError::Schema("non-positive chroma hop".into()));
                }
                if f.values.iter().any(|v| !v.is_finite() || *v < 0.0) {
                    return Err(SaiError::NonFinite("chroma.values"));
                }
            }
            for k in &t.key_candidates {
                crate::error::in_range(k.tonic_pc as f64, 0.0, 11.0, "key.tonic_pc")?;
                crate::error::in_range(k.confidence, 0.0, 1.0, "key.confidence")?;
                if k.mode != "major" && k.mode != "minor" {
                    return Err(SaiError::Schema(format!("unknown key mode '{}'", k.mode)));
                }
            }
            for c in &t.chord_candidates {
                crate::error::in_range(c.at_second, -1e-6, s.duration_seconds + 1e-6, "chord.at")?;
                crate::error::in_range(
                    c.end_second,
                    -1e-6,
                    s.duration_seconds + 1e-6,
                    "chord.end",
                )?;
                if c.end_second < c.at_second - 1e-9 {
                    return Err(SaiError::Schema("chord end before start".into()));
                }
                crate::error::in_range(c.root_pc as f64, 0.0, 11.0, "chord.root_pc")?;
                crate::error::in_range(c.confidence, 0.0, 1.0, "chord.confidence")?;
            }
        }
        if let Some(r) = &self.recurrence {
            for l in &r.links {
                crate::error::in_range(l.at_second, -1e-6, s.duration_seconds + 1e-6, "link.at")?;
                crate::error::in_range(
                    l.to_second,
                    -1e-6,
                    s.duration_seconds + 1e-6,
                    "link.to",
                )?;
                crate::error::in_range(l.similarity, 0.0, 1.0, "link.similarity")?;
            }
        }
        if let Some(sec) = &self.sections {
            for b in &sec.boundaries {
                crate::error::in_range(*b, -1e-6, s.duration_seconds + 1e-6, "section.boundary")?;
            }
            let mut last_end = f64::NEG_INFINITY;
            for sp in &sec.sections {
                crate::error::in_range(sp.start_second, -1e-6, s.duration_seconds + 1e-6, "span.start")?;
                crate::error::in_range(sp.end_second, -1e-6, s.duration_seconds + 1e-6, "span.end")?;
                if sp.end_second < sp.start_second - 1e-9 {
                    return Err(SaiError::Schema("section end before start".into()));
                }
                if sp.start_second < last_end - 1e-6 {
                    return Err(SaiError::Schema("sections overlap or are unordered".into()));
                }
                last_end = sp.end_second;
            }
        }
        Ok(())
    }

    /// Load and validate an artifact from JSON bytes.
    pub fn from_json_slice(bytes: &[u8]) -> crate::SaiResult<Self> {
        let artifact: EvidenceArtifact = serde_json::from_slice(bytes)?;
        artifact.validate()?;
        Ok(artifact)
    }

    /// Serialize deterministically (pretty, stable key order via struct field order).
    pub fn to_json(&self) -> crate::SaiResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> EvidenceArtifact {
        EvidenceArtifact {
            schema: SCHEMA.into(),
            source: SourceReceipt {
                sha256: "a".repeat(64),
                path_hint: None,
                duration_seconds: 1.0,
                sample_rate_hz: 48000,
                channels: 2,
                decoder: "ffmpeg test".into(),
                canonical_format: "f32le".into(),
                canonical_sample_rate_hz: 48000,
                canonical_channels: 2,
                resampled: false,
                channel_conversion: "stereo".into(),
                canonical_pcm_sha256: None,
                license: "generated fixture".into(),
            },
            analyzers: vec![],
            timing: None,
            notes: vec![],
            tonal: None,
            onsets: vec![],
            recurrence: None,
            sections: None,
            refusals: vec![],
            unknowns: vec![],
        }
    }

    #[test]
    fn minimal_artifact_round_trips_and_validates() {
        let a = minimal();
        a.validate().unwrap();
        let json = a.to_json().unwrap();
        let b = EvidenceArtifact::from_json_slice(json.as_bytes()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn bad_hash_fails_closed() {
        let mut a = minimal();
        a.source.sha256 = "xyz".into();
        assert!(a.validate().is_err());
    }

    #[test]
    fn non_finite_note_fails_closed() {
        let mut a = minimal();
        a.notes.push(NoteEvidence {
            onset_second: f64::NAN,
            offset_second: None,
            pitch_midi: 60.0,
            role_candidates: vec![],
            confidence: 0.5,
            provenance: Provenance::derived("t", "1", "m", &["pcm"]),
        });
        assert!(matches!(a.validate(), Err(crate::SaiError::NonFinite(_))));
    }

    #[test]
    fn unordered_beats_fail_closed() {
        let mut a = minimal();
        a.timing = Some(TimingEvidence {
            provenance: Provenance::derived("t", "1", "m", &["pcm"]),
            tempo_bpm: Some(120.0),
            tempo_map: vec![],
            beats: vec![
                BeatEvent { second: 0.5, confidence: 1.0, is_downbeat: false },
                BeatEvent { second: 0.2, confidence: 1.0, is_downbeat: false },
            ],
            downbeats: vec![],
            meter_hypotheses: vec![],
            ambiguity: vec![],
        });
        assert!(a.validate().is_err());
    }
}

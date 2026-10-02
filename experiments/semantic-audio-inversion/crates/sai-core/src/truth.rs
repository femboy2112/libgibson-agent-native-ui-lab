//! Hidden ground truth for the blind HumanMusic harness (`sai.truth/v1`).
//!
//! The analyzer never sees this module. The harness writes one `TruthTrack` per generated item
//! **outside** the analyzer's view; the evaluator compares a recovered quotient against it. The
//! truth is neutral serde data with no libgibson types, so `sai-core` stays free of the generator.
//!
//! A truth track records what the hidden score actually sounded (notes, chords, drums, sections,
//! tempo) in both raw seconds and beats, plus the axes the hidden score establishes. That last
//! list lets the evaluator score unknown/free classification, not just note accuracy.

use serde::{Deserialize, Serialize};

/// Schema id for hidden ground truth.
pub const TRUTH_SCHEMA: &str = "sai.truth/v1";

/// A hidden scored note event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthNote {
    pub onset_beat: f64,
    pub dur_beats: f64,
    pub pitch_midi: f64,
    pub velocity: f64,
    /// Realized role label (`lead`, `bass`, `keys`, `pad`, `drums`).
    pub role: String,
    pub onset_second: f64,
    pub offset_second: f64,
}

/// A hidden chord span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthChord {
    pub at_beat: f64,
    pub end_beat: f64,
    pub root_pc: i32,
    pub quality: String,
    /// Triad family (`major`/`minor`/`diminished`/`augmented`/`suspended`/`unknown`).
    pub family: String,
}

/// A hidden kick/snare stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthStroke {
    pub at_beat: f64,
    pub second: f64,
    pub voice: String,
}

/// A hidden section with its family index (never a semantic name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthSection {
    pub start_beat: f64,
    pub end_beat: f64,
    pub family: usize,
}

/// One hidden item: the direct musical truth plus the rendered-source hashes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthTrack {
    pub schema: String,
    pub id: String,
    pub world: String,
    pub seed: u64,
    /// `dev` or `holdout`.
    pub split: String,
    pub tempo_bpm: f64,
    pub beats_per_bar: f64,
    pub total_beats: f64,
    pub duration_seconds: f64,
    #[serde(default)]
    pub notes: Vec<TruthNote>,
    #[serde(default)]
    pub chords: Vec<TruthChord>,
    #[serde(default)]
    pub drums: Vec<TruthStroke>,
    #[serde(default)]
    pub sections: Vec<TruthSection>,
    /// Cover axes the hidden score actually establishes (non-empty lines, chords, drums, form).
    #[serde(default)]
    pub established_axes: Vec<String>,
    /// SHA-256 of the rendered WAV handed to the analyzer.
    pub wav_sha256: String,
    /// SHA-256 of the canonical PCM the analyzer saw.
    #[serde(default)]
    pub pcm_sha256: Option<String>,
}

impl TruthTrack {
    pub fn to_json(&self) -> crate::SaiResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
    pub fn from_json_slice(bytes: &[u8]) -> crate::SaiResult<Self> {
        let t: TruthTrack = serde_json::from_slice(bytes)?;
        t.validate()?;
        Ok(t)
    }

    pub fn validate(&self) -> crate::SaiResult<()> {
        use crate::{SaiError, SaiResult};
        if self.schema != TRUTH_SCHEMA {
            return Err(SaiError::Schema(format!(
                "truth schema '{}', expected '{TRUTH_SCHEMA}'",
                self.schema
            )));
        }
        if !(1.0..=1000.0).contains(&self.tempo_bpm) {
            return Err(SaiError::Schema("truth tempo out of range".into()));
        }
        if self.duration_seconds < 0.0 {
            return Err(SaiError::Schema("negative truth duration".into()));
        }
        for n in &self.notes {
            crate::error::finite(n.onset_beat, "truth.note.onset_beat")?;
            crate::error::finite(n.dur_beats, "truth.note.dur_beats")?;
            crate::error::in_range(n.pitch_midi, 0.0, 127.0, "truth.note.pitch")?;
        }
        for c in &self.chords {
            crate::error::finite(c.at_beat, "truth.chord.at")?;
            crate::error::finite(c.end_beat, "truth.chord.end")?;
            if c.end_beat < c.at_beat - 1e-9 {
                return Err(SaiError::Schema("truth chord end before start".into()));
            }
        }
        for s in &self.drums {
            crate::error::finite(s.at_beat, "truth.drum.at")?;
        }
        for s in &self.sections {
            crate::error::finite(s.start_beat, "truth.section.start")?;
            crate::error::finite(s.end_beat, "truth.section.end")?;
        }
        let _: SaiResult<()> = Ok(());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> TruthTrack {
        TruthTrack {
            schema: TRUTH_SCHEMA.into(),
            id: "t".into(),
            world: "BlackIce".into(),
            seed: 1,
            split: "dev".into(),
            tempo_bpm: 120.0,
            beats_per_bar: 4.0,
            total_beats: 8.0,
            duration_seconds: 4.0,
            notes: vec![],
            chords: vec![],
            drums: vec![],
            sections: vec![],
            established_axes: vec![],
            wav_sha256: "b".repeat(64),
            pcm_sha256: None,
        }
    }

    #[test]
    fn truth_round_trips() {
        let t = minimal();
        t.validate().unwrap();
        let s = t.to_json().unwrap();
        assert_eq!(TruthTrack::from_json_slice(s.as_bytes()).unwrap(), t);
    }

    #[test]
    fn bad_schema_fails_closed() {
        let mut t = minimal();
        t.schema = "nope".into();
        assert!(t.validate().is_err());
    }
}

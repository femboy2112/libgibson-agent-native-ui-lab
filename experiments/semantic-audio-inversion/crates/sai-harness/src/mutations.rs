//! Hostile transformation suite with preregistered expectations.
//!
//! Two families:
//! - **score-level** mutations change what the hidden score sounds (transposition, tempo, a note
//!   edit, a chord edit, a stem mute). These are generated from the hidden [`Score`] and rendered
//!   with the same synth, so the truth of the mutated score is known exactly.
//! - **PCM-level** production mutations (EQ, compression, reverb, added noise) leave the hidden
//!   score unchanged; only production evidence should move.
//!
//! Every mutation names what must change and what must remain invariant. The analyzer is frozen
//! before this round; deviations are residuals, never thresholds to tune away.

use gibson::audio::human_music::score::{DrumVoice, Role, Score};
use gibson::audio::human_music::theory::Quality;
use gibson::audio::StereoBlock;
use serde::{Deserialize, Serialize};

/// A preregistered expected effect of a mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationExpectation {
    /// Axes that must change under this mutation.
    pub must_change: Vec<String>,
    /// Axes that should remain invariant.
    pub must_hold: Vec<String>,
}

/// A hostile transformation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Mutation {
    /// Shift every note and chord root by `semitones`.
    Transpose { semitones: i32 },
    /// Stretch all beat coordinates by `factor` (audio duration scales; relative identity holds).
    TempoScale { factor: f64 },
    /// Delete the `index`-th lead note.
    DeleteLeadNote { index: usize },
    /// Move the `index`-th lead note later by `delta_beats`.
    ShiftLeadNote { index: usize, delta_beats: f64 },
    /// Change the quality of the `index`-th chord.
    ChordEdit { index: usize },
    /// Remove every note of `role`.
    MuteRole { role: String },
    /// Remove every kick and snare.
    MuteDrums,
    /// High-shelf boost (production).
    Eq,
    /// Soft compression (production).
    Compress,
    /// A short synthetic reverb tail (production).
    Reverb,
    /// Add low-level white noise (negative control).
    Noise,
}

impl Mutation {
    /// True when the mutation is applied to rendered PCM, not the score.
    pub fn is_pcm_only(&self) -> bool {
        matches!(self, Mutation::Eq | Mutation::Compress | Mutation::Reverb | Mutation::Noise)
    }

    pub fn label(&self) -> String {
        match self {
            Mutation::Transpose { semitones } => format!("transpose-{semitones:+}"),
            Mutation::TempoScale { factor } => format!("tempo-x{factor}"),
            Mutation::DeleteLeadNote { index } => format!("delete-lead-{index}"),
            Mutation::ShiftLeadNote { index, delta_beats } => {
                format!("shift-lead-{index}-{delta_beats:+}")
            }
            Mutation::ChordEdit { index } => format!("chord-edit-{index}"),
            Mutation::MuteRole { role } => format!("mute-{role}"),
            Mutation::MuteDrums => "mute-drums".into(),
            Mutation::Eq => "eq".into(),
            Mutation::Compress => "compress".into(),
            Mutation::Reverb => "reverb".into(),
            Mutation::Noise => "noise".into(),
        }
    }

    pub fn expectation(&self) -> MutationExpectation {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        match self {
            Mutation::Transpose { .. } => MutationExpectation {
                must_change: s(&["notes.all", "harmony.root"]),
                must_hold: s(&["motif.relation", "timing.tempo"]),
            },
            Mutation::TempoScale { .. } => MutationExpectation {
                must_change: s(&["timing.tempo"]),
                must_hold: s(&["motif.relation"]),
            },
            Mutation::DeleteLeadNote { .. } | Mutation::ShiftLeadNote { .. } => {
                MutationExpectation {
                    must_change: s(&["motif.relation"]),
                    must_hold: s(&["timing.tempo"]),
                }
            }
            Mutation::ChordEdit { .. } => MutationExpectation {
                must_change: s(&["harmony.relation"]),
                must_hold: s(&["motif.relation"]),
            },
            Mutation::MuteRole { .. } => MutationExpectation {
                must_change: s(&["notes.all"]),
                must_hold: s(&["timing.tempo"]),
            },
            Mutation::MuteDrums => MutationExpectation {
                must_change: s(&["groove.relation"]),
                must_hold: s(&["motif.relation"]),
            },
            Mutation::Eq | Mutation::Compress | Mutation::Reverb => MutationExpectation {
                must_change: s(&[]),
                must_hold: s(&["motif.relation", "timing.tempo"]),
            },
            Mutation::Noise => MutationExpectation {
                must_change: s(&[]),
                must_hold: s(&["timing.tempo"]),
            },
        }
    }
}

fn role_of(s: &str) -> Option<Role> {
    Some(match s {
        "lead" => Role::Lead,
        "bass" => Role::Bass,
        "keys" => Role::Keys,
        "pad" => Role::Pad,
        _ => return None,
    })
}

fn flip_quality(q: Quality) -> Quality {
    match q {
        Quality::Maj | Quality::Maj7 | Quality::Dom7 | Quality::Maj9 | Quality::Dom9
        | Quality::Add9 | Quality::Maj6 => Quality::Min,
        _ => Quality::Maj,
    }
}

/// Apply a score-level mutation in place. PCM-only mutations are a no-op here.
pub fn apply_score(score: &mut Score, m: &Mutation) {
    match m {
        Mutation::Transpose { semitones } => {
            for n in &mut score.notes {
                n.pitch += *semitones;
            }
            for c in &mut score.chords {
                c.chord.root_pc = (c.chord.root_pc + *semitones).rem_euclid(12);
            }
        }
        Mutation::TempoScale { factor } => {
            let f = *factor;
            for n in &mut score.notes {
                n.start_beat *= f;
                n.dur_beats = (n.dur_beats as f64 * f) as f32;
            }
            for d in &mut score.drums {
                d.start_beat *= f;
            }
            for c in &mut score.chords {
                c.start_beat *= f;
                c.dur_beats = (c.dur_beats as f64 * f) as f32;
            }
            score.total_beats *= f;
            for s in &mut score.sections {
                s.start_bar = (s.start_bar as f64 * f).round() as u32;
                s.bars = (s.bars as f64 * f).round() as u32;
            }
        }
        Mutation::DeleteLeadNote { index } => {
            let mut seen = 0usize;
            score.notes.retain(|n| {
                if n.role == Role::Lead {
                    let keep = seen != *index;
                    seen += 1;
                    keep
                } else {
                    true
                }
            });
        }
        Mutation::ShiftLeadNote { index, delta_beats } => {
            let mut seen = 0usize;
            for n in &mut score.notes {
                if n.role == Role::Lead {
                    if seen == *index {
                        n.start_beat += *delta_beats;
                    }
                    seen += 1;
                }
            }
        }
        Mutation::ChordEdit { index } => {
            if let Some(c) = score.chords.get_mut(*index) {
                c.chord.quality = flip_quality(c.chord.quality);
            }
        }
        Mutation::MuteRole { role } => {
            if let Some(r) = role_of(role) {
                score.notes.retain(|n| n.role != r);
            }
        }
        Mutation::MuteDrums => {
            score
                .drums
                .retain(|d| !matches!(d.voice, DrumVoice::Kick | DrumVoice::Snare));
        }
        _ => {}
    }
}

fn xorshift(state: &mut u64) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    (x >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
}

/// Apply a PCM-only production mutation in place.
pub fn apply_audio(audio: &mut StereoBlock, m: &Mutation, sr: u32) {
    match m {
        Mutation::Eq => {
            // One-pole low-pass, then a high-shelf boost of the residual.
            let a = 0.15f32;
            for ch in [&mut audio.left, &mut audio.right] {
                let mut lp = 0.0f32;
                for s in ch.iter_mut() {
                    lp += a * (*s - lp);
                    *s += 0.9 * (*s - lp);
                }
            }
        }
        Mutation::Compress => {
            let k = 3.0f32;
            let norm = (k).tanh();
            for ch in [&mut audio.left, &mut audio.right] {
                for s in ch.iter_mut() {
                    *s = (*s * k).tanh() / norm;
                }
            }
        }
        Mutation::Reverb => {
            // Two decaying feedback taps (~90ms and ~140ms) plus a small gain.
            let d1 = (sr as f32 * 0.09) as usize;
            let d2 = (sr as f32 * 0.14) as usize;
            let g = 0.35f32;
            for ch in [&mut audio.left, &mut audio.right] {
                let n = ch.len();
                let mut out = ch.clone();
                for i in 0..n {
                    let mut v = 0.0f32;
                    if i >= d1 {
                        v += 0.6 * out[i - d1];
                    }
                    if i >= d2 {
                        v += 0.4 * out[i - d2];
                    }
                    out[i] = ch[i] + g * v;
                }
                *ch = out;
            }
        }
        Mutation::Noise => {
            let mut seed = 0x9E3779B97F4A7C15u64;
            for ch in [&mut audio.left, &mut audio.right] {
                for s in ch.iter_mut() {
                    *s += 0.01 * xorshift(&mut seed);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transposition_shifts_pitches_and_roots() {
        let mut s = Score::new(120.0, 4.0, 8.0);
        s.notes.push(gibson::audio::human_music::score::Note::new(
            0.0,
            1.0,
            60,
            0.8,
            Role::Lead,
            gibson::audio::human_music::score::Provenance::new(
                gibson::audio::human_music::form::SectionKind::A,
            ),
        ));
        apply_score(&mut s, &Mutation::Transpose { semitones: 3 });
        assert_eq!(s.notes[0].pitch, 63);
    }

    #[test]
    fn mute_drums_removes_kick_and_snare() {
        let mut s = Score::new(120.0, 4.0, 8.0);
        let p = gibson::audio::human_music::score::Provenance::new(
            gibson::audio::human_music::form::SectionKind::A,
        );
        s.drums.push(gibson::audio::human_music::score::DrumHit {
            start_beat: 0.0,
            voice: DrumVoice::Kick,
            velocity: 0.9,
            prov: p,
        });
        s.drums.push(gibson::audio::human_music::score::DrumHit {
            start_beat: 1.0,
            voice: DrumVoice::ClosedHat,
            velocity: 0.5,
            prov: p,
        });
        apply_score(&mut s, &Mutation::MuteDrums);
        assert_eq!(s.drums.len(), 1);
        assert!(matches!(s.drums[0].voice, DrumVoice::ClosedHat));
    }

    #[test]
    fn expectations_are_preregistered_for_every_mutation() {
        let all = [
            Mutation::Transpose { semitones: 5 },
            Mutation::TempoScale { factor: 1.25 },
            Mutation::Eq,
            Mutation::Noise,
        ];
        for m in all {
            let e = m.expectation();
            assert!(e.must_change.len() + e.must_hold.len() > 0, "{m:?}");
        }
    }
}

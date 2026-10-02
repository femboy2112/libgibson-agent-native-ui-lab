//! Beat-domain transport: `seconds <-> beat` while **always preserving seconds**.
//!
//! The evidence IR stores raw seconds as authoritative. This module derives a beat
//! coordinate by piecewise-linear interpolation between detected beat times, so a local
//! tempo change or drift is represented without baking in a global fixed BPM. Quantization to
//! a rational metric grid is offered separately and always returns the **residual**, so a
//! later observer can distinguish true off-grid timing from quantization error.
//!
//! A `BeatGrid` is a *hypothesis*, not a truth. When the analyzer has rival tempo hypotheses,
//! the caller keeps the alternatives in [`crate::evidence::TimingEvidence::ambiguity`]; this
//! struct represents one chosen transport.

use serde::{Deserialize, Serialize};

use crate::error::{finite, SaiError, SaiResult};
use crate::evidence::TimingEvidence;

/// Seconds <-> beat map built from detected beat times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatGrid {
    /// Beat times in seconds, strictly increasing.
    beats: Vec<f64>,
    /// Indices into `beats` that are downbeats.
    downbeat_indices: Vec<usize>,
    /// Nominal beats per bar of the selected meter hypothesis (1 = unknown).
    beats_per_bar: u32,
}

/// A beat-domain coordinate that retains its raw seconds origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatCoordinate {
    /// Authoritative raw seconds.
    pub seconds: f64,
    /// Interpolated beat position (integer exactly on a detected beat).
    pub beat: f64,
}

impl BeatGrid {
    /// Build from timing evidence. Requires strictly increasing beat times.
    pub fn from_timing(t: &TimingEvidence) -> SaiResult<Self> {
        let beats: Vec<f64> = t
            .beats
            .iter()
            .map(|b| b.second)
            .collect::<Vec<_>>();
        Self::from_beats(&beats, &t.downbeats, t.meter_hypotheses.iter().map(|m| m.beats_per_bar).max().unwrap_or(4))
    }

    /// Build directly from beat seconds.
    pub fn from_beats(beats: &[f64], downbeats_seconds: &[f64], beats_per_bar: u32) -> SaiResult<Self> {
        if beats.is_empty() {
            return Err(SaiError::NoEvidence("beat grid (no beats detected)"));
        }
        for w in beats.windows(2) {
            if !(w[1] > w[0]) {
                return Err(SaiError::Schema("beat times must strictly increase".into()));
            }
        }
        for b in beats {
            finite(*b, "beat")?;
        }
        let mut downbeat_indices = Vec::new();
        for d in downbeats_seconds {
            if let Some(i) = beats
                .iter()
                .position(|b| (b - d).abs() < 1e-6)
            {
                downbeat_indices.push(i);
            }
        }
        Ok(Self {
            beats: beats.to_vec(),
            downbeat_indices,
            beats_per_bar: beats_per_bar.max(1),
        })
    }

    /// Number of detected beats.
    pub fn len(&self) -> usize {
        self.beats.len()
    }

    /// True if the grid has no beats (never constructed, but part of the API contract).
    pub fn is_empty(&self) -> bool {
        self.beats.is_empty()
    }

    /// Beat times in seconds.
    pub fn beat_seconds(&self) -> &[f64] {
        &self.beats
    }

    /// Nominal beats per bar of the selected meter hypothesis.
    pub fn beats_per_bar(&self) -> u32 {
        self.beats_per_bar
    }

    /// Median beat interval (seconds) — a robust local tempo estimate.
    pub fn median_interval(&self) -> f64 {
        let mut d: Vec<f64> = self.beats.windows(2).map(|w| w[1] - w[0]).collect();
        if d.is_empty() {
            return 0.0;
        }
        d.sort_by(|a, b| a.total_cmp(b));
        d[d.len() / 2]
    }

    /// Median BPM.
    pub fn median_bpm(&self) -> f64 {
        let i = self.median_interval();
        if i > 0.0 {
            60.0 / i
        } else {
            0.0
        }
    }

    /// Map seconds to a beat coordinate by piecewise-linear interpolation.
    ///
    /// Before the first beat / after the last, extrapolates with the nearest local interval so
    /// the coordinate stays defined; the seconds field is unchanged and authoritative.
    pub fn seconds_to_beat(&self, seconds: f64) -> SaiResult<BeatCoordinate> {
        finite(seconds, "seconds")?;
        let n = self.beats.len();
        if n == 1 {
            return Ok(BeatCoordinate { seconds, beat: 0.0 });
        }
        let beat = if seconds <= self.beats[0] {
            let dt = self.beats[1] - self.beats[0];
            (seconds - self.beats[0]) / dt
        } else if seconds >= self.beats[n - 1] {
            let dt = self.beats[n - 1] - self.beats[n - 2];
            (n as f64 - 1.0) + (seconds - self.beats[n - 1]) / dt
        } else {
            // Binary-search the right segment.
            let mut lo = 0usize;
            let mut hi = n - 1;
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if self.beats[mid] <= seconds {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let dt = self.beats[hi] - self.beats[lo];
            lo as f64 + (seconds - self.beats[lo]) / dt
        };
        Ok(BeatCoordinate { seconds, beat })
    }

    /// Map a beat position back to seconds (inverse of [`Self::seconds_to_beat`]).
    pub fn beat_to_seconds(&self, beat: f64) -> SaiResult<f64> {
        finite(beat, "beat")?;
        let n = self.beats.len();
        if n == 1 {
            return Ok(self.beats[0]);
        }
        let seconds = if beat <= 0.0 {
            let dt = self.beats[1] - self.beats[0];
            self.beats[0] + beat * dt
        } else if beat >= (n - 1) as f64 {
            let dt = self.beats[n - 1] - self.beats[n - 2];
            self.beats[n - 1] + (beat - (n - 1) as f64) * dt
        } else {
            let i = beat.floor() as usize;
            let frac = beat - i as f64;
            let dt = self.beats[i + 1] - self.beats[i];
            self.beats[i] + frac * dt
        };
        Ok(seconds)
    }

    /// Local BPM in the neighbourhood of `seconds`.
    pub fn local_bpm(&self, seconds: f64) -> SaiResult<f64> {
        let c = self.seconds_to_beat(seconds)?;
        let i = (c.beat.round() as isize).clamp(0, self.beats.len() as isize - 1) as usize;
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(self.beats.len() - 1);
        let dt = self.beats[hi] - self.beats[lo];
        if dt > 0.0 {
            Ok(60.0 / dt * (hi - lo) as f64)
        } else {
            Ok(self.median_bpm())
        }
    }

    /// Index of the nearest detected beat to `seconds`, if any.
    pub fn nearest_beat_index(&self, seconds: f64) -> Option<usize> {
        if self.beats.is_empty() {
            return None;
        }
        let mut best = 0usize;
        let mut bd = f64::INFINITY;
        for (i, b) in self.beats.iter().enumerate() {
            let d = (b - seconds).abs();
            if d < bd {
                bd = d;
                best = i;
            }
        }
        Some(best)
    }
}

/// A rationalized metric coordinate with its residual, so quantization is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuantizedBeat {
    /// Reduced rational ticks.
    pub ticks: i64,
    /// Reduced rational subdivision (quarter-note beats).
    pub subdivision: u32,
    /// `beat - ticks/subdivision`: the quantization residual in beats.
    pub residual_beats: f64,
}

impl QuantizedBeat {
    pub fn beats(&self) -> f64 {
        self.ticks as f64 / self.subdivision as f64
    }
}

/// Quantize a beat coordinate to the nearest `1/divisions` grid, returning the residual.
///
/// `divisions` is quarter-note beats per whole beat cell, e.g. 4 = sixteenth-note grid.
pub fn quantize(beat: f64, divisions: u32) -> SaiResult<QuantizedBeat> {
    finite(beat, "beat")?;
    if divisions == 0 {
        return Err(SaiError::Schema("zero quantization divisions".into()));
    }
    let scaled = beat * divisions as f64;
    let ticks = scaled.round() as i64;
    let quantized = ticks as f64 / divisions as f64;
    Ok(QuantizedBeat {
        ticks,
        subdivision: divisions,
        residual_beats: beat - quantized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> BeatGrid {
        // Beats at 0.0, 0.5, 1.0, 1.75 -> a tempo change in the last interval.
        BeatGrid::from_beats(&[0.0, 0.5, 1.0, 1.75], &[0.0, 1.0], 4).unwrap()
    }

    #[test]
    fn interpolation_is_exact_on_beats() {
        let g = grid();
        for (i, b) in g.beat_seconds().iter().enumerate() {
            let c = g.seconds_to_beat(*b).unwrap();
            assert!((c.beat - i as f64).abs() < 1e-12);
            assert_eq!(c.seconds, *b);
        }
    }

    #[test]
    fn round_trip_seconds_beat_seconds() {
        let g = grid();
        for s in [0.0, 0.2, 0.5, 0.9, 1.3, 1.7, 2.2] {
            let b = g.seconds_to_beat(s).unwrap().beat;
            let s2 = g.beat_to_seconds(b).unwrap();
            assert!((s - s2).abs() < 1e-9, "s={s} s2={s2}");
        }
    }

    #[test]
    fn local_tempo_reflects_the_change() {
        let g = grid();
        let a = g.local_bpm(0.25).unwrap();
        let b = g.local_bpm(1.4).unwrap();
        assert!((a - 120.0).abs() < 20.0, "a={a}");
        assert!((b - 120.0).abs() > 20.0 || (b - 80.0).abs() < 20.0, "b={b}");
    }

    #[test]
    fn quantization_reports_residual() {
        let q = quantize(1.13, 4).unwrap();
        assert_eq!(q.ticks, 5); // 1.25
        assert!((q.residual_beats + 0.12).abs() < 1e-9);
        assert!((q.beats() - 1.25).abs() < 1e-12);
        assert!(quantize(1.0, 0).is_err());
    }

    #[test]
    fn empty_grid_refuses() {
        assert!(BeatGrid::from_beats(&[], &[], 4).is_err());
    }
}

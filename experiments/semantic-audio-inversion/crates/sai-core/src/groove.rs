//! Groove inference: kick/snare evidence in beat coordinates, with the metric skeleton kept
//! distinct from microtiming.
//!
//! HumanMusic's dial orders groove relations `Free < PocketSkeleton < KickSnare`. The distinction
//! is exactly the honest one for audio: a quarter-note skeleton is recoverable from noisy onsets
//! far more reliably than every off-beat stroke. This module therefore keeps **two** views of a
//! groove — the full stroke list (microtiming preserved, raw seconds retained) and the
//! quarter-note skeleton — and compares them at the matching resolution.
//!
//! Instrument identity is never asserted: an onset carries *family candidates*
//! (`kick`/`snare`/`hat`/…) with confidences from the measurement route. A stroke records which
//! candidate won and how sure the route was; ties are kept as ambiguity, not averaged away.

use serde::{Deserialize, Serialize};

use crate::error::{finite, SaiResult};
use crate::evidence::OnsetEvidence;
use crate::transport::BeatGrid;

/// A quarter-note-grid tolerance in beats: a stroke this close to an integer beat is "on the
/// beat" for the skeleton view.
pub const SKELETON_TOL_BEATS: f64 = 0.10;

/// Exact relations on a kick/snare part (mirrors HumanMusic's `GrooveRelation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrooveRelation {
    Free,
    /// The kick and snare strokes on the quarter-note beat grid; between beats is free.
    PocketSkeleton,
    /// Every kick and snare stroke at its canonical position.
    KickSnare,
}

impl GrooveRelation {
    pub fn label(self) -> &'static str {
        match self {
            GrooveRelation::Free => "free",
            GrooveRelation::PocketSkeleton => "pocket-skeleton",
            GrooveRelation::KickSnare => "kick-snare",
        }
    }
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "free" => Self::Free,
            "pocket-skeleton" => Self::PocketSkeleton,
            "kick-snare" => Self::KickSnare,
            _ => return None,
        })
    }
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// One inferred percussion stroke. `second` is the authoritative raw coordinate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrooveStroke {
    pub second: f64,
    pub at_beat: f64,
    /// `kick`, `snare`, `hat`, or `unknown`.
    pub voice: String,
    pub confidence: f64,
}

impl GrooveStroke {
    /// True when the stroke lies on the quarter-note grid within tolerance.
    pub fn on_quarter(&self) -> bool {
        (self.at_beat - self.at_beat.round()).abs() <= SKELETON_TOL_BEATS
    }
}

/// The two views of an inferred groove plus the route's own ambiguity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrooveInference {
    pub provenance: crate::evidence::Provenance,
    /// Every inferred stroke, microtiming preserved.
    pub strokes: Vec<GrooveStroke>,
    /// The quarter-note skeleton (a subset of `strokes`).
    pub skeleton: Vec<GrooveStroke>,
    /// Beats per bar used by the transport (1 = unknown meter).
    pub beats_per_bar: u32,
    /// Reasons the ceiling cannot be raised (e.g. only one family observed).
    pub ambiguity: Vec<String>,
}

impl GrooveInference {
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }
}

/// Infer kick/snare strokes from onset evidence.
///
/// Onsets that carry a `kick` or `snare` family candidate above `min_confidence` become strokes.
/// The strongest candidate wins; if two families tie within `TIE_TOL`, the stroke voice is
/// `unknown` and the tie is recorded as ambiguity.
pub fn infer_groove(
    onsets: &[OnsetEvidence],
    grid: &BeatGrid,
    min_confidence: f64,
) -> SaiResult<GrooveInference> {
    let mut strokes = Vec::new();
    let mut ambiguity = Vec::new();
    for o in onsets {
        finite(o.second, "onset.second")?;
        // Only kick/snare are groove evidence; hats, claps and unknown families are not.
        let mut best: Option<(String, f64)> = None;
        let mut second_best = 0.0f64;
        for f in &o.family_candidates {
            let fam = f.family.to_ascii_lowercase();
            if !matches!(fam.as_str(), "kick" | "snare") {
                continue;
            }
            finite(f.confidence, "family.confidence")?;
            if f.confidence < min_confidence {
                continue;
            }
            match &best {
                Some((_, c)) if *c >= f.confidence => {
                    second_best = second_best.max(f.confidence);
                }
                Some((_, c)) => {
                    second_best = second_best.max(*c);
                    best = Some((fam, f.confidence));
                }
                None => best = Some((fam, f.confidence)),
            }
        }
        let Some((voice, conf)) = best else {
            continue;
        };
        let at_beat = grid.seconds_to_beat(o.second)?.beat;
        let tied = (conf - second_best).abs() <= 1e-6 && second_best > 0.0;
        if tied {
            ambiguity.push(format!(
                "onset at {:.3}s: kick/snare candidates tie within 1e-6",
                o.second
            ));
        }
        strokes.push(GrooveStroke {
            second: o.second,
            at_beat,
            voice: if tied { "unknown".into() } else { voice },
            confidence: conf,
        });
    }
    strokes.sort_by(|a, b| a.at_beat.total_cmp(&b.at_beat));
    let skeleton: Vec<GrooveStroke> = strokes.iter().filter(|s| s.on_quarter()).cloned().collect();
    if strokes.is_empty() {
        ambiguity.push("no kick or snare onsets above the confidence floor".into());
    }
    Ok(GrooveInference {
        provenance: onsets
            .first()
            .map(|o| o.provenance.clone())
            .unwrap_or_else(|| {
                crate::evidence::Provenance::derived(
                    "sai-core",
                    env!("CARGO_PKG_VERSION"),
                    "groove-from-onset-families/v1",
                    &["onsets"],
                )
            }),
        strokes,
        skeleton,
        beats_per_bar: grid.beats_per_bar(),
        ambiguity,
    })
}

/// A groove comparison: the declared relation plus the diagnostics behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrooveComparison {
    pub relation: GrooveRelation,
    /// Fraction of the finer pattern matched (greedy, one-to-one, within tolerance).
    pub full_match: f64,
    /// Fraction of the quarter-note skeleton matched.
    pub skeleton_match: f64,
    pub matched_kick: usize,
    pub matched_snare: usize,
}

fn greedy_match(a: &[GrooveStroke], b: &[GrooveStroke], tol: f64, full: bool) -> (usize, usize, usize) {
    // Match same-voice strokes one-to-one on beat distance. Greedy nearest is stable and
    // deterministic given both lists are beat-sorted.
    let mut used = vec![false; b.len()];
    let mut mk = 0usize;
    let mut ms = 0usize;
    for x in a {
        let mut best: Option<(usize, f64)> = None;
        for (j, y) in b.iter().enumerate() {
            if used[j] {
                continue;
            }
            if !voice_compatible(&x.voice, &y.voice) {
                continue;
            }
            if full && !x.on_quarter() && !y.on_quarter() {
                // off-beat strokes must agree in voice exactly
                if x.voice != y.voice {
                    continue;
                }
            }
            let d = (x.at_beat - y.at_beat).abs();
            if d <= tol && best.map_or(true, |(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        if let Some((j, _)) = best {
            used[j] = true;
            if x.voice == "kick" {
                mk += 1;
            } else if x.voice == "snare" {
                ms += 1;
            }
        }
    }
    (mk, ms, used.iter().filter(|u| **u).count())
}

fn voice_compatible(a: &str, b: &str) -> bool {
    a == b || a == "unknown" || b == "unknown"
}

/// Compare two inferred grooves under the declared relations.
///
/// - `KickSnare`: the full stroke patterns match within `tol` (default [`crate::metrics`]-level
///   1/16 grid) with identical voice assignments;
/// - `PocketSkeleton`: the quarter-note skeletons match within `SKELETON_TOL_BEATS`;
/// - `Free`: otherwise.
pub fn compare_groove(a: &GrooveInference, b: &GrooveInference, tol: f64) -> GrooveComparison {
    let (mk, ms, matched) = greedy_match(&a.strokes, &b.strokes, tol, true);
    let full_den = a.strokes.len().max(b.strokes.len()).max(1);
    let full_match = matched as f64 / full_den as f64;

    let (sk, ss, skel_matched) =
        greedy_match(&a.skeleton, &b.skeleton, SKELETON_TOL_BEATS, false);
    let skel_den = a.skeleton.len().max(b.skeleton.len()).max(1);
    let skeleton_match = skel_matched as f64 / skel_den as f64;

    let relation = if a.is_empty() || b.is_empty() {
        GrooveRelation::Free
    } else if full_match >= 1.0 - 1e-9 {
        GrooveRelation::KickSnare
    } else if skeleton_match >= 1.0 - 1e-9 {
        GrooveRelation::PocketSkeleton
    } else {
        GrooveRelation::Free
    };
    let _ = (sk, ss);
    GrooveComparison {
        relation,
        full_match,
        skeleton_match,
        matched_kick: mk,
        matched_snare: ms,
    }
}

/// Convert a groove inference to the compact stroke list carried by a quotient.
pub fn strokes_of(g: &GrooveInference) -> Vec<GrooveStroke> {
    g.strokes.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{FamilyCandidate, OnsetEvidence, Provenance};

    fn onset(sec: f64, fam: &str, conf: f64) -> OnsetEvidence {
        OnsetEvidence {
            second: sec,
            strength: 0.8,
            family_candidates: vec![FamilyCandidate {
                family: fam.into(),
                confidence: conf,
            }],
            provenance: Provenance::derived("test", "1", "onset/v1", &["pcm"]),
        }
    }

    fn grid4() -> BeatGrid {
        BeatGrid::from_beats(&[0.0, 0.5, 1.0, 1.5, 2.0, 2.5], &[0.0], 4).unwrap()
    }

    #[test]
    fn skeleton_keeps_quarter_strokes_only() {
        let o = vec![onset(0.0, "kick", 0.9), onset(0.25, "hat", 0.9), onset(0.5, "snare", 0.9)];
        let g = infer_groove(&o, &grid4(), 0.5).unwrap();
        assert_eq!(g.strokes.len(), 2); // hat ignored
        assert_eq!(g.skeleton.len(), 2);
        assert!(g.strokes.iter().all(|s| s.on_quarter()));
    }

    #[test]
    fn offbeat_stroke_is_kick_snare_not_skeleton() {
        let a = infer_groove(&[onset(0.0, "kick", 0.9), onset(0.25, "snare", 0.9)], &grid4(), 0.5)
            .unwrap();
        let b = infer_groove(&[onset(0.0, "kick", 0.9), onset(0.25, "snare", 0.9)], &grid4(), 0.5)
            .unwrap();
        // Same off-beat pattern -> full match -> KickSnare.
        assert_eq!(compare_groove(&a, &b, 0.05).relation, GrooveRelation::KickSnare);
        // A different off-beat placement keeps only the quarter skeleton.
        let c = infer_groove(&[onset(0.0, "kick", 0.9), onset(0.30, "snare", 0.9)], &grid4(), 0.5)
            .unwrap();
        assert_eq!(compare_groove(&a, &c, 0.01).relation, GrooveRelation::PocketSkeleton);
    }

    #[test]
    fn empty_groove_is_free() {
        let a = infer_groove(&[], &grid4(), 0.5).unwrap();
        let b = infer_groove(&[onset(0.0, "kick", 0.9)], &grid4(), 0.5).unwrap();
        assert_eq!(compare_groove(&a, &b, 0.05).relation, GrooveRelation::Free);
    }

    #[test]
    fn tied_families_become_unknown_not_a_guess() {
        let o = OnsetEvidence {
            second: 0.0,
            strength: 1.0,
            family_candidates: vec![
                FamilyCandidate { family: "kick".into(), confidence: 0.7 },
                FamilyCandidate { family: "snare".into(), confidence: 0.7 },
            ],
            provenance: Provenance::derived("test", "1", "onset/v1", &["pcm"]),
        };
        let g = infer_groove(&[o], &grid4(), 0.5).unwrap();
        assert_eq!(g.strokes[0].voice, "unknown");
        assert!(!g.ambiguity.is_empty());
    }
}

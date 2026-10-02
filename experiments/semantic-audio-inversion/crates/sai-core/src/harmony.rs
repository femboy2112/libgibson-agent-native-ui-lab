//! Harmony by triangulation: two independent conceptual routes, never a majority vote.
//!
//! Route A reads chord candidates from **chroma/HPCP** evidence (an acoustic route).
//! Route B reads chord candidates from **transcribed sounding notes** and windows their
//! pitch classes (a symbolic route). The two routes have different failure modes; agreement
//! corroborates, disagreement is evidence about ambiguity, nonharmonic tones, inversion or a
//! model failure. The reconciler reports rivals and the strongest *justified* relation
//! (`Free < Ordered < QualityFamily < Exact`), never a forced Exact.

use serde::{Deserialize, Serialize};

use crate::error::{finite, SaiError, SaiResult};
use crate::evidence::{ChordCandidate, NoteEvidence};
use crate::transport::BeatGrid;

/// Chord quality families, used to lower `Exact` to `QualityFamily` honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QualityFamily {
    Major,
    Minor,
    Diminished,
    Augmented,
    Suspended,
    Unknown,
}

impl QualityFamily {
    pub fn of(quality: &str) -> QualityFamily {
        match quality {
            "maj" | "maj7" | "dom7" | "6" => QualityFamily::Major,
            "min" | "min7" | "min7b5" | "min6" | "moll" => QualityFamily::Minor,
            "dim" | "dim7" => QualityFamily::Diminished,
            "aug" => QualityFamily::Augmented,
            "sus" | "sus2" | "sus4" => QualityFamily::Suspended,
            _ => QualityFamily::Unknown,
        }
    }
}

/// A chord label: root pitch class plus a quality string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordLabel {
    pub root_pc: i32,
    pub quality: String,
}

impl ChordLabel {
    pub fn family(&self) -> QualityFamily {
        QualityFamily::of(&self.quality)
    }
    pub fn label(&self) -> String {
        format!("{}/{}", self.root_pc, self.quality)
    }
}

/// Which measurement route produced a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarmonyRoute {
    /// Route A: chroma/HPCP template matching.
    AcousticChroma,
    /// Route B: transcribed-note pitch-class windows.
    NoteWindow,
}

/// A detected chord span in beat-domain coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChordSpan {
    pub at_beat: f64,
    pub end_beat: f64,
    pub label: ChordLabel,
    pub confidence: f64,
    pub route: HarmonyRoute,
    /// `root`, `first`, `second`, `unknown`.
    pub inversion: String,
}

/// The exact relations a harmony pair can hold (mirrors HumanMusic's fidelity dial).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarmonyRelation {
    Free,
    /// Relative roots and qualities in order, durations ignored.
    Ordered,
    /// Exact spans and relative roots; quality up to its triad family.
    QualityFamily,
    /// Exact spans, relative roots and qualities.
    Exact,
}

impl HarmonyRelation {
    pub fn label(self) -> &'static str {
        match self {
            HarmonyRelation::Free => "free",
            HarmonyRelation::Ordered => "ordered",
            HarmonyRelation::QualityFamily => "quality-family",
            HarmonyRelation::Exact => "exact",
        }
    }
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "free" => Self::Free,
            "ordered" => Self::Ordered,
            "quality-family" => Self::QualityFamily,
            "exact" => Self::Exact,
            _ => return None,
        })
    }
}

/// A reconciled harmonic span holding any live rivals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciledSpan {
    pub at_beat: f64,
    pub end_beat: f64,
    /// The selected hypothesis (may itself be `unknown`).
    pub selected: ChordLabel,
    pub confidence: f64,
    /// Live rival labels that the evidence does not separate.
    pub rivals: Vec<ChordLabel>,
    /// The routes that contributed.
    pub routes: Vec<HarmonyRoute>,
}

/// The result of reconciling two routes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reconciliation {
    pub spans: Vec<ReconciledSpan>,
    /// The strongest relation the agreement supports.
    pub ceiling: HarmonyRelation,
    /// Reasons the ceiling was lowered.
    pub lowering: Vec<String>,
    /// Windows where the routes disagreed and no discriminator resolved them.
    pub disputes: usize,
}

// --- Route A: acoustic chroma ---

const TEMPLATES: &[(&str, [u8; 4])] = &[
    // (quality, [semitone offsets of a 4-slot template; 255 = unused])
    ("maj", [0, 4, 7, 255]),
    ("min", [0, 3, 7, 255]),
    ("dim", [0, 3, 6, 255]),
    ("aug", [0, 4, 8, 255]),
    ("dom7", [0, 4, 7, 10]),
    ("maj7", [0, 4, 7, 11]),
    ("min7", [0, 3, 7, 10]),
    ("min7b5", [0, 3, 6, 10]),
];

/// Score a 12-bin chroma vector against the fixed template table.
///
/// Returns `(root_pc, quality, score, inversion, rivals)`. Score is normalized coverage in
/// `[0,1]`. Rivals are templates within 90% of the best score (kept, never averaged).
pub fn match_template(chroma: &[f64; 12]) -> (i32, String, f64, String, Vec<ChordLabel>) {
    let total: f64 = chroma.iter().sum();
    if total <= 1e-12 {
        return (0, "unknown".into(), 0.0, "unknown".into(), vec![]);
    }
    let norm: [f64; 12] = std::array::from_fn(|i| chroma[i] / total);
    let mut scored: Vec<(f64, i32, &str, [u8; 4])> = Vec::new();
    for root in 0..12i32 {
        for (quality, tmpl) in TEMPLATES {
            let mut covered = 0.0;
            for &off in tmpl.iter().filter(|o| **o != 255) {
                covered += norm[(root + off as i32).rem_euclid(12) as usize];
            }
            scored.push((covered, root, quality, *tmpl));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best_score, best_root, best_quality, _) = scored[0];
    // Bass/inversion: strongest single pitch class in the chroma. A tie at the maximum does not
    // identify a bass, so the inversion is honestly `unknown` rather than an arbitrary pick.
    let max_v = norm.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let tied: Vec<i32> = (0..12)
        .filter(|&i| (norm[i as usize] - max_v).abs() < 1e-9)
        .collect();
    let bass_pc = tied[0];
    let inversion = if tied.len() > 1 {
        "unknown"
    } else if bass_pc == best_root {
        "root"
    } else if (bass_pc - best_root).rem_euclid(12) == 4
        || (bass_pc - best_root).rem_euclid(12) == 3
    {
        "first"
    } else {
        "second"
    };
    let rivals: Vec<ChordLabel> = scored
        .iter()
        .filter(|(s, r, q, _)| *s >= best_score * 0.9 - 1e-9 && !(*r == best_root && *q == best_quality))
        .take(3)
        .map(|(_, r, q, _)| ChordLabel { root_pc: *r, quality: (*q).to_string() })
        .collect();
    (
        best_root,
        best_quality.to_string(),
        best_score.clamp(0.0, 1.0),
        inversion.to_string(),
        rivals,
    )
}

/// Route A: average chroma frames per window and match templates.
pub fn chords_from_chroma(
    tonal: &crate::evidence::TonalEvidence,
    grid: &BeatGrid,
    window_beats: f64,
    total_beats: f64,
) -> SaiResult<Vec<ChordSpan>> {
    if tonal.chroma_frames.is_empty() {
        return Err(SaiError::NoEvidence("chroma frames"));
    }
    let mut out = Vec::new();
    let mut at = 0.0f64;
    while at < total_beats - 1e-9 {
        let end = (at + window_beats).min(total_beats);
        let (s0, s1) = (grid.beat_to_seconds(at)?, grid.beat_to_seconds(end)?);
        let mut acc = [0.0f64; 12];
        let mut n = 0usize;
        for f in &tonal.chroma_frames {
            if f.center_second >= s0 - 1e-9 && f.center_second < s1 - 1e-9 {
                for i in 0..12 {
                    acc[i] += f.values[i];
                }
                n += 1;
            }
        }
        if n > 0 {
            let (root, quality, score, inversion, _) = match_template(&acc);
            if score > 0.0 {
                out.push(ChordSpan {
                    at_beat: at,
                    end_beat: end,
                    label: ChordLabel { root_pc: root, quality },
                    confidence: score,
                    route: HarmonyRoute::AcousticChroma,
                    inversion,
                });
            }
        }
        at = end;
    }
    Ok(out)
}

/// Route B: window transcribed notes into pitch-class vectors and match templates.
pub fn chords_from_notes(
    notes: &[NoteEvidence],
    grid: &BeatGrid,
    window_beats: f64,
    total_beats: f64,
) -> SaiResult<Vec<ChordSpan>> {
    if notes.is_empty() {
        return Err(SaiError::NoEvidence("transcribed notes for note-window harmony"));
    }
    // Convert notes to beat spans.
    let mut spans: Vec<(f64, f64, i32, f64)> = Vec::new();
    for n in notes {
        let on = grid.seconds_to_beat(n.onset_second)?.beat;
        let off = match n.offset_second {
            Some(o) => grid.seconds_to_beat(o)?.beat,
            None => on + 0.5,
        };
        spans.push((on, off.max(on + 1e-6), n.pitch_midi.round() as i32, n.confidence));
    }
    let mut out = Vec::new();
    let mut at = 0.0f64;
    while at < total_beats - 1e-9 {
        let end = (at + window_beats).min(total_beats);
        let mut chroma = [0.0f64; 12];
        for (on, off, pitch, conf) in &spans {
            if *on < end - 1e-9 && *off > at + 1e-9 {
                chroma[pitch.rem_euclid(12) as usize] += conf;
            }
        }
        if chroma.iter().sum::<f64>() > 1e-9 {
            let (root, quality, score, inversion, _) = match_template(&chroma);
            if score > 0.0 {
                out.push(ChordSpan {
                    at_beat: at,
                    end_beat: end,
                    label: ChordLabel { root_pc: root, quality },
                    confidence: score,
                    route: HarmonyRoute::NoteWindow,
                    inversion,
                });
            }
        }
        at = end;
    }
    Ok(out)
}

// --- reconciliation ---

/// Reconcile two route outputs window by window. Never majority-votes; keeps rivals.
pub fn reconcile(a: &[ChordSpan], b: &[ChordSpan]) -> Reconciliation {
    use std::collections::BTreeMap;
    let mut by_start: BTreeMap<i64, Vec<&ChordSpan>> = BTreeMap::new();
    for s in a.iter().chain(b.iter()) {
        by_start.entry((s.at_beat * 1000.0).round() as i64).or_default().push(s);
    }
    let mut spans = Vec::new();
    let mut disputes = 0usize;
    let mut lowering = Vec::new();
    for (_, group) in by_start {
        let at = group.iter().map(|s| s.at_beat).fold(f64::INFINITY, f64::min);
        let end = group.iter().map(|s| s.end_beat).fold(f64::NEG_INFINITY, f64::max);
        let routes: Vec<HarmonyRoute> = {
            let mut v: Vec<_> = group.iter().map(|s| s.route).collect();
            v.sort_by_key(|r| *r as u8);
            v.dedup();
            v
        };
        // Distinct labels among the group.
        let mut labels: Vec<&ChordLabel> = group.iter().map(|s| &s.label).collect();
        labels.sort_by_key(|l| (l.root_pc, l.quality.clone()));
        labels.dedup();
        let conf = group.iter().map(|s| s.confidence).fold(0.0f64, f64::max);
        if labels.len() == 1 {
            spans.push(ReconciledSpan {
                at_beat: at,
                end_beat: end,
                selected: labels[0].clone(),
                confidence: conf,
                rivals: vec![],
                routes,
            });
        } else {
            // Keep the highest-confidence label as selected, list the rest as rivals.
            disputes += 1;
            let mut ordered: Vec<&&ChordSpan> = group.iter().collect();
            ordered.sort_by(|x, y| y.confidence.total_cmp(&x.confidence));
            let selected = ordered[0].label.clone();
            let rivals: Vec<ChordLabel> = ordered
                .iter()
                .skip(1)
                .map(|s| s.label.clone())
                .collect();
            spans.push(ReconciledSpan {
                at_beat: at,
                end_beat: end,
                selected,
                confidence: conf * 0.5,
                rivals,
                routes,
            });
        }
    }
    // Ceiling: both routes at the same window, labels agreeing => Exact; family agreement =>
    // QualityFamily; ordered agreement => Ordered; otherwise Free.
    let ceiling = if a.is_empty() || b.is_empty() {
        lowering.push("only one harmonic route produced evidence".into());
        HarmonyRelation::Ordered
    } else if disputes == 0 {
        HarmonyRelation::Exact
    } else {
        lowering.push(format!("{disputes} window(s) where the two routes disagree"));
        HarmonyRelation::QualityFamily
    };
    Reconciliation {
        spans,
        ceiling,
        lowering,
        disputes,
    }
}

/// Compare two *ordered* chord sequences and return the strongest justified relation.
pub fn relation_between(a: &[ChordLabel], b: &[ChordLabel]) -> HarmonyRelation {
    if a.is_empty() || b.is_empty() {
        return HarmonyRelation::Free;
    }
    let n = a.len().min(b.len());
    let roots_equal = (0..n).all(|i| a[i].root_pc == b[i].root_pc);
    let quality_equal = (0..n).all(|i| a[i].quality == b[i].quality);
    let family_equal = (0..n).all(|i| a[i].family() == b[i].family());
    let length_equal = a.len() == b.len();
    if length_equal && roots_equal && quality_equal {
        HarmonyRelation::Exact
    } else if length_equal && roots_equal && family_equal {
        HarmonyRelation::QualityFamily
    } else if length_equal && roots_equal {
        HarmonyRelation::Ordered
    } else {
        HarmonyRelation::Free
    }
}

/// Convenience: reduce a set of reconciled spans to the ordered selected label sequence.
pub fn selected_sequence(spans: &[ReconciledSpan]) -> Vec<ChordLabel> {
    spans
        .iter()
        .filter(|s| s.selected.quality != "unknown")
        .map(|s| s.selected.clone())
        .collect()
}

/// Validate a chord candidate from the wire, failing closed on bad domains.
pub fn validate_candidate(c: &ChordCandidate) -> SaiResult<()> {
    finite(c.at_second, "chord.at")?;
    finite(c.end_second, "chord.end")?;
    finite(c.confidence, "chord.confidence")?;
    if !(0..12).contains(&c.root_pc) {
        return Err(SaiError::Schema("chord root out of range".into()));
    }
    if c.end_second < c.at_second {
        return Err(SaiError::Schema("chord end before start".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(at: f64, end: f64, root: i32, q: &str, route: HarmonyRoute) -> ChordSpan {
        ChordSpan {
            at_beat: at,
            end_beat: end,
            label: ChordLabel { root_pc: root, quality: q.into() },
            confidence: 0.8,
            route,
            inversion: "root".into(),
        }
    }

    #[test]
    fn template_recovers_a_major_triad() {
        let mut c = [0.0; 12];
        // A clear bass on the root disambiguates the inversion.
        c[0] = 1.2;
        c[4] = 1.0;
        c[7] = 1.0;
        let (root, q, score, inv, _) = match_template(&c);
        assert_eq!((root, q.as_str()), (0, "maj"));
        assert!(score > 0.9);
        assert_eq!(inv, "root");
    }

    #[test]
    fn tied_bass_yields_unknown_inversion_not_a_guess() {
        let mut c = [0.0; 12];
        c[0] = 1.0;
        c[4] = 1.0;
        c[7] = 1.0;
        let (root, q, _score, inv, _) = match_template(&c);
        assert_eq!((root, q.as_str()), (0, "maj"));
        assert_eq!(inv, "unknown");
    }

    #[test]
    fn routes_agreeing_give_exact_ceiling() {
        let a = vec![span(0.0, 2.0, 0, "maj", HarmonyRoute::AcousticChroma)];
        let b = vec![span(0.0, 2.0, 0, "maj", HarmonyRoute::NoteWindow)];
        let r = reconcile(&a, &b);
        assert_eq!(r.ceiling, HarmonyRelation::Exact);
        assert!(r.spans[0].rivals.is_empty());
    }

    #[test]
    fn routes_disagreeing_keep_rivals_and_lower_ceiling() {
        let a = vec![span(0.0, 2.0, 0, "maj", HarmonyRoute::AcousticChroma)];
        let b = vec![span(0.0, 2.0, 7, "min", HarmonyRoute::NoteWindow)];
        let r = reconcile(&a, &b);
        assert_eq!(r.ceiling, HarmonyRelation::QualityFamily);
        assert_eq!(r.spans[0].rivals.len(), 1);
    }

    #[test]
    fn relation_lowers_to_family_when_quality_differs() {
        let a = vec![ChordLabel { root_pc: 0, quality: "maj".into() }];
        let b = vec![ChordLabel { root_pc: 0, quality: "maj7".into() }];
        assert_eq!(relation_between(&a, &b), HarmonyRelation::QualityFamily);
    }

    #[test]
    fn relation_is_free_when_roots_differ() {
        let a = vec![ChordLabel { root_pc: 0, quality: "maj".into() }];
        let b = vec![ChordLabel { root_pc: 5, quality: "maj".into() }];
        assert_eq!(relation_between(&a, &b), HarmonyRelation::Free);
    }
}

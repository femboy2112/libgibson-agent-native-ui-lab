//! Motif / lick inference under **explicit musical transformations before embeddings**.
//!
//! There is no embedding threshold and no "do these sound similar?" model call. A motif
//! statement is a concrete `(pitch, onset, duration)` sequence; its identity is the
//! transposition- and tempo-normalized structure HumanMusic's `motif.rs` also uses:
//!
//! - **interval contour** — successive semitone differences (transposition-invariant);
//! - **direction signature** — the bare up/down/same gesture;
//! - **proportional rhythm** — durations normalized to sum to one (augmentation-invariant);
//! - **normalized onset profile** — inter-onset positions as fractions of the span
//!   (tempo-scaling invariant).
//!
//! Relations are declared, not fitted. [`MotifRelation`] is checked with fixed tolerances:
//! a stronger relation is never claimed when the evidence supports only a weaker one. The
//! exact notes are retained alongside the quotient identity so the quotient can be audited.

use serde::{Deserialize, Serialize};

use crate::error::{finite, SaiError, SaiResult};

/// Declared pitch tolerance in semitones. A recovered pitch is treated as equal to a hidden
/// pitch within this distance. Frozen before holdout contact (see `docs/THRESHOLDS.md`).
pub const PITCH_TOL_SEMITONES: f64 = 0.5;
/// Declared onset tolerance as a fraction of the span (normalized inter-onset).
pub const ONSET_TOL_FRACTION: f64 = 0.04;
/// Declared proportional-rhythm L-infinity tolerance.
pub const RHYTHM_TOL: f64 = 0.12;
/// Similarity above which two statements are clustered into one family (diagnostic only; the
/// relation label, not this number, is what the quotient consumes).
pub const FAMILY_SIMILARITY: f64 = 0.80;

/// A concrete motif statement in beat-domain coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotifStatement {
    /// Absolute MIDI pitches (float, so fractional estimates are representable).
    pub pitches: Vec<f64>,
    /// Onset positions in beats (authoritative seconds are retained by the caller).
    pub onsets_beats: Vec<f64>,
    /// Durations in beats.
    pub durations_beats: Vec<f64>,
}

impl MotifStatement {
    /// Build from parallel slices, failing closed on mismatched lengths or non-finite values.
    pub fn new(
        pitches: Vec<f64>,
        onsets_beats: Vec<f64>,
        durations_beats: Vec<f64>,
    ) -> SaiResult<Self> {
        let n = pitches.len();
        if n == 0 {
            return Err(SaiError::NoEvidence("motif statement has no notes"));
        }
        if onsets_beats.len() != n || durations_beats.len() != n {
            return Err(SaiError::Schema("motif slices have different lengths".into()));
        }
        for p in &pitches {
            finite(*p, "motif.pitch")?;
        }
        for o in &onsets_beats {
            finite(*o, "motif.onset")?;
        }
        for d in &durations_beats {
            finite(*d, "motif.duration")?;
            if *d <= 0.0 {
                return Err(SaiError::NonFinite("motif duration must be positive"));
            }
        }
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|a, b| onsets_beats[*a].total_cmp(&onsets_beats[*b]));
        let mut sorted = Self {
            pitches: idx.iter().map(|&i| pitches[i]).collect(),
            onsets_beats: idx.iter().map(|&i| onsets_beats[i]).collect(),
            durations_beats: idx.iter().map(|&i| durations_beats[i]).collect(),
        };
        // Reject exact duplicate onsets: a motif is a monophonic-ish line, not a chord.
        if sorted.onsets_beats.windows(2).any(|w| w[1] - w[0] < 1e-9) {
            // Not fatal: collapse to the lower onset's first note, but flag by refusing here so
            // the caller decides (chords belong to the harmony route).
            return Err(SaiError::Ambiguous {
                what: "motif onset collision (polyphonic window)",
                rivals: 2,
            });
        }
        sorted.onsets_beats.shrink_to_fit();
        Ok(sorted)
    }

    pub fn len(&self) -> usize {
        self.pitches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pitches.is_empty()
    }

    /// Span in beats from first onset to last onset (0 for a single note).
    pub fn span_beats(&self) -> f64 {
        self.onsets_beats
            .last()
            .copied()
            .unwrap_or(0.0)
            - self.onsets_beats.first().copied().unwrap_or(0.0)
    }

    /// Semitone differences between successive pitches.
    pub fn interval_contour(&self) -> Vec<f64> {
        self.pitches.windows(2).map(|w| w[1] - w[0]).collect()
    }

    /// Sign of each interval: -1, 0, +1.
    pub fn direction_signature(&self) -> Vec<i8> {
        self.interval_contour()
            .iter()
            .map(|d| {
                if d.abs() < 1e-9 {
                    0
                } else if *d > 0.0 {
                    1
                } else {
                    -1
                }
            })
            .collect()
    }

    /// Durations normalized to sum to one (augmentation/diminution invariant).
    pub fn rhythmic_profile(&self) -> Vec<f64> {
        let total: f64 = self.durations_beats.iter().sum();
        if total > 1e-12 {
            self.durations_beats.iter().map(|d| d / total).collect()
        } else {
            let even = 1.0 / self.len() as f64;
            vec![even; self.len()]
        }
    }

    /// Inter-onset positions normalized to span (tempo-scaling invariant). Empty for one note.
    pub fn onset_profile(&self) -> Vec<f64> {
        let span = self.span_beats();
        if self.len() < 2 {
            return Vec::new();
        }
        let base = self.onsets_beats[0];
        if span <= 1e-12 {
            return Vec::new();
        }
        self.onsets_beats
            .iter()
            .map(|o| (o - base) / span)
            .collect()
    }

    /// Pitches relative to the first note (transposition-invariant, including octave).
    pub fn relative_pitches(&self) -> Vec<f64> {
        let base = self.pitches[0];
        self.pitches.iter().map(|p| p - base).collect()
    }

    /// Tonic-relative chromatic pitches normalized by one global octave, matching the
    /// HumanMusic cover-line relation: subtract the first note rounded down to an octave.
    pub fn octave_normalized_pitches(&self, tonic: f64) -> Vec<f64> {
        let first = self.pitches[0] - tonic;
        let origin = first - first.rem_euclid(12.0);
        self.pitches.iter().map(|p| p - tonic - origin).collect()
    }

    // --- declared transformations (identity-preserving) ---

    /// Transpose every pitch by `semitones`.
    pub fn transpose(&self, semitones: f64) -> Self {
        Self {
            pitches: self.pitches.iter().map(|p| p + semitones).collect(),
            ..self.clone()
        }
    }

    /// Invert the contour about the first pitch.
    pub fn invert(&self) -> Self {
        let pivot = self.pitches[0];
        Self {
            pitches: self.pitches.iter().map(|p| 2.0 * pivot - p).collect(),
            ..self.clone()
        }
    }

    /// Reverse both contour and rhythm.
    pub fn retrograde(&self) -> Self {
        let mut s = self.clone();
        s.pitches.reverse();
        s.durations_beats.reverse();
        // Onsets stay sorted: mirror within the span.
        let span = self.span_beats();
        let base = self.onsets_beats[0];
        let n = self.len();
        s.onsets_beats = (0..n)
            .map(|i| base + span - (self.onsets_beats[n - 1 - i] - base))
            .collect();
        s
    }

    /// Scale all durations and inter-onset gaps by `factor` (augmentation > 1).
    pub fn scale_rhythm(&self, factor: f64) -> SaiResult<Self> {
        finite(factor, "scale factor")?;
        if factor <= 0.0 {
            return Err(SaiError::NonFinite("non-positive rhythm scale"));
        }
        let base = self.onsets_beats[0];
        Ok(Self {
            pitches: self.pitches.clone(),
            onsets_beats: self
                .onsets_beats
                .iter()
                .map(|o| base + (o - base) * factor)
                .collect(),
            durations_beats: self.durations_beats.iter().map(|d| d * factor).collect(),
        })
    }

    /// First `take` notes (a fragment / question).
    pub fn fragment(&self, take: usize) -> SaiResult<Self> {
        let n = take.clamp(1, self.len());
        Self::new(
            self.pitches[..n].to_vec(),
            self.onsets_beats[..n].to_vec(),
            self.durations_beats[..n].to_vec(),
        )
    }
}

/// The exact relations a motif pair can hold, weakest to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotifRelation {
    /// No declared relation.
    Free,
    /// Same gesture (direction signature) and same onset profile; pitch contour may differ.
    Theme,
    /// Exact transposition- and octave-relative pitches at matching normalized onsets; durations free.
    Metric,
    /// [`Self::Metric`] plus every proportional duration.
    Faithful,
}

impl MotifRelation {
    pub fn label(self) -> &'static str {
        match self {
            MotifRelation::Free => "free",
            MotifRelation::Theme => "theme",
            MotifRelation::Metric => "metric",
            MotifRelation::Faithful => "faithful",
        }
    }
    /// The stronger of two relations.
    pub fn max(self, other: Self) -> Self {
        Ord::max(self, other)
    }
    /// Parse a relation label (used by the evaluator to read hidden truth).
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "free" => Self::Free,
            "theme" => Self::Theme,
            "metric" => Self::Metric,
            "faithful" => Self::Faithful,
            _ => return None,
        })
    }
}

/// The result of comparing two statements: a declared relation plus the diagnostics behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotifComparison {
    pub relation: MotifRelation,
    /// Bounded diagnostic similarity in `[0,1]` (never the identity authority).
    pub similarity: f64,
    pub length_equal: bool,
    pub contour_equal: bool,
    pub direction_equal: bool,
    pub onsets_equal: bool,
    pub rhythm_equal: bool,
    /// Competing relations when the tolerance boundary is tight; empty when unambiguous.
    pub rivals: Vec<MotifRelation>,
}

fn close_vec(a: &[f64], b: &[f64], tol: f64) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

/// Compare two statements under the declared relation tolerances.
pub fn compare(a: &MotifStatement, b: &MotifStatement) -> MotifComparison {
    let length_equal = a.len() == b.len();
    let contour_equal = length_equal && close_vec(&a.relative_pitches(), &b.relative_pitches(), PITCH_TOL_SEMITONES);
    let direction_equal = a.direction_signature() == b.direction_signature();
    let onsets_equal = length_equal
        && close_vec(&a.onset_profile(), &b.onset_profile(), ONSET_TOL_FRACTION);
    let rhythm_equal = length_equal && close_vec(&a.rhythmic_profile(), &b.rhythmic_profile(), RHYTHM_TOL);

    let mut rivals = Vec::new();
    let relation = if contour_equal && onsets_equal {
        if rhythm_equal {
            MotifRelation::Faithful
        } else {
            rivals.push(MotifRelation::Faithful);
            MotifRelation::Metric
        }
    } else if contour_equal && !onsets_equal {
        rivals.push(MotifRelation::Metric);
        MotifRelation::Theme
    } else if direction_equal && onsets_equal && !length_equal {
        MotifRelation::Theme
    } else {
        MotifRelation::Free
    };
    MotifComparison {
        relation,
        similarity: similarity(a, b),
        length_equal,
        contour_equal,
        direction_equal,
        onsets_equal,
        rhythm_equal,
        rivals,
    }
}

/// Bounded diagnostic relatedness, mirroring HumanMusic's declared weighting. This is **not**
/// the identity relation; it exists to rank candidate families.
pub fn similarity(a: &MotifStatement, b: &MotifStatement) -> f64 {
    const W_INTERVAL: f64 = 0.55;
    const W_DIRECTION: f64 = 0.25;
    const W_RHYTHM: f64 = 0.20;
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let ca = a.interval_contour();
    let cb = b.interval_contour();
    let m = ca.len().min(cb.len());
    let interval_sim = if m == 0 {
        1.0
    } else {
        let mut acc = 0.0;
        for i in 0..m {
            acc += 1.0 - ((ca[i] - cb[i]).abs() / 12.0).min(1.0);
        }
        acc / m as f64
    };
    let da = a.direction_signature();
    let db = b.direction_signature();
    let dm = da.len().min(db.len());
    let dir_sim = if dm == 0 {
        1.0
    } else {
        let mut acc = 0.0;
        for i in 0..dm {
            if da[i] == db[i] {
                acc += 1.0;
            }
        }
        acc / dm as f64
    };
    let ra = a.rhythmic_profile();
    let rb = b.rhythmic_profile();
    let rm = ra.len().min(rb.len());
    let rhythm_sim = if rm == 0 {
        1.0
    } else {
        let l1: f64 = (0..rm).map(|i| (ra[i] - rb[i]).abs()).sum();
        (1.0 - l1 / 2.0).clamp(0.0, 1.0)
    };
    let base = W_INTERVAL * interval_sim + W_DIRECTION * dir_sim + W_RHYTHM * rhythm_sim;
    let penalty = n as f64 / a.len().max(b.len()) as f64;
    (base * penalty).clamp(0.0, 1.0)
}

/// A motif family: a representative plus variants under declared transformations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotifFamily {
    /// The representative statement (the earliest, longest candidate).
    pub representative: MotifStatement,
    /// Variant indices into the caller's candidate list, with the relation and transform label.
    pub variants: Vec<MotifVariant>,
    /// Whether the family's members are supported by more than one candidate window.
    pub support: usize,
}

/// One statement's membership in a family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotifVariant {
    pub candidate_index: usize,
    pub relation: MotifRelation,
    pub similarity: f64,
    /// The declared transformation that aligns it to the representative.
    pub transform: String,
}

/// A candidate window over a note stream, in raw seconds (authoritative) and beats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateWindow {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub statement: MotifStatement,
}

/// Cluster candidate windows into families under the declared transformations.
///
/// The cluster criterion is a declared similarity floor; the *relation* recorded per variant
/// is the declared [`MotifRelation`], so the quotient can lower the ceiling honestly.
pub fn families(candidates: &[CandidateWindow], transform_tolerance: f64) -> Vec<MotifFamily> {
    let n = candidates.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    let mut best_align: Vec<Option<(MotifRelation, f64, String)>> = vec![None; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &candidates[i].statement;
            let b = &candidates[j].statement;
            // Try the declared transform set; pick the strongest relation found.
            let mut best: Option<(MotifRelation, f64, String)> = None;
            let mut consider = |s: &MotifStatement, label: &str| {
                let c = compare(a, s);
                if c.similarity >= FAMILY_SIMILARITY - transform_tolerance {
                    let entry = (c.relation, c.similarity, label.to_string());
                    if best.as_ref().is_none_or(|e| e.0 < entry.0 || (e.0 == entry.0 && e.1 < entry.1)) {
                        best = Some(entry);
                    }
                }
            };
            consider(b, "identity");
            consider(&b.transpose(12.0), "transposition");
            consider(&b.invert(), "inversion");
            consider(&b.retrograde(), "retrograde");
            if let Ok(d) = b.scale_rhythm(2.0) {
                consider(&d, "augmentation");
            }
            if let Ok(d) = b.scale_rhythm(0.5) {
                consider(&d, "diminution");
            }
            if let Some((rel, sim, label)) = best {
                let (ra, rb) = (find(&mut parent, i), find(&mut parent, j));
                if ra != rb {
                    parent[ra] = rb;
                }
                if best_align[j].as_ref().is_none_or(|e| e.0 < rel || (e.0 == rel && e.1 < sim)) {
                    best_align[j] = Some((rel, sim, label));
                }
            }
        }
    }
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        groups.entry(find(&mut parent, i)).or_default().push(i);
    }
    let mut out = Vec::new();
    for (_, members) in groups {
        // Representative: longest, then earliest.
        let rep_idx = *members
            .iter()
            .max_by(|x, y| {
                let ax = &candidates[**x];
                let ay = &candidates[**y];
                ax.statement
                    .len()
                    .cmp(&ay.statement.len())
                    .then(ax.start_seconds.total_cmp(&ay.start_seconds).reverse())
            })
            .unwrap();
        let rep = candidates[rep_idx].statement.clone();
        let variants = members
            .iter()
            .filter(|&&m| m != rep_idx)
            .map(|&m| {
                let c = compare(&rep, &candidates[m].statement);
                let (rel, sim, transform) = best_align[m]
                    .clone()
                    .unwrap_or((c.relation, c.similarity, "identity".into()));
                MotifVariant {
                    candidate_index: m,
                    relation: rel,
                    similarity: sim,
                    transform,
                }
            })
            .collect();
        out.push(MotifFamily {
            representative: rep,
            variants,
            support: members.len(),
        });
    }
    out.sort_by(|a, b| b.support.cmp(&a.support).then(a.representative.onsets_beats[0].total_cmp(&b.representative.onsets_beats[0])));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(p: &[f64], o: &[f64], d: &[f64]) -> MotifStatement {
        MotifStatement::new(p.to_vec(), o.to_vec(), d.to_vec()).unwrap()
    }

    #[test]
    fn global_transpose_preserves_relative_motif() {
        let a = st(&[60.0, 64.0, 62.0, 67.0], &[0.0, 0.5, 1.0, 1.5], &[0.5, 0.5, 0.5, 1.0]);
        let b = a.transpose(7.0);
        let c = compare(&a, &b);
        assert_eq!(c.relation, MotifRelation::Faithful);
        assert!(c.contour_equal);
    }

    #[test]
    fn uniform_tempo_scale_preserves_proportional_rhythm_and_relation() {
        let a = st(&[60.0, 62.0, 65.0], &[0.0, 1.0, 2.0], &[1.0, 1.0, 2.0]);
        let b = a.scale_rhythm(1.5).unwrap();
        let c = compare(&a, &b);
        assert_eq!(c.relation, MotifRelation::Faithful, "{c:?}");
    }

    #[test]
    fn one_wrong_structural_note_changes_the_relation() {
        let a = st(&[60.0, 64.0, 67.0, 72.0], &[0.0, 1.0, 2.0, 3.0], &[1.0; 4]);
        let b = st(&[60.0, 64.0, 66.0, 72.0], &[0.0, 1.0, 2.0, 3.0], &[1.0; 4]);
        let c = compare(&a, &b);
        assert!(c.relation < MotifRelation::Metric, "one wrong note must break exact contour: {c:?}");
    }

    #[test]
    fn similar_rhythm_unrelated_pitches_do_not_collapse() {
        let a = st(&[60.0, 63.0, 65.0], &[0.0, 1.0, 2.0], &[1.0, 1.0, 1.0]);
        let b = st(&[61.0, 66.0, 68.0], &[0.0, 1.0, 2.0], &[1.0, 1.0, 1.0]);
        let c = compare(&a, &b);
        assert_eq!(c.relation, MotifRelation::Free, "{c:?}");
    }

    #[test]
    fn same_contour_different_rhythm_is_metric_not_faithful() {
        let a = st(&[60.0, 62.0, 64.0], &[0.0, 1.0, 2.0], &[1.0, 1.0, 1.0]);
        let b = st(&[60.0, 62.0, 64.0], &[0.0, 1.0, 2.0], &[0.5, 0.5, 3.0]);
        let c = compare(&a, &b);
        assert_eq!(c.relation, MotifRelation::Metric, "{c:?}");
        assert!(c.rivals.contains(&MotifRelation::Faithful));
    }

    #[test]
    fn deletion_degrades_predictably() {
        let a = st(&[60.0, 62.0, 64.0, 65.0], &[0.0, 1.0, 2.0, 3.0], &[1.0; 4]);
        let b = a.fragment(2).unwrap();
        let c = compare(&a, &b);
        assert!(!c.length_equal);
        assert!(c.relation <= MotifRelation::Theme);
    }

    #[test]
    fn families_group_transposed_repeats() {
        let a = st(&[60.0, 64.0, 62.0], &[0.0, 1.0, 2.0], &[1.0, 1.0, 1.0]);
        let b = a.transpose(5.0);
        let c = a.transpose(-12.0);
        let cands = vec![
            CandidateWindow { start_seconds: 0.0, end_seconds: 3.0, statement: a },
            CandidateWindow { start_seconds: 4.0, end_seconds: 7.0, statement: b },
            CandidateWindow { start_seconds: 8.0, end_seconds: 11.0, statement: c },
        ];
        let fams = families(&cands, 0.0);
        assert_eq!(fams.len(), 1, "transposed repeats should be one family: {fams:?}");
        assert_eq!(fams[0].support, 3);
    }
}

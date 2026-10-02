//! Per-axis evaluation of a recovered quotient against hidden ground truth.
//!
//! There is deliberately no single "accuracy" number. Each axis reports its own residual and
//! status, and classification errors (unknown/free/derived) are counted separately, because a
//! plausible global score can hide one catastrophically wrong load-bearing axis.

use serde::{Deserialize, Serialize};

use crate::form::{compare_forms, FormInference, FormSection};
use crate::groove::{compare_groove, GrooveInference, GrooveStroke};
use crate::harmony::{relation_between, ChordLabel};
use crate::motif::{compare, MotifStatement};
use crate::quotient::{AxisKnowledge, AxisRelation, RecoveredQuotient};
use crate::truth::TruthTrack;

/// Schema id for evaluation reports.
pub const METRICS_SCHEMA: &str = "sai.metrics/v1";

/// Onset matching tolerance in beats for note events.
pub const NOTE_ONSET_TOL_BEATS: f64 = 0.12;
/// Pitch matching tolerance in semitones for note events.
pub const NOTE_PITCH_TOL_SEMITONES: f64 = 0.75;

/// One axis's residual.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisResidual {
    pub axis: String,
    /// `ok`, `unknown`, `absent`, `free`, or `mismatch`.
    pub status: String,
    /// A bounded numeric residual where one is meaningful (higher is better for relations).
    pub value: Option<f64>,
    pub detail: String,
}

/// The full per-axis report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub schema: String,
    pub id: String,
    pub split: String,
    pub axes: Vec<AxisResidual>,
    /// Axes the hidden score establishes but the analyzer marked Unknown.
    pub false_unknown: Vec<String>,
    /// Axes the hidden score establishes but the analyzer marked deliberately Free.
    pub false_free: Vec<String>,
    /// Axes the hidden score does not establish but the analyzer invented content for.
    pub false_present: Vec<String>,
}

impl EvaluationReport {
    pub fn axis(&self, name: &str) -> Option<&AxisResidual> {
        self.axes.iter().find(|a| a.axis == name)
    }
    pub fn to_json(&self) -> crate::SaiResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

fn residual(axis: &str, status: &str, value: Option<f64>, detail: impl Into<String>) -> AxisResidual {
    AxisResidual {
        axis: axis.into(),
        status: status.into(),
        value,
        detail: detail.into(),
    }
}

/// Greedy one-to-one match count between two onset/pitch point sets.
fn match_notes(
    pred: &[(f64, f64)],
    truth: &[(f64, f64)],
) -> (usize, usize) {
    let mut used = vec![false; truth.len()];
    let mut matched = 0usize;
    for &(po, pp) in pred {
        let mut best: Option<(usize, f64)> = None;
        for (j, &(to, tp)) in truth.iter().enumerate() {
            if used[j] {
                continue;
            }
            let d = (po - to).abs();
            if d <= NOTE_ONSET_TOL_BEATS && (pp - tp).abs() <= NOTE_PITCH_TOL_SEMITONES {
                if best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((j, d));
                }
            }
        }
        if let Some((j, _)) = best {
            used[j] = true;
            matched += 1;
        }
    }
    (matched, pred.len())
}

fn f1(precision: f64, recall: f64) -> f64 {
    if precision + recall > 1e-12 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    }
}

fn first_phrase(line: &[(f64, f64, f64)]) -> MotifStatement {
    // line: (onset_beat, dur_beats, pitch)
    let mut pitches = Vec::new();
    let mut onsets = Vec::new();
    let mut durs = Vec::new();
    let mut prev = f64::NEG_INFINITY;
    for &(o, d, p) in line {
        if prev.is_finite() && (o - prev) > 1.0 && !pitches.is_empty() {
            break;
        }
        pitches.push(p);
        onsets.push(o);
        durs.push(d.max(0.05));
        prev = o;
    }
    MotifStatement::new(pitches, onsets, durs).unwrap_or_else(|_| {
        MotifStatement::new(vec![60.0], vec![0.0], vec![1.0]).expect("single-note fallback")
    })
}

/// Evaluate a recovered quotient against the hidden truth.
pub fn evaluate(recovered: &RecoveredQuotient, truth: &TruthTrack) -> EvaluationReport {
    let mut axes = Vec::new();

    // --- timing: tempo (octave-folded) ---
    match (recovered.tempo_bpm, truth.tempo_bpm) {
        (Some(est), true_bpm) if est > 1e-9 => {
            let ratio = est / true_bpm;
            let octaves = ratio.log2().round();
            let folded = ratio / 2f64.powf(octaves);
            let rel_err = (folded - 1.0).abs();
            axes.push(residual(
                "timing.tempo",
                if rel_err <= 0.02 { "ok" } else { "mismatch" },
                Some(1.0 - rel_err.min(1.0)),
                format!("estimated {est:.2} bpm vs true {true_bpm:.2} bpm (octave-folded rel err {rel_err:.4})"),
            ));
        }
        _ => axes.push(residual("timing.tempo", "unknown", None, "no tempo estimate")),
    }

    // --- timing: beat phase residual ---
    if !recovered.beats_seconds.is_empty() {
        let true_interval = if truth.notes.is_empty() {
            0.5
        } else {
            60.0 / truth.tempo_bpm
        };
        let mut errs: Vec<f64> = Vec::new();
        for b in &truth.beats_from_seconds_estimate() {
            if let Some(n) = recovered
                .beats_seconds
                .iter()
                .min_by(|x, y| (*x - b).abs().total_cmp(&(*y - b).abs()))
            {
                errs.push((n - b).abs() / true_interval.max(1e-9));
            }
        }
        errs.sort_by(f64::total_cmp);
        if !errs.is_empty() {
            let median = errs[errs.len() / 2];
            axes.push(residual(
                "timing.beat",
                if median <= 0.15 { "ok" } else { "mismatch" },
                Some(1.0 - median.min(1.0)),
                format!("median nearest-beat residual {median:.3} beats over {} truth beats", errs.len()),
            ));
        }
    } else {
        axes.push(residual("timing.beat", "unknown", None, "no beat grid"));
    }

    // --- notes: all + lead ---
    let pred_all: Vec<(f64, f64)> = recovered
        .notes
        .iter()
        .map(|n| (n.onset_beat, n.pitch_midi))
        .collect();
    let truth_all: Vec<(f64, f64)> = truth
        .notes
        .iter()
        .map(|n| (n.onset_beat, n.pitch_midi))
        .collect();
    let (m, _p) = match_notes(&pred_all, &truth_all);
    let precision = if !pred_all.is_empty() { m as f64 / pred_all.len() as f64 } else { 0.0 };
    let recall = if !truth_all.is_empty() { m as f64 / truth_all.len() as f64 } else { 1.0 };
    axes.push(residual(
        "notes.all",
        if f1(precision, recall) >= 0.5 { "ok" } else { "mismatch" },
        Some(f1(precision, recall)),
        format!("p={precision:.3} r={recall:.3} f1={:.3} ({m} matched of {} pred / {} truth)",
            f1(precision, recall), pred_all.len(), truth_all.len()),
    ));

    let pred_lead: Vec<(f64, f64)> = recovered
        .notes
        .iter()
        .filter(|n| n.role.as_deref() == Some("lead"))
        .map(|n| (n.onset_beat, n.pitch_midi))
        .collect();
    let truth_lead: Vec<(f64, f64)> = truth
        .notes
        .iter()
        .filter(|n| n.role == "lead")
        .map(|n| (n.onset_beat, n.pitch_midi))
        .collect();
    let (ml, _pl) = match_notes(&pred_lead, &truth_lead);
    let lp = if !pred_lead.is_empty() { ml as f64 / pred_lead.len() as f64 } else { 0.0 };
    let lr = if !truth_lead.is_empty() { ml as f64 / truth_lead.len() as f64 } else { 1.0 };
    axes.push(residual(
        "notes.lead",
        if f1(lp, lr) >= 0.4 { "ok" } else { "mismatch" },
        Some(f1(lp, lr)),
        format!("p={lp:.3} r={lr:.3} f1={:.3} ({} pred / {} truth)", f1(lp, lr), pred_lead.len(), truth_lead.len()),
    ));

    // --- motif relation recovery (recovered lead vs truth lead) ---
    {
        let rec_line: Vec<(f64, f64, f64)> = recovered
            .notes
            .iter()
            .filter(|n| n.role.as_deref() == Some("lead") || n.role.is_none())
            .map(|n| (n.onset_beat, n.dur_beats, n.pitch_midi))
            .collect();
        let truth_line: Vec<(f64, f64, f64)> = truth
            .notes
            .iter()
            .filter(|n| n.role == "lead")
            .map(|n| (n.onset_beat, n.dur_beats, n.pitch_midi))
            .collect();
        if !rec_line.is_empty() && !truth_line.is_empty() {
            let a = first_phrase(&rec_line);
            let b = first_phrase(&truth_line);
            let c = compare(&a, &b);
            axes.push(residual(
                "motif.relation",
                if c.relation.rank() >= 2 { "ok" } else { "mismatch" },
                Some(c.relation.rank() as f64),
                format!(
                    "relation={} similarity={:.3} contour={} onsets={} rhythm={}",
                    c.relation.label(), c.similarity, c.contour_equal, c.onsets_equal, c.rhythm_equal
                ),
            ));
        } else {
            axes.push(residual("motif.relation", "absent", None, "recovered or truth lead line empty"));
        }
    }

    // --- harmony relation + root accuracy ---
    {
        let rec: Vec<ChordLabel> = recovered
            .chords
            .iter()
            .map(|c| ChordLabel { root_pc: c.root_pc, quality: c.quality.clone() })
            .collect();
        let tru: Vec<ChordLabel> = truth
            .chords
            .iter()
            .map(|c| ChordLabel { root_pc: c.root_pc, quality: c.quality.clone() })
            .collect();
        if !rec.is_empty() && !tru.is_empty() {
            let rel = relation_between(&rec, &tru);
            axes.push(residual(
                "harmony.relation",
                if rel.rank() >= 2 { "ok" } else { "mismatch" },
                Some(rel.rank() as f64),
                format!("relation={} ({} recovered vs {} truth chords)", rel.label(), rec.len(), tru.len()),
            ));
            // Root accuracy over the first min(len) chords.
            let n = rec.len().min(tru.len());
            let hit = (0..n).filter(|&i| rec[i].root_pc == tru[i].root_pc).count();
            let acc = if n > 0 { hit as f64 / n as f64 } else { 0.0 };
            axes.push(residual(
                "harmony.root",
                if acc >= 0.5 { "ok" } else { "mismatch" },
                Some(acc),
                format!("{hit}/{n} roots agree in order"),
            ));
        } else {
            axes.push(residual("harmony.relation", "absent", None, "recovered or truth chord sequence empty"));
        }
    }

    // --- groove relation ---
    {
        let gi = |strokes: Vec<GrooveStroke>| GrooveInference {
            provenance: crate::evidence::Provenance::derived("evaluator", "1", "cmp/v1", &["truth"]),
            skeleton: strokes.iter().filter(|s| s.on_quarter()).cloned().collect(),
            strokes,
            beats_per_bar: 4,
            ambiguity: vec![],
        };
        let rec = gi(recovered.groove_strokes.clone());
        let tru = gi(truth
            .drums
            .iter()
            .map(|d| GrooveStroke {
                second: d.second,
                at_beat: d.at_beat,
                voice: d.voice.clone(),
                confidence: 1.0,
            })
            .collect());
        if !rec.is_empty() && !tru.is_empty() {
            let c = compare_groove(&rec, &tru, 1.0 / 8.0);
            axes.push(residual(
                "groove.relation",
                if c.relation.rank() >= 1 { "ok" } else { "mismatch" },
                Some(c.relation.rank() as f64),
                format!(
                    "relation={} full={:.3} skeleton={:.3}",
                    c.relation.label(), c.full_match, c.skeleton_match
                ),
            ));
        } else {
            axes.push(residual("groove.relation", "absent", None, "recovered or truth groove empty"));
        }
    }

    // --- form topology ---
    {
        let mk = |sections: Vec<FormSection>| FormInference {
            provenance: crate::evidence::Provenance::derived("evaluator", "1", "cmp/v1", &["truth"]),
            topology: {
                let mut t = Vec::new();
                for s in &sections {
                    if t.last() != Some(&s.family) {
                        t.push(s.family);
                    }
                }
                t
            },
            sections,
            boundary_source: "cmp".into(),
            ambiguity: vec![],
        };
        let rec = mk(recovered.sections.clone());
        let tru = mk(truth
            .sections
            .iter()
            .map(|s| FormSection {
                start_second: 0.0,
                end_second: 0.0,
                start_beat: s.start_beat,
                end_beat: s.end_beat,
                family: s.family,
            })
            .collect());
        if !rec.is_empty() && !tru.is_empty() {
            let c = compare_forms(&rec, &tru, 0.5);
            axes.push(residual(
                "form.topology",
                if c.relation.rank() >= 1 { "ok" } else { "mismatch" },
                Some(c.relation.rank() as f64),
                format!(
                    "relation={} topology_equal={} spans_equal={}",
                    c.relation.label(), c.topology_equal, c.spans_equal
                ),
            ));
        } else {
            axes.push(residual("form.topology", "absent", None, "recovered or truth form empty"));
        }
    }

    // --- classification ---
    let est_axes: Vec<(&str, bool)> = vec![
        ("motif", !truth.notes.iter().all(|n| n.role != "lead")),
        ("harmony", !truth.chords.is_empty()),
        ("groove", !truth.drums.is_empty()),
        ("form", !truth.sections.is_empty()),
    ];
    let mut false_unknown = Vec::new();
    let mut false_free = Vec::new();
    let mut false_present = Vec::new();
    for (axis, established) in &est_axes {
        let k = match *axis {
            "motif" => Some(&recovered.motif.knowledge),
            "harmony" => Some(&recovered.harmony.knowledge),
            "groove" => Some(&recovered.groove.knowledge),
            "form" => Some(&recovered.form.knowledge),
            _ => None,
        };
        if let Some(k) = k {
            if *established && k.is_unknown() {
                false_unknown.push((*axis).to_string());
            }
            // Effective Free with an established truth axis is deliberate only when the
            // requested profile did not pin it; otherwise it is a missed coordinate.
            if *established && k.is_free() && recovered.profile_pins(axis) {
                false_free.push((*axis).to_string());
            }
            let has_content = match *axis {
                "motif" => !recovered.notes.is_empty(),
                "harmony" => !recovered.chords.is_empty(),
                "groove" => !recovered.groove_strokes.is_empty(),
                "form" => !recovered.sections.is_empty(),
                _ => false,
            };
            if !*established && has_content {
                false_present.push((*axis).to_string());
            }
        }
    }
    axes.push(residual(
        "classification",
        if false_unknown.is_empty() && false_present.is_empty() { "ok" } else { "mismatch" },
        Some(false_unknown.len() as f64),
        format!(
            "false_unknown={:?} false_free={:?} false_present={:?}",
            false_unknown, false_free, false_present
        ),
    ));

    let _ = AxisKnowledge::Unknown; // keep the import exercised for readers
    EvaluationReport {
        schema: METRICS_SCHEMA.into(),
        id: truth.id.clone(),
        split: truth.split.clone(),
        axes,
        false_unknown,
        false_free,
        false_present,
    }
}

/// A small helper on the truth side: the truth beats are reconstructed from the note grid only
/// when needed for phase comparison. Kept explicit so the metric does not pretend the hidden
/// transport is exposed.
trait TruthBeats {
    fn beats_from_seconds_estimate(&self) -> Vec<f64>;
}

impl TruthBeats for TruthTrack {
    fn beats_from_seconds_estimate(&self) -> Vec<f64> {
        if self.tempo_bpm <= 0.0 {
            return Vec::new();
        }
        let interval = 60.0 / self.tempo_bpm;
        let n = (self.duration_seconds / interval).floor() as usize;
        (0..=n).map(|i| i as f64 * interval).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotient::{AxisResult, RecoveredChord, RecoveredNote, RequestedProfile};
    use crate::truth::{TruthChord, TruthNote, TruthStroke};

    fn truth() -> TruthTrack {
        TruthTrack {
            schema: crate::truth::TRUTH_SCHEMA.into(),
            id: "x".into(),
            world: "BlackIce".into(),
            seed: 1,
            split: "dev".into(),
            tempo_bpm: 120.0,
            beats_per_bar: 4.0,
            total_beats: 8.0,
            duration_seconds: 4.0,
            notes: vec![
                TruthNote { onset_beat: 0.0, dur_beats: 1.0, pitch_midi: 69.0, velocity: 0.8, role: "lead".into(), onset_second: 0.0, offset_second: 0.5 },
                TruthNote { onset_beat: 1.0, dur_beats: 1.0, pitch_midi: 71.0, velocity: 0.8, role: "lead".into(), onset_second: 0.5, offset_second: 1.0 },
            ],
            chords: vec![TruthChord { at_beat: 0.0, end_beat: 4.0, root_pc: 9, quality: "min".into(), family: "minor".into() }],
            drums: vec![TruthStroke { at_beat: 0.0, second: 0.0, voice: "kick".into() }],
            sections: vec![],
            established_axes: vec![],
            wav_sha256: "c".repeat(64),
            pcm_sha256: None,
        }
    }

    fn empty_axis<R: crate::quotient::AxisRelation>(axis: &str) -> AxisResult<R> {
        AxisResult {
            axis: axis.into(),
            requested: R::free(),
            effective: R::free(),
            knowledge: AxisKnowledge::Free,
            evidence: vec![],
            lowering: vec![],
            ambiguity: vec![],
        }
    }

    #[test]
    fn exact_notes_score_high() {
        let t = truth();
        let q = RecoveredQuotient {
            schema: crate::quotient::RecoveredQuotient::SCHEMA.into(),
            source_sha256: "a".repeat(64),
            profile: RequestedProfile::interpretive(),
            tempo_bpm: Some(120.0),
            beats_seconds: vec![0.0, 0.5, 1.0],
            motif: empty_axis("motif"),
            harmony: empty_axis("harmony"),
            groove: empty_axis("groove"),
            form: empty_axis("form"),
            orchestration: empty_axis("orchestration"),
            notes: vec![
                RecoveredNote { onset_second: 0.0, onset_beat: 0.0, dur_beats: 1.0, pitch_midi: 69.0, role: Some("lead".into()), confidence: 0.9 },
                RecoveredNote { onset_second: 0.5, onset_beat: 1.0, dur_beats: 1.0, pitch_midi: 71.0, role: Some("lead".into()), confidence: 0.9 },
            ],
            motif_families: vec![],
            chords: vec![RecoveredChord { at_beat: 0.0, end_beat: 4.0, root_pc: 9, quality: "min".into(), confidence: 0.9, routes: vec![], rivals: vec![] }],
            groove_strokes: vec![GrooveStroke { second: 0.0, at_beat: 0.0, voice: "kick".into(), confidence: 0.9 }],
            sections: vec![],
            refusals: vec![],
            unknowns: vec![],
            provenance: vec![],
        };
        let r = evaluate(&q, &t);
        assert_eq!(r.axis("notes.all").unwrap().value, Some(1.0));
        assert!(r.axis("motif.relation").unwrap().value.unwrap() >= 3.0);
        assert_eq!(r.axis("harmony.relation").unwrap().value, Some(3.0));
        assert_eq!(r.axis("groove.relation").unwrap().value, Some(2.0));
    }
}

//! The semantic quotient: projection of evidence into a `CoverMap`-compatible identity object.
//!
//! Every axis reports **requested** relation, **effective** relation, the evidence families behind
//! it, the reason any request was lowered, and any live ambiguity. `Unknown` is not `Free`, and a
//! missing measurement is never filled merely because a stronger preset was requested. This is the
//! inverse analogue of HumanMusic's `CoverFidelityProfile` ceiling logic.
//!
//! The recovered quotient deliberately contains no source seed, trace, score, patch or lookup key;
//! it holds only the selected musical identities and their provenance, exactly like `CoverMap`.

use serde::{Deserialize, Serialize};

use crate::error::{finite, SaiResult};
use crate::evidence::{EvidenceArtifact, EvidenceLevel, NoteEvidence, Refusal};
use crate::form::{self, FormInference, FormRelation};
use crate::groove::{self, GrooveInference, GrooveRelation, GrooveStroke};
use crate::harmony::{self, HarmonyRelation, Reconciliation};
use crate::motif::{self, CandidateWindow, MotifFamily, MotifRelation, MotifStatement};
use crate::transport::BeatGrid;

/// Exact relations on the seating (mirrors HumanMusic's `OrchestrationRelation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrchestrationRelation {
    Free,
    Exact,
}

impl OrchestrationRelation {
    pub fn label(self) -> &'static str {
        match self {
            OrchestrationRelation::Free => "free",
            OrchestrationRelation::Exact => "exact",
        }
    }
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s {
            "free" => Self::Free,
            "exact" => Self::Exact,
            _ => return None,
        })
    }
}

/// A relation that can be requested and lowered. Implemented by every axis relation.
pub trait AxisRelation: Copy + Ord + Serialize + for<'de> Deserialize<'de> {
    fn free() -> Self;
    fn strongest() -> Self;
    fn label(self) -> &'static str;
    fn from_label(s: &str) -> Option<Self>;
    fn rank(self) -> u8;
}

impl AxisRelation for MotifRelation {
    fn free() -> Self {
        MotifRelation::Free
    }
    fn strongest() -> Self {
        MotifRelation::Faithful
    }
    fn label(self) -> &'static str {
        MotifRelation::label(self)
    }
    fn from_label(s: &str) -> Option<Self> {
        MotifRelation::from_label(s)
    }
    fn rank(self) -> u8 {
        self as u8
    }
}
impl AxisRelation for HarmonyRelation {
    fn free() -> Self {
        HarmonyRelation::Free
    }
    fn strongest() -> Self {
        HarmonyRelation::Exact
    }
    fn label(self) -> &'static str {
        HarmonyRelation::label(self)
    }
    fn from_label(s: &str) -> Option<Self> {
        HarmonyRelation::from_label(s)
    }
    fn rank(self) -> u8 {
        self as u8
    }
}
impl AxisRelation for GrooveRelation {
    fn free() -> Self {
        GrooveRelation::Free
    }
    fn strongest() -> Self {
        GrooveRelation::KickSnare
    }
    fn label(self) -> &'static str {
        GrooveRelation::label(self)
    }
    fn from_label(s: &str) -> Option<Self> {
        GrooveRelation::from_label(s)
    }
    fn rank(self) -> u8 {
        self as u8
    }
}
impl AxisRelation for FormRelation {
    fn free() -> Self {
        FormRelation::Free
    }
    fn strongest() -> Self {
        FormRelation::Exact
    }
    fn label(self) -> &'static str {
        FormRelation::label(self)
    }
    fn from_label(s: &str) -> Option<Self> {
        FormRelation::from_label(s)
    }
    fn rank(self) -> u8 {
        self as u8
    }
}
impl AxisRelation for OrchestrationRelation {
    fn free() -> Self {
        OrchestrationRelation::Free
    }
    fn strongest() -> Self {
        OrchestrationRelation::Exact
    }
    fn label(self) -> &'static str {
        OrchestrationRelation::label(self)
    }
    fn from_label(s: &str) -> Option<Self> {
        OrchestrationRelation::from_label(s)
    }
    fn rank(self) -> u8 {
        self as u8
    }
}

/// The fidelity profile requested of the analyzer, one exact relation per axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestedProfile {
    pub motif: MotifRelation,
    pub harmony: HarmonyRelation,
    pub groove: GrooveRelation,
    pub form: FormRelation,
    pub orchestration: OrchestrationRelation,
}

impl RequestedProfile {
    pub const FREE: Self = Self {
        motif: MotifRelation::Free,
        harmony: HarmonyRelation::Free,
        groove: GrooveRelation::Free,
        form: FormRelation::Free,
        orchestration: OrchestrationRelation::Free,
    };
    /// The Loose preset: the opening statement as theme.
    pub fn loose() -> Self {
        Self { motif: MotifRelation::Theme, ..Self::FREE }
    }
    /// The Interpretive preset.
    pub fn interpretive() -> Self {
        Self {
            motif: MotifRelation::Metric,
            harmony: HarmonyRelation::QualityFamily,
            groove: GrooveRelation::PocketSkeleton,
            ..Self::FREE
        }
    }
    /// The Faithful preset.
    pub fn faithful() -> Self {
        Self {
            motif: MotifRelation::Faithful,
            harmony: HarmonyRelation::Exact,
            groove: GrooveRelation::KickSnare,
            form: FormRelation::Exact,
            ..Self::FREE
        }
    }
    /// The Strict preset.
    pub fn strict() -> Self {
        Self { orchestration: OrchestrationRelation::Exact, ..Self::faithful() }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "loose" => Self::loose(),
            "interpretive" => Self::interpretive(),
            "faithful" => Self::faithful(),
            "strict" => Self::strict(),
            "free" => Self::FREE,
            _ => return None,
        })
    }
}

/// The epistemic state of a quotient axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AxisKnowledge {
    /// Established from direct source evidence. Never claimed for audio-derived coordinates.
    Established,
    /// Derived by a named method at a declared evidence level.
    Derived { method: String, level: EvidenceLevel },
    /// Deliberately free under the requested relation: the coordinate was not pinned.
    Free,
    /// No evidence reached the axis. Distinct from Free.
    Unknown,
    /// Evidence exists but rival hypotheses are unresolved.
    Ambiguous { rivals: usize },
}

impl AxisKnowledge {
    pub fn is_unknown(&self) -> bool {
        matches!(self, AxisKnowledge::Unknown)
    }
    pub fn is_free(&self) -> bool {
        matches!(self, AxisKnowledge::Free)
    }
    pub fn label(&self) -> &'static str {
        match self {
            AxisKnowledge::Established => "established",
            AxisKnowledge::Derived { .. } => "derived",
            AxisKnowledge::Free => "free",
            AxisKnowledge::Unknown => "unknown",
            AxisKnowledge::Ambiguous { .. } => "ambiguous",
        }
    }
}

/// One axis of the recovered quotient: what was asked, what was achieved, why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisResult<R> {
    pub axis: String,
    pub requested: R,
    pub effective: R,
    pub knowledge: AxisKnowledge,
    /// Evidence families / methods behind the datum.
    pub evidence: Vec<String>,
    /// Reasons the requested relation was lowered.
    pub lowering: Vec<String>,
    /// Live rival hypotheses the evidence does not separate.
    pub ambiguity: Vec<String>,
}

impl<R: AxisRelation> AxisResult<R> {
    /// Resolve a request against an evidential ceiling. `None` ceiling = Unknown.
    fn resolve(
        axis: &str,
        requested: R,
        ceiling: Option<R>,
        method: &str,
        level: EvidenceLevel,
        evidence: Vec<String>,
    ) -> Self {
        if requested == R::free() {
            return Self {
                axis: axis.into(),
                requested,
                effective: R::free(),
                knowledge: AxisKnowledge::Free,
                evidence,
                lowering: vec![],
                ambiguity: vec![],
            };
        }
        match ceiling {
            None => Self {
                axis: axis.into(),
                requested,
                effective: R::free(),
                knowledge: AxisKnowledge::Unknown,
                evidence,
                lowering: vec![format!("no evidence supports a {axis} relation")],
                ambiguity: vec![],
            },
            Some(c) => {
                let effective = requested.min(c);
                let mut lowering = Vec::new();
                if effective < requested {
                    lowering.push(format!(
                        "ceiling is {} ({}), requested {}",
                        c.label(),
                        c.rank(),
                        requested.label()
                    ));
                }
                Self {
                    axis: axis.into(),
                    requested,
                    effective,
                    knowledge: AxisKnowledge::Derived { method: method.into(), level },
                    evidence,
                    lowering,
                    ambiguity: vec![],
                }
            }
        }
    }
}

/// A recovered pitched note (content of the line axes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveredNote {
    pub onset_second: f64,
    pub onset_beat: f64,
    pub dur_beats: f64,
    pub pitch_midi: f64,
    #[serde(default)]
    pub role: Option<String>,
    pub confidence: f64,
}

/// A recovered chord span (content of the harmony axis).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveredChord {
    pub at_beat: f64,
    pub end_beat: f64,
    pub root_pc: i32,
    pub quality: String,
    pub confidence: f64,
    /// The routes that produced it.
    pub routes: Vec<String>,
    #[serde(default)]
    pub rivals: Vec<String>,
}

/// A recovered section (content of the form axis).
pub type RecoveredSection = form::FormSection;

/// The recovered quotient: selected musical identities plus per-axis honesty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveredQuotient {
    pub schema: String,
    pub source_sha256: String,
    pub profile: RequestedProfile,
    /// Estimated global tempo (median of the transport), if a beat grid was built.
    #[serde(default)]
    pub tempo_bpm: Option<f64>,
    /// Detected beat times in raw seconds (authoritative coordinate).
    #[serde(default)]
    pub beats_seconds: Vec<f64>,
    pub motif: AxisResult<MotifRelation>,
    pub harmony: AxisResult<HarmonyRelation>,
    pub groove: AxisResult<GrooveRelation>,
    pub form: AxisResult<FormRelation>,
    pub orchestration: AxisResult<OrchestrationRelation>,
    /// Line content (all recovered pitched notes).
    #[serde(default)]
    pub notes: Vec<RecoveredNote>,
    /// Motif families over phrase windows (diagnostic; never the identity authority).
    #[serde(default)]
    pub motif_families: Vec<MotifFamily>,
    /// Harmony content.
    #[serde(default)]
    pub chords: Vec<RecoveredChord>,
    /// Groove content.
    #[serde(default)]
    pub groove_strokes: Vec<GrooveStroke>,
    /// Form content.
    #[serde(default)]
    pub sections: Vec<RecoveredSection>,
    pub refusals: Vec<Refusal>,
    pub unknowns: Vec<String>,
    /// Human-readable provenance chain for this quotient.
    pub provenance: Vec<String>,
}

impl RecoveredQuotient {
    pub const SCHEMA: &'static str = "sai.quotient/v1";

    pub fn to_json(&self) -> SaiResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json_slice(bytes: &[u8]) -> SaiResult<Self> {
        let q: RecoveredQuotient = serde_json::from_slice(bytes)?;
        Ok(q)
    }

    /// Effective relations as a compact profile.
    pub fn effective_profile(&self) -> RequestedProfile {
        RequestedProfile {
            motif: self.motif.effective,
            harmony: self.harmony.effective,
            groove: self.groove.effective,
            form: self.form.effective,
            orchestration: self.orchestration.effective,
        }
    }

    /// Whether the requested profile pins `axis` (asks for more than Free).
    pub fn profile_pins(&self, axis: &str) -> bool {
        match axis {
            "motif" => self.profile.motif != MotifRelation::free(),
            "harmony" => self.profile.harmony != HarmonyRelation::free(),
            "groove" => self.profile.groove != GrooveRelation::free(),
            "form" => self.profile.form != FormRelation::free(),
            "orchestration" => self.profile.orchestration != OrchestrationRelation::free(),
            _ => false,
        }
    }
}

// --- content construction ---

fn best_role(n: &NoteEvidence) -> Option<String> {
    n.role_candidates
        .iter()
        .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
        .map(|c| c.role.clone())
}

fn to_recovered_notes(artifact: &EvidenceArtifact, grid: &BeatGrid) -> SaiResult<Vec<RecoveredNote>> {
    let mut out = Vec::with_capacity(artifact.notes.len());
    for n in &artifact.notes {
        let onset_beat = grid.seconds_to_beat(n.onset_second)?.beat;
        let dur_beats = match n.offset_second {
            Some(o) => (grid.seconds_to_beat(o)?.beat - onset_beat).max(0.05),
            None => 0.5,
        };
        out.push(RecoveredNote {
            onset_second: n.onset_second,
            onset_beat,
            dur_beats,
            pitch_midi: n.pitch_midi,
            role: best_role(n),
            confidence: n.confidence,
        });
    }
    out.sort_by(|a, b| a.onset_beat.total_cmp(&b.onset_beat));
    Ok(out)
}

/// Melody reduction: the highest-confidence / highest-pitch note per onset cluster.
fn lead_line(notes: &[RecoveredNote]) -> Vec<RecoveredNote> {
    let has_lead = notes
        .iter()
        .any(|n| n.role.as_deref() == Some("lead"));
    let mut src: Vec<RecoveredNote> = if has_lead {
        notes
            .iter()
            .filter(|n| n.role.as_deref() == Some("lead"))
            .cloned()
            .collect()
    } else {
        notes.to_vec()
    };
    src.sort_by(|a, b| a.onset_beat.total_cmp(&b.onset_beat));
    // Collapse near-simultaneous notes to the highest pitch (a monophonic motif statement).
    let mut out: Vec<RecoveredNote> = Vec::new();
    for n in src {
        match out.last_mut() {
            Some(prev) if (n.onset_beat - prev.onset_beat).abs() < 0.08 => {
                if n.pitch_midi > prev.pitch_midi {
                    *prev = n;
                }
            }
            _ => out.push(n),
        }
    }
    out
}

/// Phrase windows over a monophonic line: a new window starts after a rest.
fn phrase_windows(line: &[RecoveredNote], max_notes: usize) -> Vec<CandidateWindow> {
    let mut out = Vec::new();
    let mut pitches = Vec::new();
    let mut onsets = Vec::new();
    let mut durs = Vec::new();
    let mut start_beat = 0.0f64;
    let mut prev_beat = f64::NEG_INFINITY;
    let flush = |pitches: &mut Vec<f64>,
                     onsets: &mut Vec<f64>,
                     durs: &mut Vec<f64>,
                     start_beat: f64,
                     out: &mut Vec<CandidateWindow>| {
        if pitches.len() >= 2 {
            if let Ok(st) = MotifStatement::new(pitches.clone(), onsets.clone(), durs.clone()) {
                let end = start_beat + st.span_beats() + durs.last().copied().unwrap_or(0.5);
                out.push(CandidateWindow {
                    start_seconds: start_beat, // beats; the window is beat-referenced
                    end_seconds: end,
                    statement: st,
                });
            }
        }
        pitches.clear();
        onsets.clear();
        durs.clear();
    };
    for n in line {
        if prev_beat.is_finite() && (n.onset_beat - prev_beat) > 1.0 {
            flush(&mut pitches, &mut onsets, &mut durs, start_beat, &mut out);
        }
        if pitches.is_empty() {
            start_beat = n.onset_beat;
        }
        pitches.push(n.pitch_midi);
        onsets.push(n.onset_beat);
        durs.push(n.dur_beats);
        prev_beat = n.onset_beat;
        if pitches.len() >= max_notes {
            flush(&mut pitches, &mut onsets, &mut durs, start_beat, &mut out);
        }
    }
    flush(&mut pitches, &mut onsets, &mut durs, start_beat, &mut out);
    out
}

/// Recover a quotient from one evidence artifact under a requested profile.
pub fn recover(
    artifact: &EvidenceArtifact,
    requested: &RequestedProfile,
) -> SaiResult<RecoveredQuotient> {
    artifact.validate()?;
    let mut refusals: Vec<Refusal> = artifact.refusals.clone();
    let mut unknowns: Vec<String> = artifact.unknowns.clone();
    let mut provenance: Vec<String> = Vec::new();

    let grid = match artifact.timing.as_ref() {
        Some(t) => match BeatGrid::from_timing(t) {
            Ok(g) => Some(g),
            Err(e) => {
                refusals.push(Refusal {
                    what: "beat transport".into(),
                    reason: e.to_string(),
                });
                None
            }
        },
        None => {
            refusals.push(Refusal {
                what: "beat transport".into(),
                reason: "no timing evidence".into(),
            });
            None
        }
    };

    if grid.is_none() {
        // No transport: every beat-referenced axis is Unknown, never Free-by-default.
        return Ok(unknown_quotient(artifact, requested, refusals, unknowns));
    }
    let grid = grid.unwrap();
    let total_beats = grid
        .seconds_to_beat(artifact.source.duration_seconds)?
        .beat
        .max(1.0);

    // --- line / motif ---
    let notes = to_recovered_notes(artifact, &grid)?;
    let line = lead_line(&notes);
    let lead_conf = if line.is_empty() {
        0.0
    } else {
        line.iter().map(|n| n.confidence).sum::<f64>() / line.len() as f64
    };
    let windows = phrase_windows(&line, 8);
    let families = motif::families(&windows, 0.0);
    let motif_ceiling = if line.len() >= 2 && lead_conf >= 0.5 {
        Some(MotifRelation::Metric)
    } else if !line.is_empty() {
        Some(MotifRelation::Theme)
    } else {
        None
    };
    let mut motif_axis = AxisResult::resolve(
        "motif",
        requested.motif,
        motif_ceiling,
        "salience-onset+peak-pitch/v1",
        EvidenceLevel::DerivedMusicalEvent,
        vec![format!("note-transcription ({} notes)", notes.len())],
    );
    if motif_axis.knowledge.is_unknown() {
        unknowns.push("motif".into());
    }
    if !windows.is_empty() && families.len() < 2 {
        motif_axis
            .ambiguity
            .push("no repeated motif family crossed the declared similarity floor".into());
    }
    provenance.push("motif <- note evidence <- onset+pitch analysis of PCM".into());

    // --- harmony ---
    let rec: Option<Reconciliation> = {
        let chroma_ok = artifact
            .tonal
            .as_ref()
            .is_some_and(|t| !t.chroma_frames.is_empty());
        let a: Option<Vec<harmony::ChordSpan>> = if chroma_ok {
            harmony::chords_from_chroma(
                artifact.tonal.as_ref().unwrap(),
                &grid,
                1.0,
                total_beats,
            )?
            .into()
        } else {
            None
        };
        let b: Vec<harmony::ChordSpan> = if artifact.notes.is_empty() {
            Vec::new()
        } else {
            harmony::chords_from_notes(&artifact.notes, &grid, 1.0, total_beats)?
        };
        let mut r = harmony::reconcile(a.as_deref().unwrap_or(&[]), &b);
        if a.is_none() {
            r.lowering.push("no chroma route available".into());
        }
        if b.is_empty() {
            r.lowering.push("no note route available".into());
        }
        if a.is_none() && b.is_empty() {
            None
        } else {
            Some(r)
        }
    };
    let (harmony_ceiling, chords, harmony_ambiguity) = match &rec {
        Some(r) if !r.spans.is_empty() => {
            let chords: Vec<RecoveredChord> = r
                .spans
                .iter()
                .map(|s| RecoveredChord {
                    at_beat: s.at_beat,
                    end_beat: s.end_beat,
                    root_pc: s.selected.root_pc,
                    quality: s.selected.quality.clone(),
                    confidence: s.confidence,
                    routes: s.routes.iter().map(|x| format!("{x:?}")).collect(),
                    rivals: s.rivals.iter().map(|l| l.label()).collect(),
                })
                .collect();
            (Some(r.ceiling), chords, r.lowering.clone())
        }
        _ => (None, Vec::new(), vec![]),
    };
    let mut harmony_axis = AxisResult::resolve(
        "harmony",
        requested.harmony,
        harmony_ceiling,
        "chroma-template+note-window triangulation/v1",
        EvidenceLevel::DerivedMusicalEvent,
        vec![
            "route-a: chroma-template".into(),
            "route-b: transcribed-note window".into(),
        ],
    );
    harmony_axis.ambiguity.extend(harmony_ambiguity);
    if harmony_axis.knowledge.is_unknown() {
        unknowns.push("harmony".into());
    }
    provenance.push("harmony <- chroma-template reconciled with note-window pitch classes".into());

    // --- groove ---
    let gr: Option<GrooveInference> = if artifact.onsets.is_empty() {
        None
    } else {
        Some(groove::infer_groove(&artifact.onsets, &grid, 0.5)?)
    };
    let (groove_ceiling, strokes) = match &gr {
        Some(g) if !g.strokes.is_empty() => {
            let has_kick = g.strokes.iter().any(|s| s.voice == "kick");
            let has_snare = g.strokes.iter().any(|s| s.voice == "snare");
            let tight = g
                .strokes
                .iter()
                .all(|s| (s.at_beat - (s.at_beat * 4.0).round() / 4.0).abs() <= 0.03);
            let ceiling = if g.strokes.len() >= 2 && has_kick && has_snare && tight {
                GrooveRelation::KickSnare
            } else {
                GrooveRelation::PocketSkeleton
            };
            (Some(ceiling), g.strokes.clone())
        }
        _ => (None, Vec::new()),
    };
    let mut groove_axis = AxisResult::resolve(
        "groove",
        requested.groove,
        groove_ceiling,
        "onset-family kick/snare detection/v1",
        EvidenceLevel::DerivedMusicalEvent,
        vec![format!("onsets ({} total)", artifact.onsets.len())],
    );
    if let Some(g) = &gr {
        groove_axis.ambiguity.extend(g.ambiguity.clone());
    }
    if groove_axis.knowledge.is_unknown() {
        unknowns.push("groove".into());
    }
    provenance.push("groove <- percussion onset families over the beat grid".into());

    // --- form ---
    let fi: Option<FormInference> = match artifact.sections.as_ref() {
        Some(s) if !s.sections.is_empty() => {
            Some(form::infer_form(s, artifact.recurrence.as_ref(), &grid)?)
        }
        _ => None,
    };
    let (form_ceiling, sections) = match &fi {
        Some(f) if !f.sections.is_empty() => {
            // Audio boundaries almost never support exact bar spans: ceiling is Topology.
            (Some(FormRelation::Topology), f.sections.clone())
        }
        _ => (None, Vec::new()),
    };
    let mut form_axis = AxisResult::resolve(
        "form",
        requested.form,
        form_ceiling,
        "novelty+recurrence-family/v1",
        EvidenceLevel::DerivedStructuralInterpretation,
        vec!["section boundaries from recurrence structure".into()],
    );
    if let Some(f) = &fi {
        form_axis.ambiguity.extend(f.ambiguity.clone());
    }
    if form_axis.knowledge.is_unknown() {
        unknowns.push("form".into());
    }
    provenance.push("form <- recurrence-family topology over candidate sections".into());

    // --- orchestration (honest unknown this round) ---
    let orchestration_axis = AxisResult::resolve(
        "orchestration",
        requested.orchestration,
        None,
        "role-occupancy/v1",
        EvidenceLevel::DerivedStructuralInterpretation,
        vec![],
    );
    if orchestration_axis.knowledge.is_unknown() {
        unknowns.push("orchestration".into());
        refusals.push(Refusal {
            what: "orchestration".into(),
            reason: "per-seat occupancy from mixed audio is not implemented in this round".into(),
        });
    }
    provenance.push("orchestration <- (not attempted this round)".into());

    Ok(RecoveredQuotient {
        schema: RecoveredQuotient::SCHEMA.into(),
        source_sha256: artifact.source.sha256.clone(),
        profile: *requested,
        tempo_bpm: Some(grid.median_bpm()),
        beats_seconds: grid.beat_seconds().to_vec(),
        motif: motif_axis,
        harmony: harmony_axis,
        groove: groove_axis,
        form: form_axis,
        orchestration: orchestration_axis,
        notes,
        motif_families: families,
        chords,
        groove_strokes: strokes,
        sections,
        refusals,
        unknowns,
        provenance,
    })
}

fn unknown_quotient(
    artifact: &EvidenceArtifact,
    requested: &RequestedProfile,
    refusals: Vec<Refusal>,
    unknowns: Vec<String>,
) -> RecoveredQuotient {
    let mut unknowns = unknowns;
    for a in ["motif", "harmony", "groove", "form", "orchestration"] {
        if !unknowns.iter().any(|u| u == a) {
            unknowns.push(a.into());
        }
    }
    RecoveredQuotient {
        schema: RecoveredQuotient::SCHEMA.into(),
        source_sha256: artifact.source.sha256.clone(),
        profile: *requested,
        tempo_bpm: None,
        beats_seconds: vec![],
        motif: AxisResult {
            axis: "motif".into(),
            requested: requested.motif,
            effective: MotifRelation::Free,
            knowledge: AxisKnowledge::Unknown,
            evidence: vec![],
            lowering: vec!["no beat transport".into()],
            ambiguity: vec![],
        },
        harmony: AxisResult {
            axis: "harmony".into(),
            requested: requested.harmony,
            effective: HarmonyRelation::Free,
            knowledge: AxisKnowledge::Unknown,
            evidence: vec![],
            lowering: vec!["no beat transport".into()],
            ambiguity: vec![],
        },
        groove: AxisResult {
            axis: "groove".into(),
            requested: requested.groove,
            effective: GrooveRelation::Free,
            knowledge: AxisKnowledge::Unknown,
            evidence: vec![],
            lowering: vec!["no beat transport".into()],
            ambiguity: vec![],
        },
        form: AxisResult {
            axis: "form".into(),
            requested: requested.form,
            effective: FormRelation::Free,
            knowledge: AxisKnowledge::Unknown,
            evidence: vec![],
            lowering: vec!["no beat transport".into()],
            ambiguity: vec![],
        },
        orchestration: AxisResult {
            axis: "orchestration".into(),
            requested: requested.orchestration,
            effective: OrchestrationRelation::Free,
            knowledge: AxisKnowledge::Unknown,
            evidence: vec![],
            lowering: vec!["no beat transport".into()],
            ambiguity: vec![],
        },
        notes: vec![],
        motif_families: vec![],
        chords: vec![],
        groove_strokes: vec![],
        sections: vec![],
        refusals,
        unknowns,
        provenance: vec!["no beat transport: all axes Unknown".into()],
    }
}

/// A convenience: the strongest relation a line's raw evidence supports without a beat grid is
/// never asserted; this exists only to keep call sites total.
pub fn require_finite(v: f64, what: &'static str) -> SaiResult<f64> {
    finite(v, what)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{
        BeatEvent, ChordCandidate, ChromaFrame, Provenance, SourceReceipt, TimingEvidence,
        TonalEvidence,
    };

    fn source(dur: f64) -> SourceReceipt {
        SourceReceipt {
            sha256: "a".repeat(64),
            path_hint: None,
            duration_seconds: dur,
            sample_rate_hz: 48000,
            channels: 2,
            decoder: "test".into(),
            canonical_format: "f32le".into(),
            canonical_sample_rate_hz: 48000,
            canonical_channels: 2,
            resampled: false,
            channel_conversion: "stereo".into(),
            canonical_pcm_sha256: None,
            license: "test".into(),
        }
    }

    fn timing(beats: usize) -> TimingEvidence {
        TimingEvidence {
            provenance: Provenance::derived("test", "1", "beat/v1", &["pcm"]),
            tempo_bpm: Some(120.0),
            tempo_map: vec![],
            beats: (0..beats)
                .map(|i| BeatEvent {
                    second: i as f64 * 0.5,
                    confidence: 1.0,
                    is_downbeat: i % 4 == 0,
                })
                .collect(),
            downbeats: vec![0.0, 2.0, 4.0],
            meter_hypotheses: vec![],
            ambiguity: vec![],
        }
    }

    fn note(sec: f64, pitch: f64, role: &str) -> NoteEvidence {
        NoteEvidence {
            onset_second: sec,
            offset_second: Some(sec + 0.4),
            pitch_midi: pitch,
            role_candidates: vec![crate::evidence::RoleCandidate {
                role: role.into(),
                confidence: 0.9,
            }],
            confidence: 0.9,
            provenance: Provenance::derived("test", "1", "note/v1", &["pcm"]),
        }
    }

    #[test]
    fn no_timing_marks_every_axis_unknown() {
        let a = EvidenceArtifact {
            schema: crate::evidence::SCHEMA.into(),
            source: source(4.0),
            analyzers: vec![],
            timing: None,
            notes: vec![],
            tonal: None,
            onsets: vec![],
            recurrence: None,
            sections: None,
            refusals: vec![],
            unknowns: vec![],
        };
        let q = recover(&a, &RequestedProfile::interpretive()).unwrap();
        assert!(q.motif.knowledge.is_unknown());
        assert!(q.harmony.knowledge.is_unknown());
        assert_ne!(q.motif.knowledge, AxisKnowledge::Free);
        assert!(q.unknowns.contains(&"form".to_string()));
    }

    #[test]
    fn motif_ceiling_is_metric_never_faithful_for_audio() {
        let a = EvidenceArtifact {
            schema: crate::evidence::SCHEMA.into(),
            source: source(4.0),
            analyzers: vec![],
            timing: Some(timing(8)),
            notes: vec![note(0.0, 60.0, "lead"), note(0.5, 62.0, "lead"), note(1.0, 64.0, "lead")],
            tonal: None,
            onsets: vec![],
            recurrence: None,
            sections: None,
            refusals: vec![],
            unknowns: vec![],
        };
        let q = recover(&a, &RequestedProfile::faithful()).unwrap();
        assert_eq!(q.motif.effective, MotifRelation::Metric);
        assert!(!q.motif.lowering.is_empty());
    }

    #[test]
    fn unknown_is_not_free_when_requested_free() {
        let a = EvidenceArtifact {
            schema: crate::evidence::SCHEMA.into(),
            source: source(4.0),
            analyzers: vec![],
            timing: Some(timing(8)),
            notes: vec![],
            tonal: None,
            onsets: vec![],
            recurrence: None,
            sections: None,
            refusals: vec![],
            unknowns: vec![],
        };
        let q = recover(&a, &RequestedProfile::FREE).unwrap();
        // Not requested -> deliberately Free, not Unknown.
        assert!(q.motif.knowledge.is_free());
    }

    #[test]
    fn chroma_route_produces_chords() {
        let mut frames = Vec::new();
        for i in 0..8 {
            let mut v = [0.0f64; 12];
            v[0] = 1.0;
            v[4] = 1.0;
            v[7] = 1.0;
            frames.push(ChromaFrame {
                center_second: i as f64 * 0.25,
                hop_seconds: 0.25,
                values: v,
            });
        }
        let a = EvidenceArtifact {
            schema: crate::evidence::SCHEMA.into(),
            source: source(4.0),
            analyzers: vec![],
            timing: Some(timing(8)),
            notes: vec![note(0.0, 60.0, "keys"), note(0.0, 64.0, "keys"), note(0.0, 67.0, "keys")],
            tonal: Some(TonalEvidence {
                provenance: Provenance::derived("test", "1", "chroma/v1", &["pcm"]),
                chroma_frames: frames,
                key_candidates: vec![],
                chord_candidates: vec![],
            }),
            onsets: vec![],
            recurrence: None,
            sections: None,
            refusals: vec![],
            unknowns: vec![],
        };
        let q = recover(&a, &RequestedProfile::interpretive()).unwrap();
        assert!(!q.chords.is_empty());
        assert!(q.chords.iter().any(|c| c.root_pc == 0 && c.quality.starts_with("maj")));
    }

    #[test]
    fn chord_candidate_type_is_constructible() {
        // Guards the wire types used by adapters.
        let _ = ChordCandidate {
            at_second: 0.0,
            end_second: 1.0,
            root_pc: 0,
            quality: "maj".into(),
            inversion: "root".into(),
            confidence: 0.9,
        };
    }
}

//! # `sai-core` — Semantic Audio Inversion, neutral evidence layer and semantic core.
//!
//! This crate is the semantic centre of the Semantic Audio Inversion experiment. It owns:
//!
//! - the **neutral, versioned evidence IR** ([`evidence`]) that every measurement
//!   instrument (our own inspectable DSP baseline or an external model) must emit;
//! - **receipts** with source hashes and analyzer provenance ([`receipt`]);
//! - **beat-domain transport** that always keeps the raw seconds coordinate ([`transport`]);
//! - **musical-object inference** over evidence (motif, harmony, groove, form);
//! - a **semantic quotient** with per-axis requested/effective relations, evidence and
//!   explicit ambiguity ([`quotient`]);
//! - **per-axis metrics** for comparing a recovered quotient to a hidden direct quotient
//!   ([`metrics`]).
//!
//! ## Epistemic law
//!
//! Every inferred datum carries [`evidence::Provenance`]. A measurement model's output is
//! **derived analysis of PCM** ([`evidence::EvidenceLevel::DerivedMusicalEvent`] or
//! [`evidence::EvidenceLevel::DerivedStructuralInterpretation`]), never a directly
//! observed fact merely because a model emitted it. Unknown, ambiguous and refused are
//! first-class values.
//!
//! ## Architectural law
//!
//! External inference systems are **measurement instruments, never semantic authorities**.
//! They emit a versioned, cacheable [`evidence::EvidenceArtifact`]; the semantic core reads
//! that artifact and never links a model runtime. This is what keeps ordinary tests cheap.

pub mod audio;
pub mod error;
pub mod evidence;
pub mod form;
pub mod groove;
pub mod harmony;
pub mod metrics;
pub mod motif;
pub mod quotient;
pub mod receipt;
pub mod transport;
pub mod truth;

pub use error::{SaiError, SaiResult};
pub use evidence::{
    AnalysisLevel, AnalyzerRun, BeatEvent, ChordCandidate, ChromaFrame, EvidenceArtifact,
    EvidenceLevel, KeyCandidate, MeterHypothesis, NoteEvidence, OnsetEvidence, Provenance,
    RecurrenceEvidence, Refusal, SectionEvidence, SourceReceipt, TempoSegment, TonalEvidence,
};
pub use metrics::{evaluate, AxisResidual, EvaluationReport, METRICS_SCHEMA};
pub use quotient::{
    recover, AxisKnowledge, AxisRelation, AxisResult, OrchestrationRelation, RecoveredChord,
    RecoveredNote, RecoveredQuotient, RequestedProfile,
};
pub use truth::{TruthChord, TruthNote, TruthSection, TruthStroke, TruthTrack, TRUTH_SCHEMA};

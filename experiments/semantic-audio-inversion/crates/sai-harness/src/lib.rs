//! Blind HumanMusic ground-truth harness.
//!
//! This crate is the only place in the experiment that links the LibGibson v0.4.0 generator. It
//! produces fresh songs and their hidden truths for the blind reconstruction test. The analyzer
//! (`sai-core` + the Python adapters) never links this crate and never sees the truth.
//!
//! `sai-core` remains free of the generator dependency, so ordinary tests stay cheap and the
//! semantic core can be replayed from cached evidence alone.

pub mod dataset;
pub mod mutations;

pub use dataset::{
    build_composition, render_score, truth_from_score, ItemSpec, Manifest, ManifestItem, Rendered,
    Split, TraceKind, ANCHOR_REV, SAMPLE_RATE,
};

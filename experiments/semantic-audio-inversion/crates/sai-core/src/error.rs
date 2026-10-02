//! Fail-closed error type. No silent fallbacks: every refusal is a value.

use std::fmt;

/// The experiment's error type. Every variant names what failed and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaiError {
    /// A schema or domain violation in an evidence artifact.
    Schema(String),
    /// A non-finite / out-of-range measurement.
    NonFinite(&'static str),
    /// An audio boundary failure (decode, hash, unsupported layout).
    Audio(String),
    /// The requested axis has no supporting evidence: it is Unknown, not free.
    NoEvidence(&'static str),
    /// The evidence exists but supports only a weaker relation than requested.
    Ceiling {
        axis: &'static str,
        reason: String,
    },
    /// Two rival hypotheses disagree and no discriminator is available.
    Ambiguous {
        what: &'static str,
        rivals: usize,
    },
    /// A coordinate conversion refused to fabricate a result.
    Untransportable(String),
    /// I/O failure rendered as text (the CLI surfaces it verbatim).
    Io(String),
}

impl fmt::Display for SaiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaiError::Schema(m) => write!(f, "schema/domain violation: {m}"),
            SaiError::NonFinite(m) => write!(f, "non-finite or out-of-range value: {m}"),
            SaiError::Audio(m) => write!(f, "audio boundary failure: {m}"),
            SaiError::NoEvidence(a) => write!(f, "no evidence supports axis '{a}' (Unknown)"),
            SaiError::Ceiling { axis, reason } => {
                write!(f, "axis '{axis}' lowered to its observational ceiling: {reason}")
            }
            SaiError::Ambiguous { what, rivals } => {
                write!(f, "'{what}' is ambiguous: {rivals} live rival(s)")
            }
            SaiError::Untransportable(m) => write!(f, "untransportable coordinate: {m}"),
            SaiError::Io(m) => write!(f, "io: {m}"),
        }
    }
}

impl std::error::Error for SaiError {}

impl From<std::io::Error> for SaiError {
    fn from(e: std::io::Error) -> Self {
        SaiError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for SaiError {
    fn from(e: serde_json::Error) -> Self {
        SaiError::Schema(format!("json: {e}"))
    }
}

/// Convenience result.
pub type SaiResult<T> = Result<T, SaiError>;

/// Validate that a value is finite; otherwise fail closed with a named field.
pub fn finite(value: f64, field: &'static str) -> SaiResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(SaiError::NonFinite(field))
    }
}

/// Validate that a value lies in `[lo, hi]`.
pub fn in_range(value: f64, lo: f64, hi: f64, field: &'static str) -> SaiResult<f64> {
    let v = finite(value, field)?;
    if v < lo || v > hi {
        Err(SaiError::NonFinite(field))
    } else {
        Ok(v)
    }
}

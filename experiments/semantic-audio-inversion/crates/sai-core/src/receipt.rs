//! Receipts: source hashing, canonical-PCM hashing, and run manifests.
//!
//! Every inference run should be reproducible from a machine-readable receipt. Two artifact
//! hashes are load-bearing: the **source file hash** (what the host supplied) and the
//! **canonical PCM hash** (what the analyzer actually saw). Conflating them would hide a
//! resampling or channel-conversion step.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::evidence::{AnalyzerRun, SourceReceipt};
use crate::SaiResult;

/// SHA-256 of arbitrary bytes, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// SHA-256 of a file, streaming (never loads the whole file at once beyond a buffer).
pub fn sha256_file(path: &Path) -> SaiResult<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// A committed run manifest. Contains no audio, only hashes and metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunReceipt {
    pub schema: String,
    pub git_commit: String,
    pub created_utc: String,
    pub source: SourceReceipt,
    pub analyzers: Vec<AnalyzerRun>,
    pub artifact_sha256: String,
    pub warnings: Vec<String>,
}

impl RunReceipt {
    pub const SCHEMA: &'static str = "sai.receipt/v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        // NIST vector: SHA-256("abc").
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}

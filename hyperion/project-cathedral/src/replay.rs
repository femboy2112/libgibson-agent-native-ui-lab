//! Deterministic replay: `fixture + seed + action journal` must reconstruct an
//! equivalent semantic state.
//!
//! The record file stores the ordered journal plus the expected final semantic
//! digest and frame count. Replay rebuilds an `App` with no scripted scenario
//! (the scripted directives were journaled like any other action), applies the
//! journal at each frame, and compares digests. A mismatch names the first frame
//! at which the sim digest diverged.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::action::Journal;
use crate::app::App;
use crate::visual::Scale;

/// The on-disk receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordFile {
    pub app: String,
    pub version: String,
    pub libgibson_commit: String,
    pub journal: Journal,
    pub frames: u64,
    pub final_digest: String,
    pub final_frame: u32,
    /// Per-checkpoint digests, sampled every `checkpoint_every` frames.
    pub checkpoints: Vec<(u32, u64)>,
    pub checkpoint_every: u32,
    #[serde(default)]
    pub enable_music: bool,
    pub wav_sha256: Option<String>,
}

pub fn write_record(path: &str, rec: &RecordFile) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(rec).map_err(std::io::Error::other)?;
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, json)
}

pub fn read_record(path: &str) -> Result<RecordFile, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    serde_json::from_str(&s).map_err(|e| format!("parse {path}: {e}"))
}

/// Run a replay and compare against the record's receipts. Returns `Ok(())` iff the
/// reconstructed semantic state is equivalent and all checkpoints match.
pub fn run_replay(
    rec: &RecordFile,
    world: gibson::audio::human_music::world::WorldId,
) -> Result<String, String> {
    if rec.journal.fixture != "cathedral" {
        return Err(format!(
            "unknown fixture in journal: {}",
            rec.journal.fixture
        ));
    }
    let mut app = App::new(rec.journal.seed, world, false);
    // The music digest is part of the recorded digest, so replay must reproduce the
    // recorded music setting exactly, not the caller's current flags.
    app.enable_music = rec.enable_music;
    // Restore the journal verbatim; scenario is intentionally not run.
    app.journal.actions = rec.journal.actions.clone();

    let every = rec.checkpoint_every.max(1);
    let mut cps: Vec<(u32, u64)> = Vec::new();
    let mut mismatch: Option<(u32, u64, u64)> = None;
    for _ in 0..rec.frames {
        let _ = app.step();
        // Mirror the recorder exactly: sample *after* the step, keyed by the new
        // frame. (Sampling before the step while keying the old frame is an
        // off-by-one that makes every checkpoint look divergent.)
        if app.frame % every == 0 {
            cps.push((app.frame, app.engine.semantic_digest()));
        }
        if let Some((_cf, cd)) = rec.checkpoints.iter().find(|(f, _)| *f == app.frame) {
            let d = app.engine.semantic_digest();
            if d != *cd && mismatch.is_none() {
                mismatch = Some((app.frame, *cd, d));
            }
        }
        app.camera.update();
        if app.frame > rec.final_frame {
            break;
        }
    }
    if let Some((f, expected, got)) = mismatch {
        return Err(format!(
            "DIVERGENCE at frame {f}: recorded sim digest {:016x} != replayed {:016x}",
            expected, got
        ));
    }
    // Checkpoint sequence must contain the recorded ones exactly.
    for (f, d) in &rec.checkpoints {
        if let Some((_, got)) = cps.iter().find(|(cf, _)| cf == f) {
            if got != d {
                return Err(format!(
                    "checkpoint frame {f}: recorded {:016x} != replayed {:016x}",
                    d, got
                ));
            }
        } else {
            return Err(format!("checkpoint frame {f} was not sampled"));
        }
    }
    let final_sim = app.engine.semantic_digest();
    let expected_sim = rec.checkpoints.last().map(|(_, d)| *d).unwrap_or(0);
    let _ = expected_sim;
    let digest = app.digest();
    if rec.final_digest != digest {
        return Err(format!(
            "final digest mismatch:\n  recorded {}\n  replayed {}",
            rec.final_digest, digest
        ));
    }
    let _ = final_sim;
    Ok(format!(
        "REPLAY OK: {} frames, {} actions, {} checkpoints, final sim {:016x}\n  digest {}",
        rec.frames,
        rec.journal.actions.len(),
        rec.checkpoints.len(),
        final_sim,
        digest
    ))
}

/// Collect checkpoint digests while running a live app.
pub struct Checkpointer {
    pub every: u32,
    pub samples: Vec<(u32, u64)>,
}

impl Checkpointer {
    pub fn new(every: u32) -> Self {
        Checkpointer {
            every: every.max(1),
            samples: Vec::new(),
        }
    }

    pub fn sample(&mut self, app: &App) {
        if app.frame % self.every == 0 {
            self.samples.push((app.frame, app.engine.semantic_digest()));
        }
    }
}

/// The default scale used by headless/replay rendering.
pub fn default_scale() -> Scale {
    Scale::Whole
}

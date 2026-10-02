//! The canonical audio boundary: decode to a boring, inspectable representation.
//!
//! The authoritative canonicalization for analyzer inputs lives in
//! `adapters/canonicalize.py`, which is where real inference runs. This module provides the
//! same boundary in Rust for the harness (to verify generated WAVs) and a thin `ffmpeg`
//! wrapper for completeness. Raw-seconds coordinates at ingestion are authoritative; no
//! musical quantization happens here.

use std::path::Path;
use std::process::Command;

use crate::receipt::sha256_hex;
use crate::{SaiError, SaiResult};

/// Decoded audio, interleaved, at its native rate.
#[derive(Debug, Clone, PartialEq)]
pub struct WavAudio {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl WavAudio {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / self.sample_rate.max(1) as f64
    }

    /// Downmix to mono by averaging channels. Never hides that this happened: callers record
    /// the conversion in the source receipt.
    pub fn to_mono(&self) -> Vec<f32> {
        let ch = self.channels.max(1) as usize;
        let n = self.frames();
        let mut out = vec![0.0f32; n];
        for i in 0..n {
            let mut acc = 0.0f32;
            for c in 0..ch {
                acc += self.samples[i * ch + c];
            }
            out[i] = acc / ch as f32;
        }
        out
    }
}

fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Read a RIFF/WAVE file supporting 16-bit PCM and 32-bit IEEE float, any channel count.
pub fn read_wav(path: &Path) -> SaiResult<WavAudio> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SaiError::Audio("not a RIFF/WAVE file".into()));
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // format, channels, rate, bits
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = rd_u32(&bytes, pos + 4) as usize;
        let body = pos + 8;
        if body + size > bytes.len() {
            return Err(SaiError::Audio("truncated WAV chunk".into()));
        }
        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(SaiError::Audio("short fmt chunk".into()));
                }
                fmt = Some((
                    rd_u16(&bytes, body),
                    rd_u16(&bytes, body + 2),
                    rd_u32(&bytes, body + 4),
                    rd_u16(&bytes, body + 14),
                ));
            }
            b"data" => data = Some(&bytes[body..body + size]),
            _ => {}
        }
        pos = body + size + (size & 1);
    }
    let (format, channels, sample_rate, bits) =
        fmt.ok_or_else(|| SaiError::Audio("missing fmt chunk".into()))?;
    let data = data.ok_or_else(|| SaiError::Audio("missing data chunk".into()))?;
    if channels == 0 {
        return Err(SaiError::Audio("zero channels".into()));
    }
    let samples: Vec<f32> = match (format, bits) {
        (1, 16) => data
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect(),
        (3, 32) => data
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        _ => {
            return Err(SaiError::Audio(format!(
                "unsupported WAV format {format} / {bits} bits"
            )))
        }
    };
    if samples.iter().any(|s| !s.is_finite()) {
        return Err(SaiError::NonFinite("wav sample"));
    }
    Ok(WavAudio {
        samples,
        channels,
        sample_rate,
    })
}

/// Write interleaved f32 samples as a 16-bit PCM WAV (defensive clamp).
pub fn write_wav_i16(
    path: &Path,
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
) -> SaiResult<()> {
    use std::io::Write;
    if channels == 0 || sample_rate == 0 {
        return Err(SaiError::Audio("invalid WAV layout".into()));
    }
    let data_len = (samples.len() * 2) as u32;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data_len).to_le_bytes())?;
    out.write_all(b"WAVE")?;
    out.write_all(b"fmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?; // PCM
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * channels as u32 * 2;
    out.write_all(&byte_rate.to_le_bytes())?;
    out.write_all(&(channels * 2).to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data_len.to_le_bytes())?;
    for &s in samples {
        let c = s.clamp(-1.0, 1.0);
        let q = (c * i16::MAX as f32).round() as i16;
        out.write_all(&q.to_le_bytes())?;
    }
    out.flush()?;
    Ok(())
}

/// Canonical decode to interleaved f32le via `ffmpeg`, returning the PCM and its hash.
///
/// This deliberately shells out to the same inspectable path LibGibson's `media.rs` uses.
/// The command line is recorded by the caller; this function never writes into the repo.
pub fn canonicalize_ffmpeg(
    clip: &Path,
    target_sr: u32,
    target_channels: u16,
) -> SaiResult<(Vec<f32>, String)> {
    if target_sr == 0 || target_channels == 0 {
        return Err(SaiError::Audio("invalid canonical target".into()));
    }
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(clip)
        .args([
            "-vn",
            "-ac",
            &target_channels.to_string(),
            "-ar",
            &target_sr.to_string(),
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-",
        ])
        .output()
        .map_err(|e| SaiError::Audio(format!("ffmpeg spawn failed: {e}")))?;
    if !out.status.success() {
        return Err(SaiError::Audio(format!(
            "ffmpeg decode failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let raw = out.stdout;
    let mut samples = Vec::with_capacity(raw.len() / 4);
    for b in raw.chunks_exact(4) {
        let s = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if !s.is_finite() {
            return Err(SaiError::NonFinite("ffmpeg output sample"));
        }
        samples.push(s);
    }
    if samples.is_empty() {
        return Err(SaiError::Audio("ffmpeg produced no samples".into()));
    }
    let hash = sha256_hex(&raw);
    Ok((samples, hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trips_i16() {
        let dir = std::env::temp_dir().join("sai-core-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("tone.wav");
        let sr = 8000u32;
        let n = 400usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 0.1).sin() * 0.5)
            .flat_map(|s| [s, s]) // stereo
            .collect();
        write_wav_i16(&p, &samples, 2, sr).unwrap();
        let a = read_wav(&p).unwrap();
        assert_eq!(a.channels, 2);
        assert_eq!(a.sample_rate, sr);
        assert_eq!(a.frames(), n);
        let mono = a.to_mono();
        for (x, y) in mono.iter().zip(samples.chunks(2).map(|c| c[0])) {
            assert!((x - y).abs() < 1e-3);
        }
    }

    #[test]
    fn rejecting_garbage_is_fail_closed() {
        let dir = std::env::temp_dir().join("sai-core-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("garbage.wav");
        std::fs::write(&p, b"not a wav at all").unwrap();
        assert!(read_wav(&p).is_err());
    }
}

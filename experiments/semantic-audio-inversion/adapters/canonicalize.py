"""Canonical audio boundary for the inverse analyzer.

The analyzer never touches a container twice. This module performs **one** decode to a boring,
inspectable representation — mono, 22.05 kHz, interleaved f32 — and records both the source hash
(what the host supplied) and the canonical-PCM hash (what every downstream reading actually saw).
Conflating those two would hide a resample or a channel downmix, so both are kept.

The decode path is `ffmpeg`, the same inspectable tool the forward map uses; the exact version is
recorded in the source receipt. If `ffmpeg`/`ffprobe` are absent the boundary **refuses** rather
than silently substituting a different decoder (no silent fallback).
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from typing import Optional

import numpy as np

CANONICAL_SR = 22050
CANONICAL_CHANNELS = 1


class CanonicalizeError(RuntimeError):
    """Raised when the canonical boundary cannot be honored."""


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _require(tool: str) -> str:
    p = shutil.which(tool)
    if p is None:
        raise CanonicalizeError(f"required decoder '{tool}' not found on PATH")
    return p


def _ffmpeg_version(ffmpeg: str) -> str:
    try:
        out = subprocess.run(
            [ffmpeg, "-version"], capture_output=True, text=True, timeout=20
        )
        first = out.stdout.splitlines()[0] if out.stdout else "ffmpeg"
        return first.strip()
    except Exception:  # pragma: no cover - version is cosmetic, decode is not
        return "ffmpeg (version unknown)"


def probe(path: str) -> dict:
    """Read native stream metadata via ffprobe. Fails closed if it cannot be read."""
    ffprobe = _require("ffprobe")
    out = subprocess.run(
        [
            ffprobe,
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels,codec_name:format=duration,format_name",
            "-of",
            "json",
            path,
        ],
        capture_output=True,
        text=True,
        timeout=120,
    )
    if out.returncode != 0:
        raise CanonicalizeError(f"ffprobe failed on {path!r}: {out.stderr.strip()}")
    meta = json.loads(out.stdout or "{}")
    streams = meta.get("streams") or []
    if not streams:
        raise CanonicalizeError(f"{path!r} has no audio stream")
    s = streams[0]
    fmt = meta.get("format") or {}
    return {
        "sample_rate": int(s.get("sample_rate", 0)),
        "channels": int(s.get("channels", 0)),
        "codec": str(s.get("codec_name", "unknown")),
        "duration": float(fmt.get("duration", 0.0) or 0.0),
        "container": str(fmt.get("format_name", "unknown")),
    }


@dataclass
class CanonicalAudio:
    """The single authoritative view of a source the analyzer reasons over."""

    samples: np.ndarray  # mono float32
    sample_rate: int
    duration_seconds: float
    source_sha256: str
    canonical_pcm_sha256: str
    native_sample_rate: int
    native_channels: int
    native_codec: str
    native_container: str
    decoder: str
    resampled: bool
    channel_conversion: str

    def receipt_fields(self, path_hint: Optional[str] = None) -> dict:
        """The `SourceReceipt` body (field names mirror `sai-core::evidence`)."""
        return {
            "sha256": self.source_sha256,
            "path_hint": path_hint,
            "duration_seconds": float(self.duration_seconds),
            "sample_rate_hz": int(self.native_sample_rate),
            "channels": int(self.native_channels),
            "decoder": self.decoder,
            "canonical_format": "f32le",
            "canonical_sample_rate_hz": int(self.sample_rate),
            "canonical_channels": int(CANONICAL_CHANNELS),
            "resampled": bool(self.resampled),
            "channel_conversion": self.channel_conversion,
            "canonical_pcm_sha256": self.canonical_pcm_sha256,
            "license": "generated fixture" if path_hint is None else f"local:{path_hint}",
        }


def canonicalize(path: str, target_sr: int = CANONICAL_SR) -> CanonicalAudio:
    """Decode `path` to mono f32le at `target_sr` using ffmpeg.

    Records source hash, canonical-PCM hash, and the conversion facts. Raises
    [`CanonicalizeError`] if the boundary cannot be honored.
    """
    if not os.path.isfile(path):
        raise CanonicalizeError(f"no such file: {path!r}")
    ffmpeg = _require("ffmpeg")
    meta = probe(path)
    if meta["sample_rate"] <= 0 or meta["channels"] <= 0:
        raise CanonicalizeError(f"invalid native audio metadata for {path!r}: {meta}")

    cmd = [
        ffmpeg,
        "-v",
        "error",
        "-i",
        path,
        "-vn",
        "-ac",
        str(CANONICAL_CHANNELS),
        "-ar",
        str(target_sr),
        "-f",
        "f32le",
        "-acodec",
        "pcm_f32le",
        "-",
    ]
    proc = subprocess.run(cmd, capture_output=True, timeout=600)
    if proc.returncode != 0:
        raise CanonicalizeError(
            f"ffmpeg decode failed ({proc.returncode}): {proc.stderr.decode(errors='replace').strip()}"
        )
    raw = proc.stdout
    if not raw:
        raise CanonicalizeError(f"ffmpeg produced no samples for {path!r}")
    samples = np.frombuffer(raw, dtype="<f4")
    if samples.size == 0 or not np.all(np.isfinite(samples)):
        raise CanonicalizeError(f"canonical PCM for {path!r} is empty or non-finite")
    samples = np.ascontiguousarray(samples.astype(np.float32, copy=False))
    duration = float(samples.size) / float(target_sr)

    return CanonicalAudio(
        samples=samples,
        sample_rate=int(target_sr),
        duration_seconds=duration,
        source_sha256=sha256_file(path),
        canonical_pcm_sha256=sha256_bytes(raw),
        native_sample_rate=int(meta["sample_rate"]),
        native_channels=int(meta["channels"]),
        native_codec=meta["codec"],
        native_container=meta["container"],
        decoder=f"{_ffmpeg_version(ffmpeg)}; ffprobe-probed {meta['container']}/{meta['codec']}",
        resampled=(int(meta["sample_rate"]) != int(target_sr)),
        channel_conversion="mono-downmix" if int(meta["channels"]) > 1 else "none",
    )

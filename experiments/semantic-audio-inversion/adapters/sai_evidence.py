"""Neutral evidence builders for `sai.evidence/v1`.

Every measurement instrument (our inspectable DSP baseline, or an external model behind an
adapter) emits this shape. The builders exist so field names cannot drift from the Rust IR.
Nothing here knows about `CoverMap`: it only records observed/derived coordinates, provenance,
and explicit unknown/refusal.
"""

from __future__ import annotations

import json
from typing import Any, Iterable, Optional, Sequence

SCHEMA = "sai.evidence/v1"
RECEIPT_SCHEMA = "sai.receipt/v1"

# Evidence levels (Rust serde rename_all = "kebab-case").
ACOUSTIC = "acoustic-observation"
DERIVED_EVENT = "derived-musical-event"
DERIVED_STRUCTURAL = "derived-structural-interpretation"


def provenance(
    analyzer_id: str,
    analyzer_version: str,
    level: str,
    method: str,
    derived_from: Sequence[str] = ("pcm",),
) -> dict:
    return {
        "analyzer_id": analyzer_id,
        "analyzer_version": analyzer_version,
        "level": level,
        "method": method,
        "derived_from": list(derived_from),
    }


def analyzer_run(
    id: str,
    version: str,
    config: Any = None,
    wall_seconds: Optional[float] = None,
    weights: Optional[str] = None,
    cached: bool = False,
) -> dict:
    return {
        "id": id,
        "version": version,
        "config": config if config is not None else {},
        "wall_seconds": wall_seconds,
        "weights": weights,
        "cached": cached,
    }


def source_receipt(
    sha256: str,
    duration_seconds: float,
    sample_rate_hz: int,
    channels: int,
    decoder: str,
    canonical_sample_rate_hz: int,
    canonical_channels: int,
    resampled: bool,
    channel_conversion: str,
    canonical_pcm_sha256: Optional[str],
    path_hint: Optional[str] = None,
    canonical_format: str = "f32le",
    license: str = "generated fixture",
) -> dict:
    return {
        "sha256": sha256,
        "path_hint": path_hint,
        "duration_seconds": float(duration_seconds),
        "sample_rate_hz": int(sample_rate_hz),
        "channels": int(channels),
        "decoder": decoder,
        "canonical_format": canonical_format,
        "canonical_sample_rate_hz": int(canonical_sample_rate_hz),
        "canonical_channels": int(canonical_channels),
        "resampled": bool(resampled),
        "channel_conversion": channel_conversion,
        "canonical_pcm_sha256": canonical_pcm_sha256,
        "license": license,
    }


def beat(second: float, confidence: float, is_downbeat: bool = False) -> dict:
    return {"second": float(second), "confidence": float(confidence), "is_downbeat": bool(is_downbeat)}


def timing(
    prov: dict,
    tempo_bpm: Optional[float],
    beats: Sequence[dict] = (),
    downbeats: Sequence[float] = (),
    tempo_map: Sequence[dict] = (),
    meter_hypotheses: Sequence[dict] = (),
    ambiguity: Sequence[str] = (),
) -> dict:
    return {
        "provenance": prov,
        "tempo_bpm": None if tempo_bpm is None else float(tempo_bpm),
        "tempo_map": list(tempo_map),
        "beats": list(beats),
        "downbeats": list(downbeats),
        "meter_hypotheses": list(meter_hypotheses),
        "ambiguity": list(ambiguity),
    }


def note(
    onset_second: float,
    pitch_midi: float,
    confidence: float,
    prov: dict,
    offset_second: Optional[float] = None,
    roles: Sequence[tuple] = (),
) -> dict:
    return {
        "onset_second": float(onset_second),
        "offset_second": None if offset_second is None else float(offset_second),
        "pitch_midi": float(pitch_midi),
        "role_candidates": [{"role": r, "confidence": float(c)} for r, c in roles],
        "confidence": float(confidence),
        "provenance": prov,
    }


def chroma_frame(center_second: float, hop_seconds: float, values: Sequence[float]) -> dict:
    if len(values) != 12:
        raise ValueError("chroma frame must have 12 bins")
    return {"center_second": float(center_second), "hop_seconds": float(hop_seconds), "values": [float(v) for v in values]}


def key_candidate(tonic_pc: int, mode: str, confidence: float) -> dict:
    if mode not in ("major", "minor"):
        raise ValueError("mode must be major or minor")
    return {"tonic_pc": int(tonic_pc), "mode": mode, "confidence": float(confidence)}


def chord_candidate(at_second: float, end_second: float, root_pc: int, quality: str, confidence: float, inversion: str = "unknown") -> dict:
    return {
        "at_second": float(at_second),
        "end_second": float(end_second),
        "root_pc": int(root_pc),
        "quality": quality,
        "inversion": inversion,
        "confidence": float(confidence),
    }


def tonal(
    prov: dict,
    chroma_frames: Sequence[dict] = (),
    key_candidates: Sequence[dict] = (),
    chord_candidates: Sequence[dict] = (),
) -> dict:
    return {
        "provenance": prov,
        "chroma_frames": list(chroma_frames),
        "key_candidates": list(key_candidates),
        "chord_candidates": list(chord_candidates),
    }


def onset(second: float, strength: float, prov: dict, families: Sequence[tuple] = ()) -> dict:
    return {
        "second": float(second),
        "strength": float(strength),
        "family_candidates": [{"family": f, "confidence": float(c)} for f, c in families],
        "provenance": prov,
    }


def recurrence(prov: dict, links: Sequence[tuple] = (), summary: str = "") -> dict:
    return {
        "provenance": prov,
        "links": [{"at_second": float(a), "to_second": float(b), "similarity": float(s)} for a, b, s in links],
        "summary": summary,
    }


def sections(prov: dict, boundaries: Sequence[float] = (), spans: Sequence[tuple] = ()) -> dict:
    return {
        "provenance": prov,
        "boundaries": [float(b) for b in boundaries],
        "sections": [
            {"start_second": float(s), "end_second": float(e), "label_candidate": None}
            for s, e in spans
        ],
    }


def refusal(what: str, reason: str) -> dict:
    return {"what": what, "reason": reason}


def artifact(
    source: dict,
    analyzers: Sequence[dict] = (),
    timing_ev: Optional[dict] = None,
    notes: Sequence[dict] = (),
    tonal_ev: Optional[dict] = None,
    onsets: Sequence[dict] = (),
    recurrence_ev: Optional[dict] = None,
    sections_ev: Optional[dict] = None,
    refusals: Sequence[dict] = (),
    unknowns: Sequence[str] = (),
) -> dict:
    return {
        "schema": SCHEMA,
        "source": source,
        "analyzers": list(analyzers),
        "timing": timing_ev,
        "notes": list(notes),
        "tonal": tonal_ev,
        "onsets": list(onsets),
        "recurrence": recurrence_ev,
        "sections": sections_ev,
        "refusals": list(refusals),
        "unknowns": list(unknowns),
    }


def write_json(path: str, obj: Any) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=2, sort_keys=False)
        f.write("\n")


def receipt(
    source: dict,
    analyzers: Sequence[dict],
    artifact_sha256: str,
    git_commit: str,
    created_utc: str,
    warnings: Iterable[str] = (),
) -> dict:
    return {
        "schema": RECEIPT_SCHEMA,
        "git_commit": git_commit,
        "created_utc": created_utc,
        "source": source,
        "analyzers": list(analyzers),
        "artifact_sha256": artifact_sha256,
        "warnings": list(warnings),
    }

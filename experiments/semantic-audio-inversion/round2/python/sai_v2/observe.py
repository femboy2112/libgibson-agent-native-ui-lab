"""PCM-only observer. No score, source blueprint or target pitches enter this module.

The frozen v1 DSP is one instrument. A separately recorded YIN/energy route is an
additive monophonic instrument, not independent provenance from DeepSeek's YIN.
All events here are in seconds; metric transport belongs to the evaluator.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import scipy
from scipy import signal
import soundfile as sf
import librosa

ADAPTERS = Path(__file__).resolve().parents[3] / "adapters"
sys.path.insert(0, str(ADAPTERS))
from baseline_dsp import BaselineDSP, DspConfig  # noqa: E402
from canonicalize import canonicalize, sha256_file  # noqa: E402

VERSION = "sai.pcm_observation/v2.1"
TEMPLATES = {
    "maj": [0, 4, 7], "min": [0, 3, 7], "dim": [0, 3, 6],
    "aug": [0, 4, 8], "sus4": [0, 5, 7], "dom7": [0, 4, 7, 10],
    "maj7": [0, 4, 7, 11], "min7": [0, 3, 7, 10],
}


def write_json(path, obj):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=2, sort_keys=True, allow_nan=False) + "\n")


def production_features(path):
    y, sr = sf.read(path, always_2d=True)
    if not np.isfinite(y).all() or not len(y):
        raise ValueError("invalid PCM")
    mono = y.mean(axis=1)
    rms = float(np.sqrt(np.mean(y*y)))
    peak = float(np.max(np.abs(y)))
    freq, psd = signal.welch(mono, sr, nperseg=min(4096, len(mono)))
    power = float(psd.sum())
    bands = [float(psd[(freq >= lo) & (freq < hi)].sum() / max(power, 1e-20))
             for lo, hi in [(20, 250), (250, 2500), (2500, sr/2+1)]]
    mid = (y[:, 0]+y[:, -1])/2
    side = (y[:, 0]-y[:, -1])/2
    return {
        "rms_db": float(20*np.log10(max(rms, 1e-10))),
        "peak_db": float(20*np.log10(max(peak, 1e-10))),
        "crest_db": float(20*np.log10(max(peak, 1e-10)/max(rms, 1e-10))),
        "centroid_hz": float(np.dot(freq, psd)/max(power, 1e-20)),
        "bands_low_mid_high": bands,
        "width": float(np.sqrt(np.mean(side*side)/max(float(np.mean(mid*mid)), 1e-20))),
        "pan": float((np.mean(y[:, -1]**2)-np.mean(y[:, 0]**2))/max(2*rms*rms, 1e-20)),
        "duration_seconds": len(y)/sr, "sample_rate_hz": sr,
    }


def yin_events(x, sr, role):
    """Fixed role ranges; energy gate and stable pitch runs; no reference-note lookup."""
    if not len(x) or np.max(np.abs(x)) < 1e-6:
        return []
    lo, hi = (32.7, 392.0) if role == "bass" else (82.4, 2093.0)
    hop, frame = 256, 2048
    f0 = librosa.yin(x, sr=sr, fmin=lo, fmax=hi, frame_length=frame,
                     hop_length=hop)
    rms = librosa.feature.rms(y=x, frame_length=frame, hop_length=hop)[0]
    midi = librosa.hz_to_midi(f0)
    # Frame centered energy gates tails below -30 dB of this stem's 95th percentile.
    active = rms > max(1e-5, float(np.quantile(rms, .95)) * .0316)
    events, start, values = [], None, []

    def close(i):
        if start is not None and (i-start)*hop/sr >= .07:
            events.append({"onset_second": start*hop/sr,
                           "offset_second": min(i*hop/sr, len(x)/sr),
                           "pitch_midi": float(np.median(values)),
                           "confidence": None, "role": role,
                           "route": "yin-energy-stable-runs/v2"})
    for i, p in enumerate(midi):
        good = active[i] and np.isfinite(p)
        if start is not None and (not good or abs(p-float(np.median(values[-12:]))) > .7):
            close(i)
            start, values = None, []
        if good:
            if start is None:
                start = i
            values.append(float(p))
    close(len(midi))
    return events


def harmonic_windows(dsp):
    frames, values, centers = dsp.chroma()
    centers = np.asarray(centers)
    out = []
    templates = []
    for root in range(12):
        for quality, intervals in TEMPLATES.items():
            t = np.zeros(12)
            t[(root+np.asarray(intervals)) % 12] = 1
            templates.append((root, quality, t/np.linalg.norm(t)))
    for start in np.arange(0, dsp.duration, .25):
        end = min(float(start+.25), dsp.duration)
        selected = values[(centers >= start) & (centers < end)]
        if not len(selected) or not np.any(selected):
            continue
        c = selected.mean(axis=0)
        ranks = sorted([(float(np.dot(c, t)/max(np.linalg.norm(c), 1e-12)), r, q)
                        for r, q, t in templates], reverse=True)
        score, root, quality = ranks[0]
        out.append({"start_second": float(start), "end_second": end,
                    "root_pc": root, "quality": quality, "support": score,
                    "rivals": [{"root_pc": r, "quality": q, "support": s}
                               for s, r, q in ranks[1:3]]})
    return out


def observe(path, role="unknown", *, frozen=False):
    audio = canonicalize(str(path))
    dsp = BaselineDSP(audio.samples, audio.sample_rate, DspConfig())
    onsets, _, cross = dsp.detect_onsets()
    timing, timing_meta = dsp.timing_and_beats(onsets) if role in ("full", "unknown") else ({}, {})
    notes, note_meta = dsp.transcribe(onsets) if frozen else ([], {})
    drums, drum_meta = dsp.percussion(onsets) if role in ("full", "drums", "unknown") else ([], {})
    sections, recurrence = None, None
    if role == "full":
        _, chroma, centers = dsp.chroma()
        sections, recurrence, _ = dsp.sections_and_recurrence(
            [b["second"] for b in timing.get("beats", [])], chroma, centers)
    return {
        "schema": VERSION, "source": audio.receipt_fields(), "role_surface": role,
        "instrument": {"module_sha256": sha256_file(__file__),
                       "baseline_sha256": sha256_file(ADAPTERS / "baseline_dsp.py"),
                       "numpy": np.__version__, "scipy": scipy.__version__,
                       "librosa": librosa.__version__, "config": DspConfig().as_json()},
        "timing": timing, "timing_meta": timing_meta,
        "frozen_notes": notes, "frozen_notes_meta": note_meta,
        "notes": yin_events(audio.samples, audio.sample_rate, role)
                 if role in ("lead", "bass", "full") else [],
        "harmony": harmonic_windows(dsp) if role in ("full", "keys", "pad", "support") else [],
        "drums": drums, "drums_meta": drum_meta, "onset_crosscheck": cross,
        "sections": sections, "recurrence": recurrence,
        "production": production_features(path),
        "boundary": "PCM-only; monophonic YIN is inapplicable to polyphonic truth; no claimed lyric or artist inference",
    }


def cached_observe(path, out, role="unknown", *, frozen=False):
    out = Path(out)
    key = {"wav": sha256_file(path), "module": sha256_file(__file__),
           "baseline": sha256_file(ADAPTERS / "baseline_dsp.py"),
           "role": role, "frozen": frozen, "librosa": librosa.__version__,
           "numpy": np.__version__, "scipy": scipy.__version__}
    if out.exists():
        old = json.loads(out.read_text())
        if old.get("cache_key") == key:
            return old
    obj = observe(path, role, frozen=frozen)
    obj["cache_key"] = key
    write_json(out, obj)
    return obj

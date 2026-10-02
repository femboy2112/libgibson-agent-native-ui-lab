"""Synthesize calibration fixtures with known ground truth.

The Instrument Rule: code that measures math/music is a scientific instrument, so before any
reading on an unknown is believed, the instrument is run on cases where the answer is KNOWN.
These fixtures are generated deterministically (fixed seeds) and each carries a declared expected
reading in `scripts/calibrate.py`.

Controls:
- positive: pure tone, major triad, 120 BPM click train, kick/snare metronome;
- null: silence (no evidence; must not crash, must not invent);
- negative: white noise, frequency sweep (must not assert a confident key/pitch);
- mutation: transposed triad (relative structure must survive, absolute roots shift).
"""

from __future__ import annotations

import argparse
import os
import struct
import wave

import numpy as np

SR = 22050


def write_wav(path: str, x: np.ndarray, sr: int = SR) -> None:
    x = np.clip(np.asarray(x, dtype=np.float64), -1.0, 1.0)
    pcm = (x * 32767.0).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(pcm.tobytes())


def tone(freq: float, dur: float, sr: int = SR, amp: float = 0.5) -> np.ndarray:
    t = np.arange(int(dur * sr)) / sr
    # Short fade in/out so the boundary does not ring as a spurious transient.
    env = np.ones_like(t)
    f = min(256, t.size // 4)
    if f > 0:
        env[:f] = np.linspace(0, 1, f)
        env[-f:] = np.linspace(1, 0, f)
    return amp * env * np.sin(2 * np.pi * freq * t)


def triad(root_hz: float, dur: float, sr: int = SR) -> np.ndarray:
    # Just-intonation-ish major triad: 1, 5/4, 3/2.
    return sum(tone(root_hz * r, dur, sr, 0.28) for r in (1.0, 1.25, 1.5))


def click_train(bpm: float, dur: float, sr: int = SR) -> np.ndarray:
    x = np.zeros(int(dur * sr))
    period = 60.0 / bpm
    n = int(dur / period)
    for k in range(n):
        i = int(round(k * period * sr))
        seg = np.zeros(64)
        seg[:16] = np.hanning(16) * 0.9
        end = min(x.size, i + seg.size)
        x[i:end] += seg[: end - i]
    return x


def kick_snare(bpm: float, dur: float, sr: int = SR) -> np.ndarray:
    x = np.zeros(int(dur * sr))
    period = 60.0 / bpm
    rng = np.random.default_rng(1234)
    n = int(dur / period)
    for k in range(n):
        i = int(round(k * period * sr))
        if k % 2 == 0:  # on-beat kick
            tt = np.arange(2048) / sr
            seg = np.exp(-tt * 18.0) * np.sin(2 * np.pi * 55.0 * tt) * 0.9
        else:  # off-beat snare: noise burst
            seg = rng.standard_normal(2048) * np.exp(-np.arange(2048) / sr * 22.0) * 0.35
        end = min(x.size, i + seg.size)
        x[i:end] += seg[: end - i]
    return x


def white_noise(dur: float, sr: int = SR) -> np.ndarray:
    rng = np.random.default_rng(99)
    return 0.2 * rng.standard_normal(int(dur * sr))


def sweep(dur: float, sr: int = SR) -> np.ndarray:
    t = np.arange(int(dur * sr)) / sr
    f0, f1 = 200.0, 2000.0
    phase = 2 * np.pi * (f0 * t + (f1 - f0) * t * t / (2 * dur))
    return 0.4 * np.sin(phase)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fixtures"))
    args = ap.parse_args()
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    fixtures = {
        "positive_tone_a4.wav": tone(440.0, 2.0),
        "positive_triad_c_maj.wav": triad(261.6256, 2.0),
        "positive_click_120.wav": click_train(120.0, 8.0),
        "positive_kicksnare_120.wav": kick_snare(120.0, 8.0),
        "null_silence.wav": np.zeros(SR * 2),
        "negative_white_noise.wav": white_noise(2.0),
        "negative_sweep.wav": sweep(3.0),
        "mutation_triad_d_maj.wav": triad(293.6648, 2.0),
    }
    for name, x in fixtures.items():
        write_wav(os.path.join(out, name), x)
        print(f"wrote {name} ({x.size / SR:.2f}s)")
    print(f"wrote {len(fixtures)} fixtures to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

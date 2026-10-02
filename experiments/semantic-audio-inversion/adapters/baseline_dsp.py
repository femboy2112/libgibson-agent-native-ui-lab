"""Inspectable DSP baseline: the primary measurement instrument for this round.

This file contains no learned weights and no opaque model. Every reading is a named signal
operation, so the evidence it emits can be audited and the failure mode can be reasoned about.
That is the point: an external model, when one is available, is measured *against* this, not
trusted instead of it.

Routes and their cross-checks (the protocol's "two independent bearings" discipline):

- **onset**   log-spectral-flux (route A) vs short-time energy novelty (route B). Disagreements
  are recorded, not averaged.
- **tempo**   autocorrelation period (route A) vs inter-onset-interval histogram (route B).
  Rival hypotheses are kept in `timing.ambiguity`.
- **pitch**   harmonic-product-spectrum (route A) vs autocorrelation (route B). Agreement
  raises confidence; an octave-only agreement is flagged as ambiguous.
- **harmony**  the Rust core triangulates chroma-template vs note-window; this file supplies
  the chroma and the notes, never a fused answer.
- **form**    novelty boundaries (route A) vs section-chroma recurrence clustering (route B).

`Unknown` is a first-class output: a reading that cannot be made with confidence is dropped or
explicitly refused, never replaced by a default.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Sequence, Tuple

import numpy as np

from sai_evidence import (
    ACOUSTIC,
    DERIVED_EVENT,
    DERIVED_STRUCTURAL,
    analyzer_run,
    beat,
    chroma_frame,
    key_candidate,
    note,
    onset,
    provenance,
    recurrence,
    sections,
    timing,
    tonal,
)

ANALYZER_ID = "sai-baseline-dsp"
ANALYZER_VERSION = "0.1.0"


@dataclass
class DspConfig:
    """All tunables in one place, recorded in the artifact's analyzer config."""

    onset_n_fft: int = 1024
    onset_hop: int = 256
    onset_threshold_delta: float = 0.35
    onset_median_window: int = 8
    tempo_min_bpm: float = 55.0
    tempo_max_bpm: float = 210.0
    tempo_prior_bpm: float = 120.0
    tempo_prior_octaves: float = 0.60
    chroma_n_fft: int = 4096
    chroma_hop: int = 1024
    chroma_min_hz: float = 55.0
    chroma_max_hz: float = 2093.0
    pitch_min_hz: float = 55.0
    pitch_max_hz: float = 1200.0
    note_window: int = 2048
    note_min_confidence: float = 0.30
    note_max_duration: float = 1.5
    meter_candidates: Tuple[int, ...] = (4, 3)
    section_kernel_beats: int = 8
    novelty_sigma: float = 1.0
    recurrence_floor: float = 0.85

    def as_json(self) -> dict:
        return {
            "onset_n_fft": self.onset_n_fft,
            "onset_hop": self.onset_hop,
            "onset_threshold_delta": self.onset_threshold_delta,
            "onset_median_window": self.onset_median_window,
            "tempo_min_bpm": self.tempo_min_bpm,
            "tempo_max_bpm": self.tempo_max_bpm,
            "tempo_prior_bpm": self.tempo_prior_bpm,
            "tempo_prior_octaves": self.tempo_prior_octaves,
            "chroma_n_fft": self.chroma_n_fft,
            "chroma_hop": self.chroma_hop,
            "chroma_min_hz": self.chroma_min_hz,
            "chroma_max_hz": self.chroma_max_hz,
            "pitch_min_hz": self.pitch_min_hz,
            "pitch_max_hz": self.pitch_max_hz,
            "note_window": self.note_window,
            "note_min_confidence": self.note_min_confidence,
            "note_max_duration": self.note_max_duration,
            "meter_candidates": list(self.meter_candidates),
            "section_kernel_beats": self.section_kernel_beats,
            "novelty_sigma": self.novelty_sigma,
            "recurrence_floor": self.recurrence_floor,
        }


def _frames(x: np.ndarray, n_fft: int, hop: int) -> np.ndarray:
    if x.size < n_fft:
        x = np.pad(x, (0, n_fft - x.size))
    view = np.lib.stride_tricks.sliding_window_view(x, n_fft)[::hop]
    return np.ascontiguousarray(view, dtype=np.float64)


def _rfft_mag(frames: np.ndarray, window: np.ndarray) -> np.ndarray:
    return np.abs(np.fft.rfft(frames * window[None, :], axis=1))


def _local_maxima(env: np.ndarray) -> np.ndarray:
    if env.size < 3:
        return np.empty(0, dtype=int)
    return np.where((env[1:-1] > env[:-2]) & (env[1:-1] >= env[2:]))[0] + 1


def _norm01(v: np.ndarray) -> np.ndarray:
    if v.size == 0:
        return v
    lo = float(np.min(v))
    hi = float(np.max(v))
    if hi - lo < 1e-12:
        return np.zeros_like(v)
    return (v - lo) / (hi - lo)


KRUMHANSL_MAJOR = np.array(
    [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88]
)
KRUMHANSL_MINOR = np.array(
    [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17]
)


class BaselineDSP:
    """Deterministic, inspectable readings over one canonical mono signal."""

    def __init__(self, samples: np.ndarray, sample_rate: int, config: Optional[DspConfig] = None):
        self.cfg = config or DspConfig()
        x = np.asarray(samples, dtype=np.float64)
        if x.ndim != 1:
            raise ValueError("BaselineDSP expects mono samples")
        if not np.all(np.isfinite(x)):
            raise ValueError("non-finite samples reached the analyzer")
        self.x = x
        self.sr = int(sample_rate)
        self.duration = x.size / float(self.sr)
        self.hop_fps = self.sr / float(self.cfg.onset_hop)
        self._cache: dict = {}

    # --- route A/B: onset ---------------------------------------------------

    def _onset_envelopes(self) -> Tuple[np.ndarray, np.ndarray, float]:
        """Return (spectral-flux route A, energy-novelty route B, frame_seconds)."""
        if "onset" in self._cache:
            return self._cache["onset"]
        cfg = self.cfg
        frames = _frames(self.x, cfg.onset_n_fft, cfg.onset_hop)
        window = np.hanning(cfg.onset_n_fft)
        mag = _rfft_mag(frames, window)
        logmag = 20.0 * np.log10(mag + 1e-10)
        flux = np.sum(np.maximum(0.0, np.diff(logmag, axis=0)), axis=1)
        flux = np.concatenate([[0.0], flux]) if flux.size else np.zeros(1)
        energy = np.sum(frames * frames, axis=1)
        energy = np.sqrt(np.maximum(energy, 0.0))
        novelty = np.concatenate([[0.0], np.maximum(0.0, energy[1:] - energy[:-1])])
        frame_seconds = (np.arange(frames.shape[0]) * cfg.onset_hop + cfg.onset_n_fft / 2.0) / self.sr
        # Smooth both routes with the same short kernel so peaks are comparable.
        k = np.ones(3) / 3.0
        flux = np.convolve(flux, k, mode="same")
        novelty = np.convolve(novelty, k, mode="same")
        self._cache["onset"] = (flux, novelty, frame_seconds)
        return self._cache["onset"]

    def _pick_peaks(self, env: np.ndarray, delta_scale: float) -> np.ndarray:
        w = max(1, self.cfg.onset_median_window)
        if env.size < 3:
            return np.empty(0, dtype=int)
        # Moving median-ish baseline via a boxcar mean (cheap, inspectable) on both sides.
        pad = np.pad(env, (w, w), mode="edge")
        baseline = np.array([pad[i : i + 2 * w + 1].mean() for i in range(env.size)])
        thr = baseline + delta_scale * (float(np.std(env)) + 1e-12)
        cand = _local_maxima(env)
        if cand.size == 0:
            return cand
        return cand[env[cand] > thr[cand]]

    def detect_onsets(self) -> Tuple[List[dict], List[dict]]:
        flux, novelty, sec = self._onset_envelopes()
        cfg = self.cfg
        f_norm = _norm01(flux)
        n_norm = _norm01(novelty)
        peaks_a = self._pick_peaks(f_norm, cfg.onset_threshold_delta)
        peaks_b = self._pick_peaks(n_norm, cfg.onset_threshold_delta)
        prov_a = provenance(ANALYZER_ID, ANALYZER_VERSION, ACOUSTIC, "spectral-flux-onset/v1")
        prov_b = provenance(ANALYZER_ID, ANALYZER_VERSION, ACOUSTIC, "energy-novelty-onset/v1")
        # Cross-route agreement recorded in the frame grid (1 frame = onset_hop).
        agree = 0
        for p in peaks_a:
            if peaks_b.size and int(np.min(np.abs(peaks_b - p))) <= 1:
                agree += 1
        cross = {
            "onset_route_a": int(peaks_a.size),
            "onset_route_b": int(peaks_b.size),
            "onset_agree_within_1_frame": int(agree),
        }
        events = []
        for p in peaks_a:
            t = float(min(sec[p], self.duration))
            strength = float(np.clip(f_norm[p], 0.0, 1.0))
            events.append({"second": t, "strength": strength, "prov": prov_a})
        return events, [prov_b], cross  # type: ignore[return-value]

    # --- route A/B: tempo + beat -------------------------------------------

    def _tempo_autocorrelation(self, env: np.ndarray) -> Tuple[float, float]:
        cfg = self.cfg
        e = env - float(np.mean(env))
        n = e.size
        if n < 8:
            return 0.0, 0.0
        spec = np.fft.rfft(e, n=2 * n)
        acf = np.fft.irfft(spec * np.conjugate(spec), n=2 * n)[:n]
        if acf[0] <= 1e-12:
            return 0.0, 0.0
        acf = acf / acf[0]
        lo = int(np.floor(60.0 * self.hop_fps / cfg.tempo_max_bpm))
        hi = int(np.ceil(60.0 * self.hop_fps / cfg.tempo_min_bpm))
        lo = max(1, lo)
        hi = min(n - 1, hi)
        if hi <= lo:
            return 0.0, 0.0
        lags = np.arange(lo, hi)
        bpms = 60.0 * self.hop_fps / lags
        prior = np.exp(-0.5 * (np.log2(bpms / cfg.tempo_prior_bpm) / cfg.tempo_prior_octaves) ** 2)
        score = acf[lags] * prior
        best = int(np.argmax(score))
        lag = float(lags[best])
        # Parabolic refinement of the lag peak.
        if 0 < best < lags.size - 1:
            y0, y1, y2 = score[best - 1], score[best], score[best + 1]
            denom = y0 - 2 * y1 + y2
            if abs(denom) > 1e-12:
                lag += 0.5 * (y0 - y2) / denom
        bpm = 60.0 * self.hop_fps / max(lag, 1e-6)
        conf = float(np.clip(score[best], 0.0, 1.0))
        return bpm, conf

    def _tempo_ioi(self, onsets: List[dict]) -> float:
        cfg = self.cfg
        times = sorted(o["second"] for o in onsets)
        if len(times) < 3:
            return 0.0
        iois = np.diff(np.array(times))
        iois = iois[(iois > 60.0 / cfg.tempo_max_bpm) & (iois < 60.0 / cfg.tempo_min_bpm)]
        if iois.size == 0:
            return 0.0
        # Histogram in log-BPM space, weighted by count; pick the mode.
        bpms = 60.0 / iois
        hist, edges = np.histogram(np.log2(bpms), bins=24)
        c = int(np.argmax(hist))
        mid = 0.5 * (edges[c] + edges[c + 1])
        return float(2.0 ** mid)

    def _beat_dp(self, env: np.ndarray, period_frames: float) -> np.ndarray:
        """Ellis-style dynamic-programming beat tracker (declared, inspectable)."""
        n = env.size
        if n < 4 or period_frames < 1.0:
            return np.empty(0, dtype=int)
        alpha = 0.9
        lo = max(1, int(round(period_frames * 0.5)))
        hi = max(lo + 1, int(round(period_frames * 2.0)))
        score = np.full(n, -1e18)
        back = np.full(n, -1, dtype=int)
        for i in range(n):
            j0 = max(0, i - hi)
            j1 = i - lo
            best_val = 0.0  # start a new chain at i
            best_j = -1
            for j in range(j0, j1 + 1):
                pen = -alpha * (np.log((i - j) / period_frames) ** 2)
                v = score[j] + pen
                if v > best_val:
                    best_val = v
                    best_j = j
            score[i] = env[i] + best_val
            back[i] = best_j
        i = int(np.argmax(score))
        beats: List[int] = []
        while i >= 0:
            beats.append(i)
            i = int(back[i])
        beats.reverse()
        return np.array(beats, dtype=int)

    def timing_and_beats(self, onsets: List[dict]) -> Tuple[dict, dict]:
        flux, _novelty, sec = self._onset_envelopes()
        env = _norm01(flux)
        bpm_acf, conf_acf = self._tempo_autocorrelation(env)
        bpm_ioi = self._tempo_ioi(onsets)
        ambiguity: List[str] = []
        if bpm_acf > 0 and bpm_ioi > 0:
            rel = abs(np.log2(bpm_acf / bpm_ioi))
            if rel > 0.10:  # more than ~7% in tempo
                ambiguity.append(
                    f"tempo rivals: autocorrelation {bpm_acf:.1f} bpm vs IOI-histogram {bpm_ioi:.1f} bpm"
                )
        bpm = bpm_acf if bpm_acf > 0 else bpm_ioi
        beats_seconds: List[float] = []
        downbeats: List[float] = []
        meter_hypotheses: List[dict] = []
        if bpm > 0:
            period = 60.0 * self.hop_fps / bpm
            low = self._band_energy_curve()
            # Primary transport: a strict periodic grid. The phase is chosen by a low-band
            # weighted onset-envelope comb (the kick is the downbeat evidence), then anchored so
            # beat 0 sits on the first detected onset — a transport is defined only up to phase,
            # and this anchor makes the beat *index* reproducible without consulting hidden truth.
            best_ph, best_score = 0.0, -1.0
            for ph in np.linspace(0.0, period, 160, endpoint=False):
                ks = np.arange(0, int(np.floor((env.size - ph) / period)) + 1)
                if ks.size == 0:
                    continue
                fr = np.clip((ph + ks * period).astype(int), 0, env.size - 1)
                s = float(
                    np.sum(env[fr] * (1.0 + 2.0 * np.array([low(float(sec[i])) for i in fr])))
                )
                if s > best_score:
                    best_score, best_ph = s, ph
            anchor = float(onsets[0]["second"]) * self.hop_fps if onsets else best_ph
            phase = anchor % period
            k_lo = int(np.ceil((0.0 - phase) / period))
            k_hi = int(np.floor((env.size - 1 - phase) / period))
            idxs = [int(round(phase + k * period)) for k in range(k_lo, k_hi + 1)]
            beats_seconds = sorted(
                {float(min(sec[i], self.duration)) for i in idxs if 0 <= i < sec.size}
            )
            beats_seconds = [b for b in beats_seconds if 0.0 <= b <= self.duration]
            # Rival route: the DP tracker. If its phase disagrees, that is real ambiguity.
            dp_idx = self._beat_dp(env, period)
            dp = [float(min(sec[i], self.duration)) for i in dp_idx if 0 <= i < sec.size]
            if dp and beats_seconds:
                resid = [min(abs(d - b) for b in beats_seconds) for d in dp]
                med_beats = float(np.median(resid)) / (period / self.hop_fps)
                if med_beats > 0.15:
                    ambiguity.append(
                        f"beat-phase rivals: periodic transport vs DP tracker differ by {med_beats:.2f} beat (median)"
                    )

            # Downbeat selection: strongest low-band (kick-like) energy at the bar phase.
            if len(beats_seconds) >= 4:
                low = self._band_energy_curve()
                for bpb in self.cfg.meter_candidates:
                    best_phase, best_support = 0, -1.0
                    for phase in range(bpb):
                        sel = [b for k, b in enumerate(beats_seconds) if (k - phase) % bpb == 0]
                        if not sel:
                            continue
                        support = float(np.mean([low(b) for b in sel]))
                        if support > best_support:
                            best_support, best_phase = support, phase
                    meter_hypotheses.append(
                        {"beats_per_bar": int(bpb), "phase_beats": float(best_phase), "support": float(np.clip(best_support, 0.0, 1.0))}
                    )
                chosen = max(meter_hypotheses, key=lambda m: m["support"])
                downbeats = [
                    b for k, b in enumerate(beats_seconds) if (k - int(chosen["phase_beats"])) % chosen["beats_per_bar"] == 0
                ]
        prov = provenance(ANALYZER_ID, ANALYZER_VERSION, DERIVED_EVENT, "acf-tempo+dp-beat/v1", ["pcm", "onsets"])
        ev = timing(
            prov,
            tempo_bpm=bpm if bpm > 0 else None,
            beats=[
                beat(t, 0.8 if (bpm_acf > 0) else 0.4, is_downbeat=(t in downbeats)) for t in beats_seconds
            ],
            downbeats=downbeats,
            tempo_map=([{"at_second": 0.0, "bpm": bpm, "confidence": conf_acf}] if bpm > 0 else []),
            meter_hypotheses=meter_hypotheses,
            ambiguity=ambiguity,
        )
        meta = {
            "tempo_autocorr_bpm": bpm_acf,
            "tempo_ioi_bpm": bpm_ioi,
            "tempo_confidence": conf_acf,
            "beat_count": len(beats_seconds),
        }
        return ev, meta

    def _band_energy_curve(self):
        """A closure: low-band (kick) energy in a 60 ms window at time t, normalized."""
        cfg = self.cfg
        n = max(64, int(0.06 * self.sr))
        win = np.hanning(n)
        freqs = np.fft.rfftfreq(n, 1.0 / self.sr)
        low_mask = (freqs >= 30.0) & (freqs <= 130.0)
        # Precompute smoothed low-band envelope over the whole signal (cheap enough).
        frames = _frames(self.x, n, max(1, n // 2))
        mag = np.abs(np.fft.rfft(frames * win[None, :], axis=1))
        low = mag[:, low_mask].sum(axis=1)
        times = (np.arange(frames.shape[0]) * (n // 2)) / self.sr
        low = _norm01(low)

        def at(t: float) -> float:
            if times.size == 0:
                return 0.0
            i = int(np.argmin(np.abs(times - t)))
            return float(low[i])

        return at

    # --- tonal --------------------------------------------------------------

    def chroma(self) -> Tuple[List[dict], np.ndarray, List[float]]:
        cfg = self.cfg
        frames = _frames(self.x, cfg.chroma_n_fft, cfg.chroma_hop)
        mag = _rfft_mag(frames, np.hanning(cfg.chroma_n_fft))
        freqs = np.fft.rfftfreq(cfg.chroma_n_fft, 1.0 / self.sr)
        mask = (freqs >= cfg.chroma_min_hz) & (freqs <= cfg.chroma_max_hz)
        midi = np.round(12.0 * np.log2(freqs[mask] / 440.0) + 69.0).astype(int)
        pc = np.mod(midi, 12)
        M = np.zeros((12, int(mask.sum())))
        M[pc, np.arange(pc.size)] = 1.0
        chroma = mag[:, mask] @ M.T
        sums = chroma.sum(axis=1, keepdims=True)
        chroma = np.divide(chroma, np.maximum(sums, 1e-9))
        centers = (np.arange(frames.shape[0]) * cfg.chroma_hop + cfg.chroma_n_fft / 2.0) / self.sr
        prov = provenance(ANALYZER_ID, ANALYZER_VERSION, ACOUSTIC, "stft-chroma/v1")
        out = [
            chroma_frame(float(min(c, self.duration)), cfg.chroma_hop / self.sr, chroma[i].tolist())
            for i, c in enumerate(centers)
        ]
        return out, chroma, centers.tolist()

    def key(self, chroma: np.ndarray) -> List[dict]:
        if chroma.size == 0:
            return []
        agg = chroma.mean(axis=0)
        if agg.sum() <= 1e-12:
            return []
        cands: List[Tuple[float, int, str]] = []
        for tonic in range(12):
            for mode, prof in (("major", KRUMHANSL_MAJOR), ("minor", KRUMHANSL_MINOR)):
                rotated = np.roll(prof, tonic)
                r = float(np.corrcoef(agg, rotated)[0, 1])
                cands.append((r, tonic, mode))
        cands.sort(reverse=True)
        out = []
        best = cands[0][0]
        for r, tonic, mode in cands[:2]:
            out.append(key_candidate(tonic, mode, float(np.clip(0.5 * (r + 1.0), 0.0, 1.0))))
        # If the top two are nearly tied, that is real ambiguity the caller should see.
        return out

    # --- pitch: two routes --------------------------------------------------

    def _pitch_hps(self, window: np.ndarray) -> Tuple[float, float]:
        n = window.size
        spec = np.abs(np.fft.rfft(window * np.blackman(n)))
        if spec.sum() <= 1e-12:
            return 0.0, 0.0
        hps = spec.copy()
        for h in range(2, 6):
            ds = spec[::h]
            L = min(hps.size, ds.size)
            hps[:L] *= ds[:L]
        freqs = np.fft.rfftfreq(n, 1.0 / self.sr)
        mask = (freqs >= self.cfg.pitch_min_hz) & (freqs <= self.cfg.pitch_max_hz)
        if not mask.any():
            return 0.0, 0.0
        idx = np.where(mask)[0]
        k = int(idx[np.argmax(hps[idx])])
        freq = float(freqs[k])
        strength = float(hps[k] / (hps[idx].max() + 1e-12))
        midi = 69.0 + 12.0 * np.log2(max(freq, 1e-6) / 440.0)
        return float(midi), strength

    def _pitch_acf(self, window: np.ndarray) -> Tuple[float, float]:
        w = window - float(np.mean(window))
        if np.sum(w * w) <= 1e-12:
            return 0.0, 0.0
        n = w.size
        spec = np.fft.rfft(w, n=2 * n)
        acf = np.fft.irfft(spec * np.conjugate(spec), n=2 * n)[:n]
        if acf[0] <= 1e-12:
            return 0.0, 0.0
        acf = acf / acf[0]
        lo = max(2, int(self.sr / self.cfg.pitch_max_hz))
        hi = min(n - 1, int(self.sr / self.cfg.pitch_min_hz))
        if hi <= lo:
            return 0.0, 0.0
        lag_i = lo + int(np.argmax(acf[lo : hi + 1]))
        peak = float(acf[lag_i])
        lag = float(lag_i)
        if 0 < lag_i < n - 1:
            y0, y1, y2 = acf[lag_i - 1], acf[lag_i], acf[lag_i + 1]
            denom = y0 - 2 * y1 + y2
            if abs(denom) > 1e-12:
                lag = lag + 0.5 * (y0 - y2) / denom
        freq = self.sr / max(lag, 1e-6)
        midi = 69.0 + 12.0 * np.log2(max(freq, 1e-6) / 440.0)
        conf = float(np.clip(peak, 0.0, 1.0))
        return float(midi), conf

    def _harmonicity(self, mag: np.ndarray, freqs: np.ndarray, f0: float) -> float:
        """Fraction of the strongest peak energy explained by a harmonic series at `f0`."""
        if f0 <= 0.0 or mag.size < 2:
            return 0.0
        df = float(freqs[1] - freqs[0])
        acc = 0.0
        used = 0
        for h in range(1, 7):
            f = h * f0
            if f >= freqs[-1]:
                break
            k = int(round(f / df))
            if 0 <= k < mag.size:
                acc += float(mag[k])
                used += 1
        if used == 0:
            return 0.0
        return acc / (used * (float(mag.max()) + 1e-12))

    def _poly_pitches(self, window: np.ndarray) -> List[Tuple[float, float]]:
        """Up to three simultaneous fundamentals from spectral peaks, partials suppressed.

        A peak that is an integer multiple (>=2) of a stronger lower peak is treated as a
        partial of that note, not a separate note — this is the octave/partial correction the
        monophonic HPS route lacked. Returns `(midi, score)` in score-descending order.
        """
        n = window.size
        spec = np.abs(np.fft.rfft(window * np.blackman(n)))
        freqs = np.fft.rfftfreq(n, 1.0 / self.sr)
        if spec.max() <= 1e-12:
            return []
        band = (freqs >= self.cfg.pitch_min_hz) & (freqs <= 2093.0)
        idxs = np.where(band)[0]
        if idxs.size == 0:
            return []
        peaks: List[int] = []
        for k in idxs:
            if k <= 0 or k >= spec.size - 1:
                continue
            if spec[k] >= spec[k - 1] and spec[k] > spec[k + 1] and spec[k] > 0.02 * spec.max():
                peaks.append(int(k))
        peaks.sort(key=lambda k: -spec[k])
        selected: List[float] = []
        out: List[Tuple[float, float]] = []
        for k in peaks:
            f = float(freqs[k])
            # Already explained as a partial of an accepted note.
            if any(abs(f / f0 - round(f / f0)) < 0.03 and round(f / f0) >= 2 for f0 in selected):
                continue
            # Defer to a stronger lower peak that this one is a partial of.
            defer = False
            for k2 in peaks:
                g = float(freqs[k2])
                if g < f and spec[k2] >= 0.20 * spec[k]:
                    r = f / g
                    if abs(r - round(r)) < 0.03 and round(r) >= 2:
                        defer = True
                        break
            if defer:
                continue
            fit = self._harmonicity(spec, freqs, f)
            if fit < 0.15:
                continue
            midi = 69.0 + 12.0 * np.log2(max(f, 1e-6) / 440.0)
            selected.append(f)
            out.append((float(midi), float(np.clip(fit * 2.5, 0.0, 1.0))))
            if len(out) >= 3:
                break
        out.sort(key=lambda x: -x[1])
        return out

    def transcribe(self, onsets: List[dict]) -> Tuple[List[dict], dict]:
        cfg = self.cfg
        prov = provenance(
            ANALYZER_ID, ANALYZER_VERSION, DERIVED_EVENT, "poly-peak+harmonic-fit/v1", ["pcm", "onsets"]
        )
        times = sorted(o["second"] for o in onsets)
        notes: List[dict] = []
        agree = octave = disagree = 0
        for i, o in enumerate(onsets):
            t = float(o["second"])
            start = int(round(t * self.sr))
            if start >= self.x.size:
                continue
            w = self.x[start : start + cfg.note_window]
            if w.size < 256:
                w = np.pad(w, (0, 256 - w.size))
            # Two independent single-pitch routes are kept as a cross-check on the primary note.
            hps, hps_str = self._pitch_hps(w)
            acf, acf_conf = self._pitch_acf(w)
            if hps > 0 and acf > 0:
                d = abs(hps - acf)
                if d <= 0.5:
                    agree += 1
                elif abs(d % 12.0) <= 0.5 or abs((d % 12.0) - 12.0) <= 0.5:
                    octave += 1
                else:
                    disagree += 1
            poly = self._poly_pitches(w)
            if not poly:
                # No confident fundamental: fall back to a single route, explicitly weaker.
                if hps > 0:
                    poly = [(float(hps), float(np.clip(0.4 + 0.4 * hps_str, 0.0, 1.0)))]
                elif acf > 0:
                    poly = [(float(acf), float(np.clip(0.4 * acf_conf, 0.0, 1.0)))]
            nxt = times[i + 1] if i + 1 < len(times) else self.duration
            offset = min(t + cfg.note_max_duration, max(t + 0.05, nxt - 0.02), self.duration)
            for pitch, score in poly:
                conf = float(np.clip(0.5 + 0.5 * score, 0.0, 1.0))
                # Both single-pitch routes agreeing with this note is genuine corroboration.
                if hps > 0 and abs(pitch - hps) <= 0.5 and acf > 0 and abs(pitch - acf) <= 0.5:
                    conf = min(1.0, conf + 0.15)
                if conf < cfg.note_min_confidence:
                    continue
                notes.append(
                    note(
                        onset_second=min(t, self.duration),
                        pitch_midi=float(np.clip(pitch, 0.0, 127.0)),
                        confidence=conf,
                        prov=prov,
                        offset_second=float(offset),
                        roles=(),  # role/orchestration is Unknown this round, by design
                    )
                )
        meta = {
            "pitch_notes_emitted": int(len(notes)),
            "pitch_route_a_hps": None,
            "pitch_route_b_acf_agreement": int(agree),
            "pitch_octave_only": int(octave),
            "pitch_disagreement": int(disagree),
        }
        return notes, meta

    # --- percussion families ------------------------------------------------

    def percussion(self, onsets: List[dict]) -> Tuple[List[dict], dict]:
        cfg = self.cfg
        prov = provenance(ANALYZER_ID, ANALYZER_VERSION, DERIVED_EVENT, "band-energy-family/v1", ["pcm", "onsets"])
        n = 1024
        win = np.hanning(n)
        freqs = np.fft.rfftfreq(n, 1.0 / self.sr)
        low_m = (freqs >= 30.0) & (freqs <= 130.0)
        mid_m = (freqs >= 150.0) & (freqs <= 400.0)
        hi_m = (freqs >= 4000.0) & (freqs <= 11000.0)
        out: List[dict] = []
        counts = {"kick": 0, "snare": 0, "hat": 0}
        for o in onsets:
            start = int(round(float(o["second"]) * self.sr))
            w = self.x[start : start + n]
            if w.size < n:
                w = np.pad(w, (0, n - w.size))
            mag = np.abs(np.fft.rfft(w * win))
            total = mag.sum() + 1e-12
            low_r = float(mag[low_m].sum() / total)
            mid_r = float(mag[mid_m].sum() / total)
            hi_r = float(mag[hi_m].sum() / total)
            flat = float(np.exp(np.mean(np.log(mag + 1e-12))) / (np.mean(mag) + 1e-12))
            fams: List[Tuple[str, float]] = []
            kick = float(np.clip((low_r - 0.15) / 0.35, 0.0, 1.0))
            snare = float(np.clip(0.5 * (mid_r / 0.25) + 0.5 * flat / 0.5, 0.0, 1.0))
            hat = float(np.clip((hi_r - 0.05) / 0.25, 0.0, 1.0))
            for fam, score in (("kick", kick), ("snare", snare), ("hat", hat)):
                if score >= 0.5:
                    fams.append((fam, round(float(min(score, 1.0)), 3)))
                    counts[fam] += 1
            out.append(
                onset(
                    second=float(o["second"]),
                    strength=float(o["strength"]),
                    prov=prov,
                    families=fams,
                )
            )
        return out, counts

    # --- recurrence + form --------------------------------------------------

    def sections_and_recurrence(
        self, beats_seconds: Sequence[float], chroma: np.ndarray, centers: Sequence[float]
    ) -> Tuple[Optional[dict], Optional[dict], dict]:
        cfg = self.cfg
        if len(beats_seconds) < 4 or chroma.size == 0:
            return None, None, {"sections": 0, "links": 0, "reason": "insufficient beats/chroma"}
        beats = list(beats_seconds)
        if beats[-1] < self.duration - 1e-6:
            beats.append(self.duration)
        cents = np.asarray(centers, dtype=np.float64)
        beat_chroma = []
        for i in range(len(beats) - 1):
            lo, hi = beats[i], beats[i + 1]
            sel = (cents >= lo) & (cents < hi)
            if not sel.any():
                beat_chroma.append(np.zeros(12))
            else:
                beat_chroma.append(chroma[sel].mean(axis=0))
        C = np.asarray(beat_chroma, dtype=np.float64)
        norms = np.linalg.norm(C, axis=1, keepdims=True)
        C = np.divide(C, np.maximum(norms, 1e-9))
        S = C @ C.T
        n = C.shape[0]
        K = cfg.section_kernel_beats
        # Gaussian checkerboard novelty kernel.
        ax = np.arange(-K, K + 1)
        g = np.exp(-0.5 * (ax / cfg.novelty_sigma) ** 2)
        kernel = np.outer(g, g) * np.sign(np.outer(ax, ax))
        novelty = np.zeros(n)
        for i in range(K, n - K):
            block = S[i - K : i + K + 1, i - K : i + K + 1]
            novelty[i] = float(np.sum(block * kernel))
        bound_idx = [0]
        if n > 2 * K + 2 and novelty[K : n - K].std() > 1e-9:
            thr = float(novelty[K : n - K].mean() + 0.5 * novelty[K : n - K].std())
            for i in range(K, n - K):
                if novelty[i] > thr and novelty[i] >= novelty[i - 1] and novelty[i] >= novelty[i + 1]:
                    if i - bound_idx[-1] >= max(2, K // 2):
                        bound_idx.append(i)
        bound_idx.append(n)
        boundaries = [float(beats[i]) for i in bound_idx]
        spans = [(float(beats[bound_idx[i]]), float(beats[bound_idx[i + 1]])) for i in range(len(bound_idx) - 1)]
        secs_prov = provenance(ANALYZER_ID, ANALYZER_VERSION, DERIVED_STRUCTURAL, "novelty-sections/v1", ["chroma", "beats"])
        rec_prov = provenance(ANALYZER_ID, ANALYZER_VERSION, DERIVED_STRUCTURAL, "section-chroma-recurrence/v1", ["chroma", "beats"])
        # Section-level recurrence: mean beat-chroma per section, cosine similarity.
        sec_vecs = []
        for s, e in spans:
            sel = (np.asarray(beats[:-1]) >= s) & (np.asarray(beats[:-1]) < e)
            if sel.any():
                v = C[sel].mean(axis=0)
            else:
                v = np.zeros(12)
            nv = np.linalg.norm(v)
            sec_vecs.append(v / nv if nv > 1e-9 else v)
        links: List[Tuple[float, float, float]] = []
        for i in range(len(spans)):
            for j in range(i + 1, len(spans)):
                sim = float(np.dot(sec_vecs[i], sec_vecs[j]))
                if sim >= cfg.recurrence_floor:
                    links.append((spans[i][0], spans[j][0], float(np.clip(sim, 0.0, 1.0))))
        sec_ev = sections(secs_prov, boundaries=boundaries, spans=spans)
        rec_ev = recurrence(
            rec_prov,
            links=links,
            summary=f"{len(spans)} candidate sections, {len(links)} section-recurrence links above {cfg.recurrence_floor}",
        )
        meta = {"sections": len(spans), "links": len(links), "duration_buckets": n}
        return sec_ev, rec_ev, meta

    # --- top-level ----------------------------------------------------------

    def analyze(self) -> dict:
        """Run every route and assemble the evidence body (no source/analyzers)."""
        onset_dicts, onset_provs, onset_cross = self.detect_onsets()
        onset_dicts.sort(key=lambda o: o["second"])
        timing_ev, timing_meta = self.timing_and_beats(onset_dicts)
        chroma_list, chroma, centers = self.chroma()
        keys = self.key(chroma)
        notes, pitch_meta = self.transcribe(onset_dicts)
        onsets_ev, perc_counts = self.percussion(onset_dicts)
        beats_seconds = [b["second"] for b in timing_ev["beats"]]
        sec_ev, rec_ev, sec_meta = self.sections_and_recurrence(beats_seconds, chroma, centers)

        analyzers = [
            analyzer_run(ANALYZER_ID, ANALYZER_VERSION, config=self.cfg.as_json(), weights=None),
        ]
        unknowns: List[str] = []
        # Record every cross-route disagreement as first-class ambiguity.
        if onset_cross["onset_agree_within_1_frame"] < 0.6 * max(1, onset_cross["onset_route_a"]):
            unknowns.append(
                "onset routes disagree on >40% of peaks; onset set is route-A only and is ambiguous"
            )
        if pitch_meta["pitch_disagreement"] > 0.4 * max(1, len(notes)):
            unknowns.append("pitch routes (HPS vs ACF) disagree on many notes; pitches are ambiguous")
        if len(keys) >= 2:
            unknowns.append("key top-2 hypotheses retained; no majority vote taken")
        if sec_meta.get("sections", 0) <= 1:
            unknowns.append("form: no section boundary crossed the novelty floor")

        tonal_ev = tonal(
            provenance(ANALYZER_ID, ANALYZER_VERSION, ACOUSTIC, "stft-chroma/v1"),
            chroma_frames=chroma_list,
            key_candidates=keys,
            chord_candidates=[],  # chords are the core's triangulation, not this file's claim
        )
        return {
            "timing": timing_ev,
            "notes": notes,
            "tonal": tonal_ev,
            "onsets": onsets_ev,
            "recurrence": rec_ev,
            "sections": sec_ev,
            "unknowns": unknowns,
            "analyzers": analyzers,
            "_meta": {
                "onset": onset_cross,
                "timing": timing_meta,
                "pitch": pitch_meta,
                "percussion_counts": perc_counts,
                "sections": sec_meta,
                "note_count": len(notes),
                "onset_count": len(onset_dicts),
            },
        }

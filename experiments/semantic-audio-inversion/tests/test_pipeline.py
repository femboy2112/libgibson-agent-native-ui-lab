"""Tests for the Semantic Audio Inversion experiment.

Run:
    python3 -m unittest discover -s tests -v

Design constraints honoured here:
- ordinary tests never run a neural model and never decode long recordings;
- the replay test consumes the committed `cached/` evidence only (no audio);
- the calibration test decodes only the tiny committed fixtures, and skips cleanly if the
  optional `ffmpeg` decoder is unavailable.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import unittest

import numpy as np

_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(_ROOT, "adapters"))
sys.path.insert(0, os.path.join(_ROOT, "scripts"))

import sai_evidence as ev  # noqa: E402
from baseline_dsp import BaselineDSP, DspConfig  # noqa: E402

SAI_BIN = os.path.join(_ROOT, "target", "debug", "sai")


def _sai_available() -> bool:
    return os.path.isfile(SAI_BIN)


class EvidenceContractTests(unittest.TestCase):
    def test_artifact_schema_and_round_trip(self):
        prov = ev.provenance("x", "1", ev.ACOUSTIC, "unit/v1")
        art = ev.artifact(
            source=ev.source_receipt(
                sha256="0" * 64,
                duration_seconds=1.0,
                sample_rate_hz=44100,
                channels=2,
                decoder="ffmpeg test",
                canonical_sample_rate_hz=22050,
                canonical_channels=1,
                resampled=True,
                channel_conversion="mono-downmix",
                canonical_pcm_sha256="1" * 64,
            ),
            analyzers=[ev.analyzer_run("x", "1")],
            timing_ev=ev.timing(prov, tempo_bpm=120.0, beats=[ev.beat(0.0, 1.0, True)]),
            notes=[ev.note(0.1, 69.0, 0.8, prov)],
            tonal_ev=ev.tonal(prov, chroma_frames=[ev.chroma_frame(0.1, 0.02, [0.0] * 12)]),
            onsets=[ev.onset(0.1, 0.5, prov)],
            unknowns=["unit"],
        )
        self.assertEqual(art["schema"], "sai.evidence/v1")
        blob = json.dumps(art, sort_keys=True)
        self.assertEqual(json.loads(blob)["schema"], "sai.evidence/v1")
        # Unknown is representable and is not silently dropped.
        self.assertEqual(art["unknowns"], ["unit"])

    def test_chroma_frame_rejects_wrong_bin_count(self):
        with self.assertRaises(ValueError):
            ev.chroma_frame(0.0, 0.01, [1.0, 2.0, 3.0])

    def test_key_candidate_rejects_unknown_mode(self):
        with self.assertRaises(ValueError):
            ev.key_candidate(0, "dorian", 0.5)
        self.assertEqual(ev.key_candidate(0, "minor", 0.5)["mode"], "minor")

    def test_receipt_keeps_source_and_artifact_hashes_distinct(self):
        src = ev.source_receipt(
            sha256="a" * 64, duration_seconds=2.0, sample_rate_hz=22050, channels=1,
            decoder="ffmpeg", canonical_sample_rate_hz=22050, canonical_channels=1,
            resampled=False, channel_conversion="none", canonical_pcm_sha256="b" * 64,
        )
        rec = ev.receipt(src, [ev.analyzer_run("x", "1")], artifact_sha256="c" * 64, git_commit="deadbeef", created_utc="2026-10-02T00:00:00Z")
        self.assertEqual(rec["schema"], "sai.receipt/v1")
        self.assertNotEqual(rec["source"]["sha256"], rec["source"]["canonical_pcm_sha256"])
        self.assertNotEqual(rec["artifact_sha256"], rec["source"]["sha256"])


class DspInvariantTests(unittest.TestCase):
    def _sine(self, freq=440.0, dur=1.0, sr=22050):
        t = np.arange(int(dur * sr)) / sr
        return (0.5 * np.sin(2 * np.pi * freq * t)).astype(np.float64)

    def test_readings_are_finite_and_bounded(self):
        d = BaselineDSP(self._sine(), 22050, DspConfig())
        onsets, _prov, _cross = d.detect_onsets()
        timing, _meta = d.timing_and_beats(onsets)
        _cl, chroma, _c = d.chroma()
        notes, _pm = d.transcribe(onsets)
        for o in onsets:
            self.assertGreaterEqual(o["strength"], 0.0)
            self.assertLessEqual(o["strength"], 1.0)
        if chroma.size:
            self.assertTrue(np.all(np.isfinite(chroma)))
            self.assertTrue(np.all(chroma >= 0.0))
            self.assertEqual(chroma.shape[1], 12)
        beats = [b["second"] for b in timing["beats"]]
        self.assertEqual(beats, sorted(beats))
        for b in beats:
            self.assertGreaterEqual(b, 0.0)
            self.assertLessEqual(b, 1.0 + 1e-6)
        for n in notes:
            self.assertTrue(np.isfinite(n["pitch_midi"]))
            self.assertGreaterEqual(n["pitch_midi"], 0.0)
            self.assertLessEqual(n["pitch_midi"], 127.0)

    def test_silence_yields_no_invention(self):
        d = BaselineDSP(np.zeros(22050), 22050, DspConfig())
        onsets, _p, _c = d.detect_onsets()
        timing, _m = d.timing_and_beats(onsets)
        notes, _pm = d.transcribe(onsets)
        self.assertLessEqual(len(onsets), 3)
        self.assertIsNone(timing["tempo_bpm"])
        self.assertEqual(notes, [])

    def test_non_finite_input_is_rejected(self):
        bad = np.zeros(1000)
        bad[10] = np.nan
        with self.assertRaises(ValueError):
            BaselineDSP(bad, 22050)


class ReplayCachedEvidenceTests(unittest.TestCase):
    """Replay mode: the core must accept committed evidence with no audio at all."""

    def setUp(self):
        self.evidence_dir = os.path.join(_ROOT, "cached", "evidence")
        if not os.path.isdir(self.evidence_dir):
            self.skipTest("no cached evidence present")
        if not _sai_available():
            self.skipTest(f"Rust `sai` binary not built at {SAI_BIN}")

    def test_every_cached_artifact_validates_and_recovers(self):
        files = sorted(f for f in os.listdir(self.evidence_dir) if f.endswith(".evidence.json"))
        self.assertTrue(files, "cached evidence corpus is empty")
        for name in files:
            path = os.path.join(self.evidence_dir, name)
            v = subprocess.run([SAI_BIN, "validate", path], capture_output=True, text=True)
            self.assertEqual(v.returncode, 0, f"validate failed for {name}: {v.stderr}")
            out = os.path.join("/tmp", f"sai-test-{name}.quotient.json")
            r = subprocess.run(
                [SAI_BIN, "recover", "--evidence", path, "--profile", "interpretive", "--out", out],
                capture_output=True, text=True,
            )
            self.assertEqual(r.returncode, 0, f"recover failed for {name}: {r.stderr}")
            with open(out, "r", encoding="utf-8") as f:
                q = json.load(f)
            self.assertEqual(q["schema"], "sai.quotient/v1")
            for axis in ("motif", "harmony", "groove", "form", "orchestration"):
                self.assertIn(axis, q)
                self.assertIn("requested", q[axis])
                self.assertIn("effective", q[axis])


class CalibrationTests(unittest.TestCase):
    def setUp(self):
        self.fixtures = os.path.join(_ROOT, "fixtures")
        if not os.path.isdir(self.fixtures):
            self.skipTest("fixtures not generated; run scripts/make_fixtures.py")
        import shutil

        if shutil.which("ffmpeg") is None or shutil.which("ffprobe") is None:
            self.skipTest("ffmpeg/ffprobe not available for the canonical boundary")

    def test_known_case_controls_pass(self):
        import calibrate

        names = sorted(n for n in os.listdir(self.fixtures) if n.endswith(".wav"))
        self.assertGreaterEqual(len(names), 8)
        for name in names:
            readings = calibrate.analyze(os.path.join(self.fixtures, name))
            ok, notes = calibrate.check(name, readings)
            self.assertTrue(ok, f"{name} control failed:\n" + "\n".join(notes))


if __name__ == "__main__":
    unittest.main()

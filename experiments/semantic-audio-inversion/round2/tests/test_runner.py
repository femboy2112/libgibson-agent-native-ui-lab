import tempfile
import unittest
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf

from sai_v2.rank import rank_candidates, REQUIRED
from sai_v2.production import fit_stems
from sai_v2.observe import production_features, yin_events
from sai_v2.evaluate import octave_line_f1, drums_metric, transported_source, phrase_residual
from sai_v2.relations import quality_family


class RunnerTests(unittest.TestCase):
    def test_correct_attacks_do_not_hide_filled_rests(self):
        a = [{"onset_beat": 0, "duration_beats": .25, "pitch_midi": 60},
             {"onset_beat": 1, "duration_beats": .25, "pitch_midi": 62}]
        b = [{"onset_beat": 0, "offset_beat": .9, "pitch_midi": 60},
             {"onset_beat": 1, "offset_beat": 1.25, "pitch_midi": 62}]
        self.assertEqual(octave_line_f1(a, b)["f1"], 1.)
        self.assertGreater(phrase_residual(a, b)["rest_intrusion_fraction"], .8)
    def test_public_quality_family_complete_extensions(self):
        self.assertEqual(quality_family("add9"), "major")
        self.assertEqual(quality_family("minmaj7"), "minor")
        self.assertEqual(quality_family("m6"), "minor")
    def test_global_preroll_transport_every_coordinate(self):
        source = {"duration_beats": 4, "notes": [{"onset_beat": 1, "duration_beats": .5}],
                  "drums": [{"onset_beat": 1, "metric_onset_beat": 1}],
                  "harmony": [{"start_beat": 0, "end_beat": 4}],
                  "sections": [{"start_beat": 0, "end_beat": 4}]}
        shifted = transported_source(source, .75)
        self.assertEqual(shifted["notes"][0], {"onset_beat": 1.75, "duration_beats": .5})
        self.assertEqual(shifted["harmony"][0], {"start_beat": .75, "end_beat": 4.75})
        self.assertEqual(shifted["sections"][0], shifted["harmony"][0])
        self.assertEqual(shifted["drums"][0]["metric_onset_beat"], 1.75)
        self.assertEqual(source["notes"][0]["onset_beat"], 1)
    def test_drum_rivals_not_invented_as_simultaneous_hits(self):
        result = drums_metric([{"second": 1, "family_candidates": [
            {"family": "kick", "confidence": .8}, {"family": "snare", "confidence": .8}]}], 120)
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]["family"], "unknown")

    def test_global_octave_not_per_note_cheat(self):
        a = [{"onset_beat": i, "pitch_midi": p} for i, p in enumerate((60, 62, 64))]
        b = [{"onset_beat": i, "pitch_midi": p} for i, p in enumerate((48, 50, 52))]
        self.assertEqual(octave_line_f1(a, b)["f1"], 1.)
        b[1]["pitch_midi"] += 12
        self.assertLess(octave_line_f1(a, b)["f1"], 1.)

    def test_private_artifact_paths_ignored(self):
        root = Path(__file__).resolve().parents[4]
        rel = Path(__file__).resolve().parents[1].relative_to(root)
        names = [rel/"cover_out"/"track-001"/"compiled_source_model.json",
                 rel/"private.wav", rel/"source.mp3", rel/"private-stems.tar.zst"]
        for name in names:
            self.assertEqual(subprocess.run(["git", "check-ignore", "-q", str(name)], cwd=root).returncode, 0)

    def test_rejected_and_structurally_invalid_never_win(self):
        cs = [{"candidate_id": "pretty", "admitted": True, "gates": {"motif": False},
               "production_residual": {"rms": 0}},
              {"candidate_id": "valid", "admitted": True, "gates": dict.fromkeys(REQUIRED, True),
               "production_residual": {"rms": 10}},
              {"candidate_id": "refused", "admitted": False, "gates": {"motif": True}}]
        self.assertEqual(rank_candidates(cs)["winner"], "valid")
        self.assertEqual(rank_candidates(cs), rank_candidates(list(reversed(cs))))
        self.assertIsNone(rank_candidates([cs[0], cs[2]])["winner"])

    def test_empty_or_unknown_gates_never_win(self):
        for gates in ({}, dict.fromkeys(REQUIRED, None), dict.fromkeys(REQUIRED, 1)):
            self.assertIsNone(rank_candidates([{"candidate_id": "empty", "admitted": True, "gates": gates}])["winner"])

    def test_stereo_fit_preserves_left_right(self):
        with tempfile.TemporaryDirectory() as d:
            sr = 22050
            x = .1*np.sin(2*np.pi*440*np.arange(sr)/sr)
            raw, out = Path(d)/"raw.wav", Path(d)/"out.wav"
            sf.write(raw, np.column_stack((x, x*.2)), sr)
            target = production_features(raw)
            fit_stems({"lead": raw}, {"lead": target}, out, structural_pass=True)
            self.assertLess(production_features(out)["pan"], -.8)

    def test_production_fail_closed(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)/"processed.wav"
            result = fit_stems({}, {}, out, structural_pass=False)
            self.assertEqual(result["status"], "refused")
            self.assertFalse(out.exists())

    def test_actual_production_control_and_fresh_pitch_observation(self):
        with tempfile.TemporaryDirectory() as d:
            sr = 22050
            x = .1*np.sin(2*np.pi*440*np.arange(sr*2)/sr)
            src, target, out = [Path(d)/name for name in ("raw.wav", "target.wav", "out.wav")]
            sf.write(src, np.column_stack([x, x]), sr)
            sf.write(target, np.column_stack([x*2, x*2]), sr)
            before = yin_events(x, sr, "lead")
            self.assertTrue(before and abs(before[0]["pitch_midi"]-69)<.1)
            result = fit_stems({"lead": src}, {"lead": production_features(target)}, out, structural_pass=True)
            y, _ = sf.read(out)
            after = yin_events(y.mean(axis=1), sr, "lead")
            self.assertAlmostEqual(before[0]["pitch_midi"], after[0]["pitch_midi"], delta=.1)
            self.assertAlmostEqual(result["after"]["rms_db"], production_features(target)["rms_db"], delta=.1)
            self.assertEqual(len(y), len(x))

    def test_null_pitch(self):
        self.assertEqual(yin_events(np.zeros(22050), 22050, "lead"), [])


if __name__ == "__main__":
    unittest.main()

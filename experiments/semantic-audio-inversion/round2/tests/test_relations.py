import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
from sai_v2.relations import compare_form, compare_groove, compare_harmony, compare_motif, compare_motif_occurrences, merge_harmony_windows, note_f1


def chord(start, end, root=0, quality="maj", **kw):
    return dict(start_beat=start, end_beat=end, root_pc=root, quality=quality, **kw)


def notes(pitches=(60, 64, 62, 67), scale=1, shift=0, role="lead"):
    return [dict(onset_beat=i * scale, offset_beat=(i + .75) * scale,
                 pitch_midi=p + shift, role=role) for i, p in enumerate(pitches)]


def drums(offset=0):
    return [dict(onset_beat=i + offset, family="kick" if i % 2 == 0 else "snare") for i in range(4)]


class HarmonyTests(unittest.TestCase):
    def test_split_and_merge_are_invariant(self):
        source = [chord(0, 4)]
        split = [chord(i, i + 1) for i in range(4)]
        result = compare_harmony(source, split)
        self.assertEqual(result["relation"], "exact")
        self.assertEqual(result["root_agreement"], 1)
        self.assertEqual(len(result["merged_observed"]), 1)
        self.assertEqual(len(result["merged_observed"][0]["members"]), 4)
        self.assertEqual(compare_harmony(split, source)["relation"], "exact")
        self.assertEqual(split, [chord(i, i + 1) for i in range(4)])

    def test_half_bar_root_mutation_degrades(self):
        r = compare_harmony([chord(0, 4)], [chord(0, 2), chord(2, 4, 7)])
        self.assertEqual(r["relation"], "free")
        self.assertEqual(r["root_agreement"], .5)
        self.assertEqual(r["transitions"]["root"]["extra"], 1)

    def test_extension_lowers_to_family(self):
        r = compare_harmony([chord(0, 4, quality="maj7")], [chord(0, 4)])
        self.assertEqual(r["relation"], "quality-family")
        self.assertEqual(r["exact_agreement"], 0)

    def test_quality_mutation_lowers_to_ordered(self):
        self.assertEqual(compare_harmony([chord(0, 4)], [chord(0, 4, quality="min")])["relation"], "ordered")

    def test_tiny_spurious_root_cannot_hide_under_coverage(self):
        r = compare_harmony([chord(0, 4)], [chord(0, 2), chord(2, 2.01, 7), chord(2.01, 4)])
        self.assertGreater(r["root_agreement"], .99)
        self.assertEqual(r["relation"], "free")
        self.assertEqual(r["transitions"]["root"]["extra"], 2)

    def test_spurious_extension_does_not_break_family(self):
        r = compare_harmony([chord(0, 4)], [chord(0, 2), chord(2, 4, quality="maj7")])
        self.assertEqual(r["relation"], "quality-family")
        self.assertEqual(r["transitions"]["exact"]["extra"], 1)

    def test_missing_transition_and_missing_tail_penalized(self):
        source = [chord(0, 4), chord(4, 8, 7)]
        self.assertEqual(compare_harmony(source, [chord(0, 8)])["transitions"]["root"]["missing"], 1)
        self.assertEqual(compare_harmony(source, [chord(0, 4)])["coverage"], .5)

    def test_unknown_not_free_and_unknown_qualities_not_exact(self):
        self.assertIsNone(compare_harmony([], [chord(0, 4)])["relation"])
        r = compare_harmony([chord(0, 4, quality="unknown")], [chord(0, 4, quality="unknown")])
        self.assertEqual(r["relation"], "ordered")

    def test_rivals_are_not_voted_away(self):
        source = [chord(0, 4, rivals=[dict(root_pc=7, quality="maj", score=.01)])]
        r = compare_harmony(source, [chord(0, 4)])
        self.assertIsNone(r["relation"])
        self.assertEqual(r["selected_label_relation"], "exact")
        source[0]["rivals"][0]["root_pc"] = 0
        source[0]["rivals"][0]["quality"] = "min"
        self.assertEqual(compare_harmony(source, [chord(0, 4)])["relation"], "ordered")

    def test_bad_spans_fail_closed(self):
        for bad in ([chord(2, 1)], [chord(0, float("inf"))], [chord(0, 2), chord(1, 3)], [chord(0, 4, 12)], [chord(0, 4, confidence=2)]):
            with self.assertRaises(ValueError):
                compare_harmony(bad, [chord(0, 4)])


class MotifTests(unittest.TestCase):
    def test_transposition_and_tempo_scale(self):
        r = compare_motif(notes(), notes(scale=1.7, shift=5))
        self.assertEqual(r["relation"], "metric")
        self.assertEqual(r["relative_note_match"]["f1"], 1)
        self.assertEqual(r["transposition_semitones"], 5)
        self.assertTrue(r["duration_equal"])

    def test_mutated_interval_does_not_earn_metric(self):
        r = compare_motif(notes(), notes((60, 65, 62, 67)))
        self.assertEqual(r["relation"], "theme")
        self.assertEqual(r["relative_note_match"]["f1"], .75)

    def test_deleted_note_fails_metric(self):
        r = compare_motif(notes(), notes()[:2] + notes()[3:])
        self.assertEqual(r["relation"], "free")
        self.assertLess(r["relative_note_match"]["f1"], 1)

    def test_roles_prevent_accompaniment_pollution(self):
        mixture = notes() + notes((48, 52, 50, 55), role="support")
        self.assertIsNone(compare_motif(notes(), mixture)["relation"])
        self.assertEqual(compare_motif(notes(), mixture, role="lead")["relation"], "metric")
        self.assertEqual(compare_motif(notes(), mixture, role="bass")["status"], "unknown")

    def test_rest_duration_is_diagnostic_and_never_faithful(self):
        changed = notes()
        changed[0]["offset_beat"] = 3
        r = compare_motif(notes(), changed)
        self.assertEqual(r["relation"], "metric")
        self.assertFalse(r["duration_equal"])

    def test_notes_matching_is_maximum_cardinality(self):
        # Greedy nearest consumes the only possible partner of the second event.
        a = [dict(onset_beat=.10, pitch_midi=60), dict(onset_beat=.20, pitch_midi=60)]
        b = [dict(onset_beat=.15, pitch_midi=60), dict(onset_beat=0, pitch_midi=60)]
        r = note_f1(a, b, onset_tolerance=.11)
        self.assertEqual(r["f1"], 1)
        self.assertEqual(note_f1(a, b[:1])["matched"], 1)

    def test_invalid_notes_fail(self):
        for replacement in ({"pitch_midi": float("nan")}, {"offset_beat": 0}, {"confidence": -.1}):
            a = notes()
            a[0].update(replacement)
            with self.assertRaises(ValueError):
                compare_motif(a, notes())

    def test_corpus_contract_aliases(self):
        a = [{"onset_beat": i, "duration_beats": .5, "midi": p} for i, p in enumerate((60, 62, 67))]
        self.assertEqual(compare_motif(a, a)["relation"], "metric")
        self.assertEqual(compare_harmony([dict(start_beat=0, end_beat=4, root=0, quality="maj")], [chord(0, 4)])["relation"], "exact")


class GrooveTests(unittest.TestCase):
    def test_isolated_known_drums_recover_and_microtiming_preserved(self):
        r = compare_groove(drums(), drums(.07))
        self.assertEqual(r["relation"], "kick-snare")
        self.assertEqual(r["pocket"]["f1"], 1)
        self.assertEqual(r["raw_observed"][0]["onset_beat"], .07)
        self.assertEqual(r["observed_skeleton"][0]["onset_beat"], 0)

    def test_changed_offbeat_preserves_only_pocket(self):
        a = drums() + [dict(onset_beat=.5, family="kick")]
        b = drums() + [dict(onset_beat=.75, family="kick")]
        self.assertEqual(compare_groove(a, b)["relation"], "pocket-skeleton")

    def test_explicit_fill_excluded_from_pocket_but_not_full(self):
        a = drums()
        b = drums() + [dict(onset_beat=2, family="snare", is_fill=True)]
        r = compare_groove(a, b)
        self.assertEqual(r["relation"], "pocket-skeleton")
        self.assertLess(r["full"]["f1"], 1)
        b[-1]["is_fill"] = False
        self.assertEqual(compare_groove(a, b)["relation"], "free")

    def test_unknown_family_is_not_wildcard(self):
        b = [dict(onset_beat=i, family="unknown") for i in range(4)]
        self.assertIsNone(compare_groove(drums(), b)["relation"])

    def test_empty_skeleton_cannot_vacuously_match(self):
        a = [dict(onset_beat=.3, family="kick")]
        b = [dict(onset_beat=.7, family="snare")]
        self.assertEqual(compare_groove(a, b)["relation"], "free")

    def test_missing_backbeat_is_failure(self):
        self.assertEqual(compare_groove(drums(), drums()[:3])["relation"], "free")

    def test_partial_unknown_cannot_be_silently_ignored(self):
        self.assertIsNone(compare_groove(drums(), drums() + [dict(onset_beat=1, family="unknown")])["relation"])
        self.assertEqual(compare_groove(drums(), drums() + [dict(onset_beat=1.5, family="unknown")])["relation"], "pocket-skeleton")


class FormTests(unittest.TestCase):
    def sections(self, labels=("a", "b", "a")):
        return [dict(start_beat=4 * i, end_beat=4 * (i + 1), family=f) for i, f in enumerate(labels)]

    def test_renamed_families_preserve_topology(self):
        r = compare_form(self.sections(), self.sections((5, 7, 5)))
        self.assertEqual(r["relation"], "topology")
        self.assertEqual(r["boundaries"]["f1"], 1)
        self.assertTrue(r["spans_equal"])

    def test_false_split_is_a_boundary_error(self):
        a = self.sections()
        b = [dict(start_beat=0, end_beat=2, family="a"), dict(start_beat=2, end_beat=4, family="a")] + a[1:]
        r = compare_form(a, b)
        self.assertEqual(r["relation"], "topology")
        self.assertEqual(r["boundaries"]["extra"], 1)
        self.assertFalse(r["spans_equal"])

    def test_changed_recurrence_breaks_topology(self):
        self.assertEqual(compare_form(self.sections(), self.sections(("a", "b", "b")))["relation"], "free")

    def test_links_define_families_without_human_names(self):
        a = self.sections()
        for s in a:
            del s["family"]
        links = [dict(at_beat=.2, to_beat=8.2, similarity=.9)]
        r = compare_form(a, a, links, links)
        self.assertEqual(r["relation"], "topology")
        self.assertEqual(r["reference_topology"], [0, 1, 0])
        self.assertIsNone(compare_form(a, a, links, [dict(at_beat=.2, to_beat=8.2, similarity=.2)])["relation"])
        self.assertIsNone(compare_form(a, a)["relation"])

    def test_invalid_link_and_overlap_fail(self):
        with self.assertRaises(ValueError):
            compare_form(self.sections(), self.sections(), [dict(at_beat=0, to_beat=15, similarity=.9)], [])
        b = self.sections()
        b[1]["start_beat"] = 3
        with self.assertRaises(ValueError):
            compare_form(self.sections(), b)


class MotifOccurrenceTests(unittest.TestCase):
    def repeated(self):
        return notes() + [{**n, "onset_beat": n["onset_beat"] + 8, "offset_beat": n["offset_beat"] + 8} for n in notes()]

    def windows(self):
        return [dict(start_beat=0, end_beat=4), dict(start_beat=8, end_beat=12)]

    def test_all_occurrences_evaluated(self):
        a = self.repeated()
        b = copy.deepcopy(a)
        b[-2]["pitch_midi"] += 3
        r = compare_motif_occurrences(a, b, self.windows())
        self.assertEqual(r["metric_fraction"], .5)
        self.assertNotEqual(r["relation"], "metric")
        self.assertLess(r["minimum_relative_note_f1"], 1)
        self.assertEqual(len(r["occurrences"]), 2)

    def test_absent_candidate_occurrence_is_failure(self):
        r = compare_motif_occurrences(self.repeated(), notes(), self.windows())
        self.assertEqual(r["status"], "observed")
        self.assertEqual(r["metric_fraction"], .5)
        self.assertEqual(r["minimum_relative_note_f1"], 0)
        self.assertEqual(r["relation"], "free")

    def test_empty_source_occurrence_is_unknown(self):
        r = compare_motif_occurrences(notes(), self.repeated(), self.windows())
        self.assertIsNone(r["relation"])

    def test_transposed_repetition_preserves_each_identity(self):
        a = self.repeated()
        b = [{**n, "pitch_midi": n["pitch_midi"] + 7} for n in a]
        self.assertEqual(compare_motif_occurrences(a, b, self.windows())["relation"], "metric")

    def test_window_search_not_performed(self):
        b = [{**n, "onset_beat": n["onset_beat"] + 4, "offset_beat": n["offset_beat"] + 4} for n in self.repeated()]
        self.assertEqual(compare_motif_occurrences(self.repeated(), b, self.windows())["metric_fraction"], 0)


if __name__ == "__main__":
    unittest.main()

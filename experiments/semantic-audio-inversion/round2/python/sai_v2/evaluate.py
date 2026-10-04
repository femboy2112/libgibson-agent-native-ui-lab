"""Observation-space evaluation. The observer runs before evaluator sees truth."""
from .relations import note_f1, compare_motif, compare_harmony, compare_groove, compare_form, compare_motif_occurrences
from .corpus import BeatTransport, read_csv
import copy
import math
import statistics


def transported_source(source, preroll_beats=0.):
    if not math.isfinite(preroll_beats) or preroll_beats < 0:
        raise ValueError("invalid declared preroll")
    result = copy.deepcopy(source)
    for group, keys in (("notes", ("onset_beat",)), ("drums", ("onset_beat", "metric_onset_beat")),
                        ("harmony", ("start_beat", "end_beat")), ("sections", ("start_beat", "end_beat"))):
        for event in result.get(group, []):
            for key in keys:
                if event.get(key) is not None:
                    event[key] += preroll_beats
    result["duration_beats"] = result.get("duration_beats", 0)+preroll_beats
    result["evaluation_preroll_beats"] = preroll_beats
    return result


def seconds_source(source):
    """Declare recording seconds as the comparison frame for measurement playback."""
    result = copy.deepcopy(source)
    scale = source["tempo_bpm"]/60
    for n in result["notes"]:
        n["onset_beat"] = n["onset_sec"]*scale
        n["duration_beats"] = (n["offset_sec"]-n["onset_sec"])*scale
    for n in result["drums"]:
        n["onset_beat"] = n["onset_sec"]*scale
    for group in ("harmony", "sections"):
        for span in result[group]:
            span["start_beat"] = span["start_sec"]*scale
            span["end_beat"] = span["end_sec"]*scale
    result["duration_beats"] = result["duration_sec"]*scale
    result["evaluation_source_seconds"] = True
    return result


def notes_metric(notes, bpm, role=None):
    out = []
    for x in notes:
        n = dict(x)
        n["onset_beat"] = x["onset_second"]*bpm/60
        if x.get("offset_second") is not None:
            n["offset_beat"] = x["offset_second"]*bpm/60
        if role is not None:
            n["role"] = role
        out.append(n)
    return out


def harmony_metric(spans, bpm):
    return [{**s, "start_beat": s["start_second"]*bpm/60,
             "end_beat": s["end_second"]*bpm/60} for s in spans]


def drums_metric(events, bpm):
    result = []
    for x in events:
        families = x["family_candidates"]
        result.append({"onset_beat": x["second"]*bpm/60,
                       "family": families[0]["family"] if len(families) == 1 else "unknown",
                       "family_candidates": families,
                       "confidence": families[0]["confidence"] if len(families) == 1 else None})
    return result


def octave_line_f1(reference, observed):
    """Minimize only over the public line relation's ONE global octave freedom."""
    alternatives = []
    for shift in range(-48, 49, 12):
        shifted = [{**n, "pitch_midi": n["pitch_midi"]+shift} for n in observed]
        if any(not 0 <= n["pitch_midi"] <= 127 for n in shifted):
            continue
        value = note_f1(reference, shifted)
        alternatives.append((value["f1"], -abs(shift), shift, value))
    _, _, shift, result = max(alternatives, key=lambda x: x[:3])
    return {**result, "global_octave_semitones": shift,
            "relation": "single global octave; no per-note shift, time warp, or source edit"}


def phrase_residual(reference, observed):
    """Rest occupancy complements attack-only F1; inferred silence is not ground truth."""
    # Use the comparator's canonical order, including simultaneous-pitch ties:
    # its alignment indices refer to this order, not caller insertion order.
    order = lambda n: (n["onset_beat"], n.get("pitch_midi", n.get("midi")))
    a = sorted(reference, key=order)
    b = sorted(observed, key=order)
    aligned = octave_line_f1(a, b)  # validates note domains before interval arithmetic
    def end(n):
        if n.get("offset_beat") is not None:
            return n["offset_beat"]
        if n.get("duration_beats") is not None:
            return n["onset_beat"]+n["duration_beats"]
        return None
    if not a or not b or any(end(n) is None for n in a+b):
        return {"status": "unknown", "source_gap_count": None,
                "intruded_gap_count": None, "source_rest_beats": None,
                "intrusion_beats": None, "rest_intrusion_fraction": None,
                "matched_offset_median_error_beats": None,
                "reason": "missing note route or release evidence; zero-duration notes and silence are not fabricated"}
    # A rest is a gap in the UNION of source sounding intervals. Adjacent event
    # starts alone would invent rests under a longer overlapping note.
    sounding = []
    for n in a:
        start, stop = n["onset_beat"], end(n)
        if sounding and start <= sounding[-1][1]:
            sounding[-1][1] = max(sounding[-1][1], stop)
        else:
            sounding.append([start, stop])
    gaps = [(x[1], y[0]) for x, y in zip(sounding, sounding[1:]) if y[0]-x[1] > .05]
    total = sum(hi-lo for lo, hi in gaps)
    intrusion = 0.
    intruded = 0
    for lo, hi in gaps:
        intervals = sorted((max(lo, n["onset_beat"]), min(hi, end(n))) for n in b
                           if n["onset_beat"] < hi and end(n) > lo)
        right, occupied = lo, 0.
        for left, stop in intervals:
            occupied += max(0., stop-max(left, right))
            right = max(right, stop)
        intrusion += occupied
        intruded += occupied > .01
    offsets = [abs(end(a[i])-end(b[j])) for i, j in aligned["alignment"]]
    return {"status": "observed" if gaps else "unknown", "source_gap_count": len(gaps),
            "intruded_gap_count": intruded, "source_rest_beats": total,
            "intrusion_beats": intrusion, "rest_intrusion_fraction": intrusion/total if total else None,
            "matched_offset_median_error_beats": statistics.median(offsets) if offsets else None,
            "boundary": "source F0-segmentation rests versus freshly observed voiced intervals; route uncertainty retained; no perceptual certification"}


def form_residual(source, full, bpm):
    observed = [{"start_beat": x["start_second"]*bpm/60,
                 "end_beat": x["end_second"]*bpm/60}
                for x in (full.get("sections") or {}).get("sections", [])]
    links = [{"at_beat": x["at_second"]*bpm/60, "to_beat": x["to_second"]*bpm/60,
              "similarity": x["similarity"]} for x in (full.get("recurrence") or {}).get("links", [])]
    return compare_form(source.get("sections", []), observed, observed_links=links)


def occurrence_residual(source, observed, role):
    candidates = [f for f in source.get("motif_families", []) if f.get("role") in (("vocal", "vocals", "lead") if role == "lead" else ("bass",))]
    if not candidates:
        return {"status": "unknown", "reason": "no source family", "relation": None}
    family = sorted(candidates, key=lambda f: (-f.get("n_occurrences", 0), f["canonical_id"]))[0]
    if source.get("evaluation_source_seconds"):
        transport = lambda t: t*source["tempo_bpm"]/60
    else:
        beats = read_csv(source["provenance"]["beat_grid"]["artifact"])
        transport = BeatTransport([float(b["time_sec"]) for b in beats], source["tempo_bpm"])
    # DeepSeek occurrence end_sec names the final onset, not its offset. Include that
    # endpoint and a declared 0.12 beat observation tolerance on both edges.
    shift = source.get("evaluation_preroll_beats", 0.)
    windows = [{"start_beat": max(0., transport(x["start_sec"])+shift-.12),
                "end_beat": transport(x["end_sec"])+shift+.12} for x in family.get("occurrences", [])
               if x.get("end_sec", 0) > x.get("start_sec", 0)]
    result = compare_motif_occurrences(source["notes"], observed, windows, role=role)
    result.update(source_family_id=family["canonical_id"], endpoint_tolerance_beats=.12)
    return result


def compare(source, observations, preroll_beats=0.):
    source = transported_source(source, preroll_beats)
    bpm = source["tempo_bpm"]
    full = observations["full"]
    ref = source["notes"]
    lead_ref = [n for n in ref if n["role"] == "lead"]
    bass_ref = [n for n in ref if n["role"] == "bass"]
    full_notes = notes_metric(full["frozen_notes"], bpm)
    lead = notes_metric(observations["lead"]["notes"], bpm, "lead")
    bass = notes_metric(observations["bass"]["notes"], bpm, "bass")
    stem_notes = [n for role in ("lead", "bass", "keys", "pad")
                  for n in notes_metric(observations[role]["frozen_notes"], bpm, role)]
    harmony_ref = [{**s, "root_pc": s.get("root_pc", s.get("root"))} for s in source.get("harmony", [])]
    harmony = compare_harmony(harmony_ref, harmony_metric(observations["support"]["harmony"], bpm))
    full_harmony = compare_harmony(harmony_ref, harmony_metric(full["harmony"], bpm))
    groove = compare_groove(source.get("drums", []), drums_metric(observations["drums"]["drums"], bpm))
    full_groove = compare_groove(source.get("drums", []), drums_metric(full["drums"], bpm))
    source_attacks = [{"onset_beat": x["onset_beat"], "pitch_midi": 0} for x in source.get("drums", [])]
    observed_attacks = [{"onset_beat": x["second"]*bpm/60, "pitch_midi": 0} for x in observations["drums"]["drums"]]
    onset_match = note_f1(source_attacks, observed_attacks)
    onset_match["boundary"] = "drum onset timing only; no family or PocketSkeleton certification"
    measured_bpm = full.get("timing", {}).get("tempo_bpm")
    return {
        "schema": "sai.residual/v2", "transport": "timing-conditioned: evaluation maps observed seconds through declared source/target tempo; observer receives PCM only",
        "preroll_beats": preroll_beats,
        "timing": {"reference_bpm": bpm, "observed_bpm": measured_bpm,
                   "relative_error": abs(measured_bpm-bpm)/bpm if measured_bpm else None,
                   "ambiguity": full.get("timing", {}).get("ambiguity", [])},
        "frozen_notes_all_full": note_f1(ref, full_notes),
        "frozen_notes_all_stems": note_f1(ref, stem_notes),
        "frozen_notes_lead_full": note_f1(lead_ref, full_notes),
        "frozen_notes_lead_stem": note_f1(lead_ref, notes_metric(observations["lead"]["frozen_notes"], bpm)),
        "frozen_notes_bass_full": note_f1(bass_ref, full_notes),
        "frozen_notes_bass_stem": note_f1(bass_ref, notes_metric(observations["bass"]["frozen_notes"], bpm)),
        "lead": note_f1(lead_ref, lead), "bass": note_f1(bass_ref, bass),
        "lead_octave_quotient": octave_line_f1(lead_ref, lead),
        "bass_octave_quotient": octave_line_f1(bass_ref, bass),
        "phrase": {"lead": phrase_residual(lead_ref, lead), "bass": phrase_residual(bass_ref, bass)},
        "motif": compare_motif(lead_ref, lead), "bass_figure": compare_motif(bass_ref, bass),
        "motif_occurrences": occurrence_residual(source, lead, "lead"),
        "bass_occurrences": occurrence_residual(source, bass, "bass"),
        "harmony": harmony, "full_harmony": full_harmony,
        "groove": groove, "full_groove": full_groove,
        "drum_onset_match": onset_match,
        "form": form_residual(source, full, bpm),
        "orchestration": {"status": "UNVERIFIED", "relation": None,
                          "reason": "Stem occupancy is measurable, but source seating is ambiguous"},
    }


def gates(residual):
    return {
        "timing": residual["timing"]["relative_error"] is not None and residual["timing"]["relative_error"] <= .05,
        "motif": residual["motif"].get("relation") == "metric",
        "bass": residual["bass_figure"].get("relation") == "metric",
        "harmony": residual["harmony"].get("relation") in ("quality-family", "exact"),
        "groove": residual["groove"].get("relation") in ("pocket-skeleton", "kick-snare"),
        "form": residual["form"].get("relation") in ("topology", "exact"),
    }

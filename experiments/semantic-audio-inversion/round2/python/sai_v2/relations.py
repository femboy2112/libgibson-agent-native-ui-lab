"""Additive observational comparators. No generator objects or song IDs are accepted.

All coordinates are beats in a caller-declared common metric frame. These v2
relations are tolerance-bearing measurements, not HumanMusic's symbolic proof.
Missing evidence returns relation=None, never an invented Free relation.
"""
from __future__ import annotations

import copy
import math
import statistics

VERSION = "sai.observational-relations/v2"
THRESHOLDS = {
    "note_onset_beats": 0.12, "note_pitch_semitones": 0.75,
    "motif_relative_pitch_semitones": 0.5, "motif_onset_fraction": 0.04,
    "motif_duration_fraction": 0.12, "harmony_coverage": 0.95,
    "harmony_ordered_coverage": 0.90, "transition_beats": 0.125,
    "groove_quarter_beats": 0.10, "groove_stroke_beats": 0.125,
    "form_boundary_beats": 0.5, "form_recurrence_floor": 0.70,
}


def _number(value, name, minimum=None):
    if isinstance(value, bool) or not isinstance(value, (float, int)) or not math.isfinite(value):
        raise ValueError(f"{name} must be finite numeric")
    if minimum is not None and value < minimum:
        raise ValueError(f"{name} below {minimum}")
    return float(value)


def _confidence(row):
    if row.get("confidence") is not None:
        c = _number(row["confidence"], "confidence", 0)
        if c > 1:
            raise ValueError("confidence above 1")


def _unknown(reason, **details):
    return {"schema": VERSION, "status": "unknown", "relation": None,
            "reason": reason, **details}


def quality_family(quality):
    groups = {
        "major": {"maj", "major", "maj7", "major7", "dom7", "7", "6", "maj6", "9", "dom9", "maj9"},
        "minor": {"min", "minor", "m", "min7", "minor7", "m7", "min6", "min9"},
        "diminished": {"dim", "diminished", "dim7", "min7b5", "half-diminished"},
        "augmented": {"aug", "augmented"}, "suspended": {"sus", "sus2", "sus4"},
    }
    for family, labels in groups.items():
        if quality in labels:
            return family
    return None


def _spans(rows):
    out = copy.deepcopy(list(rows))
    for row in out:
        if "root_pc" not in row:
            row["root_pc"] = row.get("root")
        row["start_beat"] = _number(row["start_beat"], "start_beat", 0)
        row["end_beat"] = _number(row["end_beat"], "end_beat", 0)
        if row["end_beat"] <= row["start_beat"]:
            raise ValueError("nonpositive harmonic span")
        root = row.get("root_pc")
        if root is not None and (isinstance(root, bool) or not isinstance(root, int) or not 0 <= root < 12):
            raise ValueError("root_pc must be integer 0..11 or None")
        _confidence(row)
    out.sort(key=lambda x: (x["start_beat"], x["end_beat"]))
    if any(a["end_beat"] > b["start_beat"] + 1e-9 for a, b in zip(out, out[1:])):
        raise ValueError("overlapping spans within one route; compare rival routes separately")
    return out


def merge_harmony_windows(rows):
    """Merge adjacent identical labels, retaining every raw row in `members`."""
    merged = []
    for row in _spans(rows):
        signature = lambda x: (x.get("root_pc"), x.get("quality"), x.get("rivals", []))
        if merged and abs(merged[-1]["end_beat"] - row["start_beat"]) <= 1e-9 and signature(merged[-1]) == signature(row):
            merged[-1]["end_beat"] = row["end_beat"]
            merged[-1]["members"].append(row)
        else:
            merged.append({**copy.deepcopy(row), "members": [row]})
    return merged


def _pairs(left, right, compatible):
    """Deterministic maximum-cardinality bipartite matching; no double counting."""
    edges = [[j for j, y in enumerate(right) if compatible(x, y)] for x in left]
    right_to_left = {}
    # Iterative augmenting paths avoid recursion limits on long real recordings.
    for start in range(len(left)):
        queue, parent, seen = [start], {}, {start}
        endpoint = None
        for i in queue:
            for j in edges[i]:
                if j in parent:
                    continue
                parent[j] = i
                if j not in right_to_left:
                    endpoint = j
                    break
                other = right_to_left[j]
                if other not in seen:
                    seen.add(other)
                    queue.append(other)
            if endpoint is not None:
                break
        if endpoint is not None:
            left_to_right = {i: j for j, i in right_to_left.items()}
            while endpoint is not None:
                i = parent[endpoint]
                previous = left_to_right.get(i)
                right_to_left[endpoint] = i
                endpoint = previous
    return sorted((i, j) for j, i in right_to_left.items())


def _prf(n_reference, n_observed, matched):
    precision = matched / n_observed if n_observed else 0.0
    recall = matched / n_reference if n_reference else 0.0
    return {"precision": precision, "recall": recall,
            "f1": 2 * precision * recall / (precision + recall) if precision + recall else 0.0,
            "matched": matched, "reference_count": n_reference, "observed_count": n_observed}


def compare_harmony(reference, observed):
    """Overlap-weighted root/family/quality with transition precision/recall.

    Unknown roots and qualities never match one another. Unresolved rivals make
    the final relation unknown while selected-label diagnostics remain available.
    """
    raw_a, raw_b = _spans(reference), _spans(observed)
    a, b = merge_harmony_windows(raw_a), merge_harmony_windows(raw_b)
    if not a or not b:
        return _unknown("missing harmonic spans", raw_reference=raw_a, raw_observed=raw_b)
    boundaries = sorted({x[k] for x in a + b for k in ("start_beat", "end_beat")})
    alignment, total, covered, root_weight, family_weight, exact_weight = [], 0., 0., 0., 0., 0.
    ia = ib = 0
    for start, end in zip(boundaries, boundaries[1:]):
        while ia < len(a) and a[ia]["end_beat"] <= start:
            ia += 1
        while ib < len(b) and b[ib]["end_beat"] <= start:
            ib += 1
        x = a[ia] if ia < len(a) and a[ia]["start_beat"] <= start else None
        y = b[ib] if ib < len(b) and b[ib]["start_beat"] <= start else None
        if x is None and y is None:
            continue
        width = end - start
        total += width
        both = x is not None and y is not None
        root = both and x.get("root_pc") is not None and x.get("root_pc") == y.get("root_pc")
        family = root and quality_family(x.get("quality")) is not None and quality_family(x.get("quality")) == quality_family(y.get("quality"))
        exact = family and x.get("quality") == y.get("quality")
        covered += width * both
        root_weight += width * root
        family_weight += width * family
        exact_weight += width * exact
        alignment.append({"start_beat": start, "end_beat": end,
                          "reference_index": ia if x else None, "observed_index": ib if y else None,
                          "root_equal": bool(root), "family_equal": bool(family), "quality_equal": bool(exact)})

    def transitions(spans, level):
        def label(s):
            if level == "root":
                return s.get("root_pc")
            return (s.get("root_pc"), quality_family(s.get("quality")) if level == "family" else s.get("quality"))
        return [{"beat": y["start_beat"], "from": label(x), "to": label(y)}
                for x, y in zip(spans, spans[1:])
                if abs(x["end_beat"] - y["start_beat"]) <= 1e-9 and label(x) != label(y)]

    transition_results = {}
    for level in ("root", "family", "exact"):
        ta, tb = transitions(a, level), transitions(b, level)
        pairs = _pairs(ta, tb, lambda x, y: abs(x["beat"] - y["beat"]) <= THRESHOLDS["transition_beats"] and x["from"] == y["from"] and x["to"] == y["to"])
        transition_results[level] = {**_prf(len(ta), len(tb), len(pairs)), "missing": len(ta) - len(pairs),
                                     "extra": len(tb) - len(pairs), "reference": ta, "observed": tb, "alignment": pairs}
    clean = lambda level: not (transition_results[level]["missing"] or transition_results[level]["extra"])
    cov, roots, families, exacts = covered / total, root_weight / total, family_weight / total, exact_weight / total
    relation = "free"
    if cov >= THRESHOLDS["harmony_ordered_coverage"] and roots >= THRESHOLDS["harmony_ordered_coverage"] and clean("root"):
        relation = "ordered"
    if cov >= THRESHOLDS["harmony_coverage"] and families >= THRESHOLDS["harmony_coverage"] and clean("family"):
        relation = "quality-family"
    if cov >= THRESHOLDS["harmony_coverage"] and exacts >= THRESHOLDS["harmony_coverage"] and clean("exact"):
        relation = "exact"
    result = {"schema": VERSION, "status": "observed", "relation": relation,
              "coverage": cov, "root_agreement": roots, "family_agreement": families,
              "exact_agreement": exacts, "union_beats": total, "alignment": alignment,
              "transitions": transition_results, "raw_reference": raw_a, "raw_observed": raw_b,
              "merged_reference": a, "merged_observed": b, "thresholds": dict(THRESHOLDS)}
    if any(s.get("rivals") for s in raw_a + raw_b):
        ceiling = 3
        ranks = ["free", "ordered", "quality-family", "exact"]
        for span in raw_a + raw_b:
            for rival in span.get("rivals", []):
                if not isinstance(rival, dict) or rival.get("root_pc", rival.get("root")) != span.get("root_pc"):
                    ceiling = -1
                elif quality_family(rival.get("quality")) != quality_family(span.get("quality")) or quality_family(span.get("quality")) is None:
                    ceiling = min(ceiling, 1)
                elif rival.get("quality") != span.get("quality"):
                    ceiling = min(ceiling, 2)
        result.update(status="ambiguous", relation=ranks[min(ranks.index(relation), ceiling)] if ceiling >= 0 else None,
                      selected_label_relation=relation, evidence_ceiling=ranks[ceiling] if ceiling >= 0 else None,
                      reason="unresolved harmonic rivals cap relation; selected-label match is not identification")
    elif any(all(s.get("root_pc") is None for s in route) for route in (raw_a, raw_b)):
        result.update(status="unknown", relation=None, reason="no known roots")
    return result


def _notes(rows, role=None):
    out = []
    for row in rows:
        if role is not None and row.get("role") != role:
            continue
        x = copy.deepcopy(row)
        if "pitch_midi" not in x:
            x["pitch_midi"] = x.get("midi")
        x["onset_beat"] = _number(x["onset_beat"], "onset_beat", 0)
        if "offset_beat" not in x and x.get("duration_beats") is not None:
            x["offset_beat"] = x["onset_beat"] + _number(x["duration_beats"], "duration_beats", 0)
        x["pitch_midi"] = _number(x["pitch_midi"], "pitch_midi", 0)
        if x["pitch_midi"] > 127:
            raise ValueError("pitch_midi above 127")
        if x.get("offset_beat") is not None:
            x["offset_beat"] = _number(x["offset_beat"], "offset_beat", 0)
            if x["offset_beat"] <= x["onset_beat"]:
                raise ValueError("nonpositive note")
        _confidence(x)
        out.append(x)
    return sorted(out, key=lambda x: (x["onset_beat"], x["pitch_midi"]))


def note_f1(reference, observed, role=None, onset_tolerance=0.12, pitch_tolerance=0.75):
    a, b = _notes(reference, role), _notes(observed, role)
    _number(onset_tolerance, "onset_tolerance", 0)
    _number(pitch_tolerance, "pitch_tolerance", 0)
    pairs = _pairs(a, b, lambda x, y: abs(x["onset_beat"] - y["onset_beat"]) <= onset_tolerance + 1e-9 and abs(x["pitch_midi"] - y["pitch_midi"]) <= pitch_tolerance + 1e-9)
    return {"schema": VERSION, "status": "observed" if a else "unknown", **_prf(len(a), len(b), len(pairs)),
            "alignment": pairs, "role": role, "onset_tolerance": onset_tolerance, "pitch_tolerance": pitch_tolerance}


def compare_motif(reference, observed, role=None):
    a, b = _notes(reference, role), _notes(observed, role)
    absolute = note_f1(a, b)
    if len(a) < 2 or len(b) < 2:
        return _unknown("at least two notes per statement required", notes=absolute)
    if any(y["onset_beat"] - x["onset_beat"] <= 1e-9 for seq in (a, b) for x, y in zip(seq, seq[1:])):
        return _unknown("onset collisions: select an evidence-backed role lane", notes=absolute)

    def normalized(seq):
        span = seq[-1]["onset_beat"] - seq[0]["onset_beat"]
        return [{"onset_beat": (x["onset_beat"] - seq[0]["onset_beat"]) / span,
                 "pitch_midi": x["pitch_midi"] - seq[0]["pitch_midi"]} for x in seq]

    na, nb = normalized(a), normalized(b)
    # Signed relative pitches intentionally bypass absolute MIDI validation.
    pairs = _pairs(na, nb, lambda x, y: abs(x["onset_beat"] - y["onset_beat"]) <= THRESHOLDS["motif_onset_fraction"] and abs(x["pitch_midi"] - y["pitch_midi"]) <= THRESHOLDS["motif_relative_pitch_semitones"])
    approximate = {**_prf(len(a), len(b), len(pairs)), "alignment": pairs,
                   "normalization": "first pitch and first-to-last-onset span; no local warping"}
    same_length = len(a) == len(b)
    metric = same_length and len(pairs) == len(a) and all(i == j for i, j in pairs)
    sign = lambda v: 0 if abs(v) <= 0.5 else (1 if v > 0 else -1)
    direction = lambda seq: [sign(y["pitch_midi"] - x["pitch_midi"]) for x, y in zip(seq, seq[1:])]
    onsets = same_length and all(abs(x["onset_beat"] - y["onset_beat"]) <= THRESHOLDS["motif_onset_fraction"] for x, y in zip(na, nb))
    theme = same_length and direction(a) == direction(b) and onsets
    durations_known = all(x.get("offset_beat") is not None for x in a + b)
    duration_equal = False
    if durations_known and same_length:
        durations = lambda seq: [x["offset_beat"] - x["onset_beat"] for x in seq]
        da, db = durations(a), durations(b)
        duration_equal = all(abs(x / sum(da) - y / sum(db)) <= THRESHOLDS["motif_duration_fraction"] for x, y in zip(da, db))
    # Audio comparison ceiling remains Metric. Durations are diagnostics.
    return {"schema": VERSION, "status": "observed", "relation": "metric" if metric else "theme" if theme else "free",
            "notes": absolute, "relative_note_match": approximate, "length_equal": same_length,
            "duration_equal": duration_equal, "durations_known": durations_known,
            "transposition_semitones": b[0]["pitch_midi"] - a[0]["pitch_midi"],
            "onset_span_ratio": (b[-1]["onset_beat"] - b[0]["onset_beat"]) / (a[-1]["onset_beat"] - a[0]["onset_beat"]),
            "role": role}


def compare_groove(reference, observed):
    unresolved = []
    def events(rows):
        out = []
        for row in rows:
            x = copy.deepcopy(row)
            x["onset_beat"] = _number(x["onset_beat"], "onset_beat", 0)
            _confidence(x)
            if x.get("family") in ("kick", "snare"):
                out.append(x)
            elif x.get("family") in (None, "unknown"):
                unresolved.append(x)
        return sorted(out, key=lambda x: (x["onset_beat"], x["family"]))
    a, b = events(reference), events(observed)
    if not a or not b:
        return _unknown("no identified kick/snare events on one route", reference_count=len(a), observed_count=len(b))
    def match(x, y):
        return x["family"] == y["family"] and abs(x["onset_beat"] - y["onset_beat"]) <= THRESHOLDS["groove_stroke_beats"]
    full = _pairs(a, b, match)
    def skeleton(rows):
        return [{**x, "raw_onset_beat": x["onset_beat"], "onset_beat": round(x["onset_beat"])} for x in rows
                if x.get("is_fill") is not True and x.get("is_ornament") is not True and x.get("fill_status") != "fill"
                and abs(x["onset_beat"] - round(x["onset_beat"])) <= THRESHOLDS["groove_quarter_beats"]]
    sa, sb = skeleton(a), skeleton(b)
    pocket = _pairs(sa, sb, lambda x, y: x["family"] == y["family"] and x["onset_beat"] == y["onset_beat"])
    both_families = {x["family"] for x in a} == {"kick", "snare"} and {x["family"] for x in b} == {"kick", "snare"}
    relation = "free"
    if sa and sb and len(pocket) == len(sa) == len(sb):
        relation = "pocket-skeleton"
    if both_families and len(full) == len(a) == len(b):
        relation = "kick-snare"
    result = {"schema": VERSION, "status": "observed", "relation": relation,
            "full": {**_prf(len(a), len(b), len(full)), "alignment": full},
            "pocket": {**_prf(len(sa), len(sb), len(pocket)), "alignment": pocket},
            "raw_reference": a, "raw_observed": b, "reference_skeleton": sa, "observed_skeleton": sb,
            "fill_policy": "exclude explicitly measured fills/ornaments from skeleton only; no inferred free passes"}
    if unresolved:
        affects_pocket = any(abs(x["onset_beat"] - round(x["onset_beat"])) <= THRESHOLDS["groove_quarter_beats"]
                             and x.get("is_fill") is not True and x.get("fill_status") != "fill" for x in unresolved)
        result.update(status="ambiguous", unresolved_events=unresolved, selected_label_relation=relation,
                      relation=None if affects_pocket else "pocket-skeleton" if relation == "kick-snare" and sa and sb and len(pocket) == len(sa) == len(sb) else relation,
                      reason="unclassified onsets may be kick/snare; not wildcard matches")
    return result


def compare_form(reference, observed, reference_links=None, observed_links=None):
    """Compare recurrence topology and independently detected section boundaries.

    Section `family`/`label` must be inferred recurrence families, never human
    semantic names. Explicit recurrence links override labels. No links/families
    means unknown, and an empty measured link set gives diagnostics but cannot
    establish a nontrivial recurrence topology.
    """
    def prepare(rows, links):
        sections = copy.deepcopy(list(rows))
        for s in sections:
            s["start_beat"] = _number(s["start_beat"], "section.start_beat", 0)
            s["end_beat"] = _number(s["end_beat"], "section.end_beat", 0)
            if s["end_beat"] <= s["start_beat"]:
                raise ValueError("nonpositive section span")
            _confidence(s)
        sections.sort(key=lambda x: (x["start_beat"], x["end_beat"]))
        if any(x["end_beat"] > y["start_beat"] + 1e-9 for x, y in zip(sections, sections[1:])):
            raise ValueError("overlapping section spans")
        if not sections:
            return sections, [], [], False
        supported = True
        if links is None:
            labels = [s.get("family", s.get("label")) for s in sections]
            supported = all(x is not None and x != "unknown" for x in labels)
        else:
            parent = list(range(len(sections)))
            def find(i):
                while parent[i] != i:
                    i = parent[i]
                return i
            def section_at(beat):
                return next((i for i, s in enumerate(sections) if s["start_beat"] <= beat < s["end_beat"]), None)
            matched_links = 0
            for link in links:
                at = _number(link["at_beat"], "link.at_beat", 0)
                to = _number(link["to_beat"], "link.to_beat", 0)
                similarity = _number(link["similarity"], "link.similarity", 0)
                if similarity > 1:
                    raise ValueError("link similarity above 1")
                i, j = section_at(at), section_at(to)
                if i is None or j is None:
                    raise ValueError("recurrence endpoint outside sections")
                if similarity >= THRESHOLDS["form_recurrence_floor"] and i != j:
                    parent[find(i)] = find(j)
                    matched_links += 1
            labels = [find(i) for i in range(len(sections))]
            supported = matched_links > 0 or len(sections) == 1
        ids, sequence = [], []
        for label in labels:
            if label not in ids:
                ids.append(label)
            sequence.append(ids.index(label))
        topology = [label for i, label in enumerate(sequence) if i == 0 or sequence[i - 1] != label]
        return sections, sequence, topology, supported

    a, fa, ta, supported_a = prepare(reference, reference_links)
    b, fb, tb, supported_b = prepare(observed, observed_links)
    if not a or not b:
        return _unknown("missing section route", raw_reference=a, raw_observed=b)
    # Internal boundaries, with separate outer extent residuals; silence gaps have
    # two edges and count as two independently measurable boundaries.
    edges = lambda s: sorted({x[k] for x in s for k in ("start_beat", "end_beat")} - {s[0]["start_beat"], s[-1]["end_beat"]})
    ea, eb = edges(a), edges(b)
    pairs = _pairs(ea, eb, lambda x, y: abs(x - y) <= THRESHOLDS["form_boundary_beats"])
    extent_error = {"start_beats": abs(a[0]["start_beat"] - b[0]["start_beat"]),
                    "end_beats": abs(a[-1]["end_beat"] - b[-1]["end_beat"])}
    spans_equal = len(a) == len(b) and all(abs(x[k] - y[k]) <= THRESHOLDS["form_boundary_beats"]
                                                        for x, y in zip(a, b) for k in ("start_beat", "end_beat"))
    topology_equal = ta == tb
    # The audio evidence ceiling remains Topology, even if all boundaries agree.
    relation = "topology" if topology_equal else "free"
    result = {"schema": VERSION, "status": "observed", "relation": relation,
              "topology_equal": topology_equal, "spans_equal": spans_equal,
              "reference_topology": ta, "observed_topology": tb,
              "reference_family_sequence": fa, "observed_family_sequence": fb,
              "boundaries": {**_prf(len(ea), len(eb), len(pairs)), "alignment": pairs,
                             "missing": len(ea) - len(pairs), "extra": len(eb) - len(pairs),
                             "reference": ea, "observed": eb}, "extent_error": extent_error,
              "raw_reference": a, "raw_observed": b,
              "reference_links": copy.deepcopy(reference_links), "observed_links": copy.deepcopy(observed_links)}
    if not supported_a or not supported_b:
        result.update(status="unknown", relation=None, diagnostic_relation=relation,
                      reason="no identified recurrence families or no nontrivial link support on one route")
    return result


def compare_motif_occurrences(reference, observed, occurrences, role=None):
    """Evaluate *every* source-declared occurrence, with no candidate-based search.

    Occurrences use start_beat/end_beat in the common metric frame. Caller selects
    the source family from source recurrence evidence alone. Overlapping windows
    are allowed but disclosed and must not be counted as independent witnesses.
    Empty candidate windows are failures when source notes establish a statement.
    """
    a, b = _notes(reference, role), _notes(observed, role)
    windows = copy.deepcopy(list(occurrences))
    rows = []
    for i, window in enumerate(windows):
        start = _number(window["start_beat"], "occurrence.start_beat", 0)
        end = _number(window["end_beat"], "occurrence.end_beat", 0)
        if end <= start:
            raise ValueError("nonpositive motif occurrence")
        _confidence(window)
        left = [n for n in a if start <= n["onset_beat"] < end]
        right = [n for n in b if start <= n["onset_beat"] < end]
        comparison = compare_motif(left, right)
        relative_f1 = comparison.get("relative_note_match", {}).get("f1", 0.)
        source_identified = len(left) >= 2 and all(y["onset_beat"] > x["onset_beat"] + 1e-9 for x, y in zip(left, left[1:]))
        rows.append({"occurrence_index": i, "window": window, "reference_count": len(left),
                     "observed_count": len(right), "source_identified": source_identified,
                     "relative_note_f1": relative_f1, "comparison": comparison})
    if not rows:
        return _unknown("no source-declared motif occurrences", occurrences=[])
    relations = [r["comparison"].get("relation") for r in rows]
    metrics = sum(r == "metric" for r in relations)
    themes = sum(r in ("theme", "metric") for r in relations)
    source_identified = all(r["source_identified"] for r in rows)
    relation = "metric" if metrics == len(rows) else "theme" if themes == len(rows) else "free"
    intervals = sorted((float(w["start_beat"]), float(w["end_beat"])) for w in windows)
    overlap = any(y[0] < x[1] for x, y in zip(intervals, intervals[1:]))
    f1s = [r["relative_note_f1"] for r in rows]
    return {"schema": VERSION, "status": "observed" if source_identified else "unknown",
            "relation": relation if source_identified else None,
            "reason": "all source-declared occurrences evaluated; missing candidate statements fail" if source_identified else "source occurrence lacks an identified monophonic statement",
            "occurrence_count": len(rows), "metric_count": metrics, "theme_or_better_count": themes,
            "metric_fraction": metrics / len(rows), "theme_or_better_fraction": themes / len(rows),
            "minimum_relative_note_f1": min(f1s), "median_relative_note_f1": statistics.median(f1s),
            "overlapping_windows": overlap, "occurrences": rows,
            "selection_policy": "source-only family and exact windows; no observed-family selection, window shifting or best-subset reporting",
            "whole_line": compare_motif(a, b)}

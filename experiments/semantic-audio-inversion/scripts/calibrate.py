"""Calibrate the inspectable DSP instrument on fixtures with known answers.

Run after `make_fixtures.py`:

    python3 scripts/make_fixtures.py
    python3 scripts/calibrate.py --fixtures fixtures --out /tmp/opencode/sai/calibration

Every reading this instrument makes on unknown audio is only as trustworthy as these known-case
readings. A control that fails is a residual on the instrument, not a threshold to tune away.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

import numpy as np

_HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.abspath(os.path.join(_HERE, ".."))
sys.path.insert(0, os.path.join(_ROOT, "adapters"))

from baseline_dsp import BaselineDSP, DspConfig  # noqa: E402
from canonicalize import canonicalize  # noqa: E402


def analyze(path: str) -> dict:
    audio = canonicalize(path)
    d = BaselineDSP(audio.samples, audio.sample_rate, DspConfig())
    onset_dicts, _provs, cross = d.detect_onsets()
    onset_dicts.sort(key=lambda o: o["second"])
    timing_ev, timing_meta = d.timing_and_beats(onset_dicts)
    chroma_list, chroma, _centers = d.chroma()
    notes, pitch_meta = d.transcribe(onset_dicts)
    perc, perc_counts = d.percussion(onset_dicts)
    top_pcs = []
    if chroma.size:
        agg = chroma.mean(axis=0)
        top_pcs = list(np.argsort(agg)[::-1][:4])
    keys = d.key(chroma) if chroma.size else []
    return {
        "duration": audio.duration_seconds,
        "onsets": len(onset_dicts),
        "cross": cross,
        "tempo_bpm": timing_ev["tempo_bpm"],
        "beats": [b["second"] for b in timing_ev["beats"]],
        "top_pcs": [int(p) for p in top_pcs],
        "keys": keys,
        "notes": notes,
        "pitch_meta": pitch_meta,
        "percussion_counts": perc_counts,
        "groove_strokes": sum(1 for p in perc if any(f["family"] in ("kick", "snare") for f in p["family_candidates"])),
    }


def folded(t: float | None, target: float) -> float | None:
    if not t:
        return None
    r = t / target
    return r / (2.0 ** round(np.log2(r)))


def check(name: str, a: dict) -> tuple[bool, list[str]]:
    notes: list[str] = []
    ok = True

    def expect(cond: bool, msg: str) -> None:
        nonlocal ok
        notes.append(("PASS " if cond else "FAIL ") + msg)
        ok = ok and cond

    if name == "positive_tone_a4.wav":
        expect(a["top_pcs"][0] == 9, f"top chroma pc is A(9): got {a['top_pcs'][:4]}")
        pitches = [n["pitch_midi"] for n in a["notes"]]
        near = [p for p in pitches if abs(p - 69.0) <= 0.6]
        expect(len(near) >= 1, f"recovered a pitch near A4(69): got {[round(p,2) for p in pitches[:6]]}")
        expect(bool(a["keys"]) and a["keys"][0]["tonic_pc"] == 9, f"key tonic is A: got {a['keys'][:1]}")
    elif name == "positive_triad_c_maj.wav":
        expect(set(a["top_pcs"][:3]) == {0, 4, 7}, f"top-3 chroma pcs are C,E,G: got {a['top_pcs'][:4]}")
        pitches = [round(n["pitch_midi"]) for n in a["notes"]]
        hit = sum(1 for p in pitches if any(abs(p - t) <= 0 for t in (60, 64, 67)))
        expect(hit >= 2, f"at least two triad notes recovered: got {pitches[:8]}")
    elif name == "positive_click_120.wav":
        f = folded(a["tempo_bpm"], 120.0)
        expect(f is not None and abs(f - 1.0) <= 0.03, f"tempo ~120 bpm: got {a['tempo_bpm']}")
        expect(len(a["beats"]) >= 14, f"at least 14 beats in 8 s: got {len(a['beats'])}")
        if a["beats"]:
            errs = [abs((b / 0.5) - round(b / 0.5)) for b in a["beats"]]
            expect(float(np.median(errs)) <= 0.1, f"beats on the 0.5 s grid: median err {float(np.median(errs)):.3f}")
    elif name == "positive_kicksnare_120.wav":
        expect(a["percussion_counts"].get("kick", 0) >= 1, f"kick detected: {a['percussion_counts']}")
        expect(a["percussion_counts"].get("snare", 0) >= 1, f"snare detected: {a['percussion_counts']}")
        expect(a["groove_strokes"] >= 2, f"kick/snare strokes: {a['groove_strokes']}")
        f = folded(a["tempo_bpm"], 120.0)
        expect(f is not None and abs(f - 1.0) <= 0.05, f"tempo ~120 bpm: got {a['tempo_bpm']}")
    elif name == "null_silence.wav":
        expect(a["onsets"] <= 3, f"no onsets on silence (null control): got {a['onsets']}")
        expect(not a["tempo_bpm"], f"no tempo claimed on silence: got {a['tempo_bpm']}")
        expect(len(a["notes"]) == 0, f"no notes on silence: got {len(a['notes'])}")
    elif name == "negative_white_noise.wav":
        conf = a["keys"][0]["confidence"] if a["keys"] else 0.0
        expect(conf < 0.72, f"no confident key on noise (negative control): top key conf {conf:.3f}")
    elif name == "negative_sweep.wav":
        expect(a["duration"] > 2.5, "sweep decoded with expected duration")
        conf = a["keys"][0]["confidence"] if a["keys"] else 0.0
        expect(conf < 0.85, f"sweep key is not near-certain: {conf:.3f}")
    elif name == "mutation_triad_d_maj.wav":
        expect(set(a["top_pcs"][:3]) == {2, 6, 9}, f"transposed top-3 pcs are D,F#,A: got {a['top_pcs'][:4]}")
    else:
        expect(False, f"unknown fixture {name}")
    return ok, notes


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fixtures", default=os.path.join(_ROOT, "fixtures"))
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    names = sorted(n for n in os.listdir(args.fixtures) if n.endswith(".wav"))
    report = []
    for name in names:
        a = analyze(os.path.join(args.fixtures, name))
        ok, notes = check(name, a)
        report.append({"fixture": name, "ok": ok, "checks": notes, "readings": {
            "tempo_bpm": a["tempo_bpm"], "onsets": a["onsets"], "beats": len(a["beats"]),
            "top_pcs": a["top_pcs"], "notes": len(a["notes"]), "percussion": a["percussion_counts"],
        }})
        print(f"{'OK  ' if ok else 'FAIL'} {name}")
        for n in notes:
            print(f"      {n}")

    passed = sum(1 for r in report if r["ok"])
    doc = {"schema": "sai.calibration/v1", "fixtures": args.fixtures, "passed": passed, "total": len(report), "report": report}
    with open(os.path.join(args.out, "calibration_report.json"), "w", encoding="utf-8") as f:
        json.dump(doc, f, indent=2)
    lines = [f"# Instrument calibration — {passed}/{len(report)} controls pass", ""]
    for r in report:
        lines.append(f"## {'PASS' if r['ok'] else 'FAIL'} `{r['fixture']}`")
        for c in r["checks"]:
            lines.append(f"- {c}")
        lines.append(f"- readings: `{json.dumps(r['readings'])}`")
        lines.append("")
    with open(os.path.join(args.out, "calibration_report.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"{passed}/{len(report)} controls pass")
    return 0 if passed == len(report) else 1


if __name__ == "__main__":
    raise SystemExit(main())

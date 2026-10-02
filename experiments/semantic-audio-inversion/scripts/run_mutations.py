"""Run the hostile mutation suite and check preregistered expectations.

Usage:
    python3 scripts/run_mutations.py --mutations /tmp/opencode/sai/mutations \\
        --base-run /tmp/opencode/sai/dev-run --out /tmp/opencode/sai/mutations-run --profile interpretive

For each mutated item we analyze it exactly like a fresh source, recover a quotient, evaluate
against the *mutated* truth, then compare to the **base** item's quotient. Two notions of "the
axis moved" are reported, because they are not the same and the difference is a finding:

- `content_change`: the recovered musical content the axis projects changed (tempo estimate,
  note set, chord sequence, groove strokes, motif contour families);
- `metric_change`: the evaluator's residual value/status changed.

The preregistered expectation names evaluator axes, so the **metric witness is the authority** for
pass/fail. `content_drift` is recorded alongside as a robustness diagnostic: it is when our
recovered musical content moved even though the evaluator's relation did not (or vice versa),
which is itself evidence about whether the axis is measured or merely labelled.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
import sys
from collections import Counter, defaultdict

_HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.abspath(os.path.join(_HERE, ".."))
sys.path.insert(0, os.path.join(_ROOT, "adapters"))

import run_analysis  # noqa: E402


def run_bin(bin_path: str, args: list) -> None:
    res = subprocess.run([bin_path] + args, capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"sai {' '.join(args)} failed: {res.stderr.strip()}")


def folded_tempo(t: float | None) -> float | None:
    return None if not t else math.log2(t)


def content_sig(q: dict, axis: str):
    if axis == "timing.tempo":
        return round(folded_tempo(q.get("tempo_bpm")) or 0.0, 4)
    if axis in ("notes.all", "notes.lead"):
        return tuple(sorted(round(n["pitch_midi"]) for n in q.get("notes", [])))
    if axis in ("harmony.root", "harmony.relation"):
        return tuple((c["root_pc"], c["quality"]) for c in q.get("chords", []))
    if axis == "motif.relation":
        fams = q.get("motif_families", [])
        return (
            q["motif"]["effective"],
            tuple(
                (
                    f["support"],
                    tuple(round(p - f["representative"]["pitches"][0]) for p in f["representative"]["pitches"]),
                )
                for f in fams
            ),
        )
    if axis == "groove.relation":
        return tuple((s["voice"], round(s["at_beat"], 1)) for s in q.get("groove_strokes", []))
    if axis == "form.topology":
        return tuple(s["family"] for s in q.get("sections", []))
    return None


def metric_sig(m: dict, axis: str):
    for a in m.get("axes", []):
        if a["axis"] == axis:
            return (a["status"], None if a["value"] is None else round(a["value"], 4))
    return None


def differs(a, b, axis: str) -> bool:
    if a is None or b is None:
        return a != b
    if axis == "timing.tempo":
        if isinstance(a, tuple) and isinstance(b, tuple):
            if a[0] != b[0]:
                return True
            if a[1] is None or b[1] is None:
                return a[1] != b[1]
            return abs(float(a[1]) - float(b[1])) > 0.02
        return abs(float(a) - float(b)) > 0.03
    return a != b


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mutations", required=True)
    ap.add_argument("--base-run", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--profile", default="interpretive")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--root", default=_ROOT)
    args = ap.parse_args()

    sai_bin = os.path.join(args.root, "target", "debug", "sai")
    if not os.path.isfile(sai_bin):
        raise SystemExit(f"missing {sai_bin}; build `cargo build -p sai-core --bin sai` first")

    with open(os.path.join(args.mutations, "mutations.json"), "r", encoding="utf-8") as f:
        suite = json.load(f)
    items = suite["items"]
    if args.limit:
        items = items[: args.limit]

    os.makedirs(os.path.join(args.out, "quotient"), exist_ok=True)
    os.makedirs(os.path.join(args.out, "metrics"), exist_ok=True)

    base_q_cache: dict = {}
    base_m_cache: dict = {}

    def base_q(sid: str) -> dict:
        if sid not in base_q_cache:
            p = os.path.join(args.base_run, "quotient", f"{sid}.{args.profile}.json")
            with open(p, "r", encoding="utf-8") as f:
                base_q_cache[sid] = json.load(f)
        return base_q_cache[sid]

    def base_m(sid: str) -> dict:
        if sid not in base_m_cache:
            p = os.path.join(args.base_run, "metrics", f"{sid}.{args.profile}.json")
            with open(p, "r", encoding="utf-8") as f:
                base_m_cache[sid] = json.load(f)
        return base_m_cache[sid]

    results = []
    per_kind = defaultdict(lambda: {"pass": 0, "fail": 0, "axes": Counter()})
    for it in items:
        label = it["id"]
        wav = os.path.join(args.mutations, it["wav_path"])
        truth = os.path.join(args.mutations, it["truth_path"])
        run_analysis.analyze_one(wav, args.out, True, "mutations")
        ev = os.path.join(args.out, "evidence", f"{label}.evidence.json")
        qp = os.path.join(args.out, "quotient", f"{label}.{args.profile}.json")
        mp = os.path.join(args.out, "metrics", f"{label}.{args.profile}.json")
        run_bin(sai_bin, ["recover", "--evidence", ev, "--profile", args.profile, "--out", qp])
        run_bin(sai_bin, ["evaluate", "--quotient", qp, "--truth", truth, "--out", mp])
        with open(qp, "r", encoding="utf-8") as f:
            q = json.load(f)
        with open(mp, "r", encoding="utf-8") as f:
            m = json.load(f)
        bq = base_q(it["source_id"])
        bm = base_m(it["source_id"])
        exp = it["expectation"]
        checks = {"must_change": {}, "must_hold": {}}
        ok = True
        for axis in exp.get("must_change", []):
            cc = differs(content_sig(q, axis), content_sig(bq, axis), axis)
            mc = differs(metric_sig(m, axis), metric_sig(bm, axis), axis)
            # The preregistered expectation names an evaluator axis, so the metric witness is the
            # authority; content drift is recorded as a separate robustness diagnostic.
            passed = mc
            checks["must_change"][axis] = {"metric_changed": mc, "content_drift": cc, "ok": passed}
            if not passed:
                ok = False
        for axis in exp.get("must_hold", []):
            cc = differs(content_sig(q, axis), content_sig(bq, axis), axis)
            mc = differs(metric_sig(m, axis), metric_sig(bm, axis), axis)
            passed = not mc
            checks["must_hold"][axis] = {"metric_changed": mc, "content_drift": cc, "ok": passed}
            if not passed:
                ok = False
        per_kind[it["mutation"]["kind"]]["pass" if ok else "fail"] += 1
        results.append(
            {
                "id": label,
                "source_id": it["source_id"],
                "mutation": it["mutation"],
                "expectation_ok": ok,
                "checks": checks,
            }
        )

    doc = {
        "schema": "sai.mutations.results/v1",
        "profile": args.profile,
        "suite": os.path.abspath(args.mutations),
        "base_run": os.path.abspath(args.base_run),
        "n": len(results),
        "overall_pass": sum(1 for r in results if r["expectation_ok"]),
        "per_kind": {k: {"pass": v["pass"], "fail": v["fail"]} for k, v in sorted(per_kind.items())},
        "results": results,
    }
    with open(os.path.join(args.out, "mutation_results.json"), "w", encoding="utf-8") as f:
        json.dump(doc, f, indent=2)

    # Human-readable report, including the content-drift diagnostic per mutation kind.
    drift = defaultdict(lambda: {"checks": 0, "drifted": 0, "witnessed": 0})
    for r in results:
        kind = r["mutation"]["kind"]
        for group in ("must_change", "must_hold"):
            for chk in r["checks"].get(group, {}).values():
                drift[kind]["checks"] += 1
                drift[kind]["drifted"] += 1 if chk.get("content_drift") else 0
                drift[kind]["witnessed"] += 1 if chk.get("metric_changed") else 0
    lines = ["# Mutation suite — preregistered expectation check", ""]
    lines.append(f"profile: `{args.profile}`  items: {doc['n']}  passed: {doc['overall_pass']}/{doc['n']}")
    lines.append("")
    lines.append("| mutation kind | pass | fail | checks | content drifted | metric witnessed |")
    lines.append("|---|---|---|---|---|---|")
    for k in sorted(per_kind):
        d = drift[k]
        lines.append(
            f"| {k} | {per_kind[k]['pass']} | {per_kind[k]['fail']} | {d['checks']} | {d['drifted']} | {d['witnessed']} |"
        )
    lines.append("")
    lines.append("A `must_change` failure means the evaluator's relation axis did not move; a "
                 "`must_hold` failure means it moved when the hidden score did not. `content drifted` "
                 "counts checks where the recovered musical content moved even though the metric "
                 "witness did not — i.e. the analyzer *did* see the change but the axis metric cannot "
                 "express it.")
    with open(os.path.join(args.out, "mutation_report.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")

    print(f"expectations passed {doc['overall_pass']}/{doc['n']}")
    for k, v in doc["per_kind"].items():
        print(f"  {k:18s} pass={v['pass']} fail={v['fail']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

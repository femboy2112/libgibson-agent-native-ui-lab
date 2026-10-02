"""Reproducible experiment driver: analysis -> recover -> evaluate -> aggregate.

This is the one command that reproduces the development measurement round:

    python3 scripts/run_experiment.py --dataset /tmp/opencode/sai/dev \\
        --out /tmp/opencode/sai/dev-run --split dev --profiles interpretive,faithful

It never runs a neural model. It runs the inspectable baseline (adapters/run_analysis.py helpers)
to emit evidence, then the Rust `sai` binary to recover a quotient and evaluate it against the
hidden truth held in `<dataset>/truth/`. Outputs:

    <out>/evidence/...            sai.evidence/v1
    <out>/receipts/...            sai.receipt/v1
    <out>/quotient/<id>.<profile>.json
    <out>/metrics/<id>.<profile>.json
    <out>/summary.json            per-axis aggregation
    <out>/summary.md              the same, human-readable
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import Counter, defaultdict

_HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.abspath(os.path.join(_HERE, ".."))
sys.path.insert(0, os.path.join(_ROOT, "adapters"))

import run_analysis  # noqa: E402


def find_or_build_sai(root: str) -> str:
    for cand in (os.path.join(root, "target", "debug", "sai"), os.path.join(root, "target", "release", "sai")):
        if os.path.isfile(cand):
            return cand
    subprocess.run(["cargo", "+1.98.1", "build", "-q", "-p", "sai-core", "--bin", "sai"], cwd=root, check=True)
    return os.path.join(root, "target", "debug", "sai")


def run_bin(bin_path: str, args: list) -> None:
    res = subprocess.run([bin_path] + args, capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"sai {' '.join(args)} failed: {res.stderr.strip()}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--split", default="dev")
    ap.add_argument("--profiles", default="interpretive,faithful")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--root", default=_ROOT)
    args = ap.parse_args()

    profiles = [p.strip() for p in args.profiles.split(",") if p.strip()]
    sai_bin = find_or_build_sai(args.root)

    with open(os.path.join(args.dataset, "manifest.json"), "r", encoding="utf-8") as f:
        manifest = json.load(f)
    items = [it for it in manifest.get("items", []) if args.split == "all" or it.get("split") == args.split]
    if args.limit:
        items = items[: args.limit]
    if not items:
        print(f"no items for split={args.split}", file=sys.stderr)
        return 2

    os.makedirs(os.path.join(args.out, "quotient"), exist_ok=True)
    os.makedirs(os.path.join(args.out, "metrics"), exist_ok=True)

    per_axis = defaultdict(lambda: {"status": Counter(), "values": []})
    effective = defaultdict(Counter)
    records = []
    for it in items:
        iid = os.path.basename(it["id"])
        wav = os.path.join(args.dataset, "wav", iid + ".wav")
        if not os.path.isfile(wav):
            wav = os.path.join(args.dataset, "wav", it["id"])
        res = run_analysis.analyze_one(wav, args.out, args.force, "driver")
        ev_path = os.path.join(args.out, "evidence", f"{iid}.evidence.json")
        truth_path = os.path.join(args.dataset, "truth", f"{iid}.json")
        rec = {"id": iid, "split": it.get("split"), "world": it.get("world"), "kind": it.get("kind")}
        for prof in profiles:
            q_path = os.path.join(args.out, "quotient", f"{iid}.{prof}.json")
            m_path = os.path.join(args.out, "metrics", f"{iid}.{prof}.json")
            run_bin(sai_bin, ["recover", "--evidence", ev_path, "--profile", prof, "--out", q_path])
            run_bin(sai_bin, ["evaluate", "--quotient", q_path, "--truth", truth_path, "--out", m_path])
            with open(q_path, "r", encoding="utf-8") as f:
                q = json.load(f)
            with open(m_path, "r", encoding="utf-8") as f:
                m = json.load(f)
            for a in m["axes"]:
                key = f"{prof}:{a['axis']}"
                per_axis[key]["status"][a["status"]] += 1
                if a["value"] is not None:
                    per_axis[key]["values"].append(a["value"])
            for axis in ("motif", "harmony", "groove", "form", "orchestration"):
                effective[f"{prof}:{axis}"][q[axis]["effective"]] += 1
            rec.setdefault("profiles", {})[prof] = {
                "tempo_bpm": q.get("tempo_bpm"),
                "effective": {ax: q[ax]["effective"] for ax in ("motif", "harmony", "groove", "form", "orchestration")},
                "notes": len(q.get("notes", [])),
                "chords": len(q.get("chords", [])),
                "motif_families": len(q.get("motif_families", [])),
                "false_unknown": m["false_unknown"],
                "false_free": m["false_free"],
                "false_present": m["false_present"],
            }
        records.append(rec)
        print(f"evaluated {iid}")

    def mean(xs):
        return round(sum(xs) / len(xs), 4) if xs else None

    summary = {
        "dataset": os.path.abspath(args.dataset),
        "split": args.split,
        "profiles": profiles,
        "items": records,
        "axes": {
            k: {
                "status_counts": dict(v["status"]),
                "mean_value": mean(v["values"]),
                "n": sum(v["status"].values()),
            }
            for k, v in sorted(per_axis.items())
        },
        "effective_relations": {k: dict(v) for k, v in sorted(effective.items())},
    }
    with open(os.path.join(args.out, "summary.json"), "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2)

    lines = [f"# Semantic Audio Inversion — {args.split} summary", ""]
    lines.append(f"dataset: `{summary['dataset']}`  items: {len(records)}  profiles: {', '.join(profiles)}")
    lines.append("")
    lines.append("| axis | ok | mismatch | unknown | absent | mean value |")
    lines.append("|---|---|---|---|---|---|")
    for k, v in summary["axes"].items():
        sc = v["status_counts"]
        lines.append(
            f"| {k} | {sc.get('ok',0)} | {sc.get('mismatch',0)} | {sc.get('unknown',0)} | {sc.get('absent',0)} | {v['mean_value']} |"
        )
    lines.append("")
    for k, v in summary["effective_relations"].items():
        lines.append(f"- {k}: {v}")
    with open(os.path.join(args.out, "summary.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote summary to {args.out}/summary.json and summary.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

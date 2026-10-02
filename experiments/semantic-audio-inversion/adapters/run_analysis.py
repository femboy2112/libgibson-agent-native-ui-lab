"""Run the inspectable baseline over a generated dataset, emitting evidence + receipts.

Usage:
    python3 adapters/run_analysis.py --dataset /tmp/opencode/sai/dev --split dev --out /tmp/opencode/sai/dev-evidence

For each manifest item the analyzer sees only the WAV path and produces:
    <out>/evidence/<id>.evidence.json   (schema sai.evidence/v1)
    <out>/receipts/<id>.receipt.json    (schema sai.receipt/v1)
    <out>/index.json                    (id -> hashes, analyzer meta)

Caching: if an evidence file already exists whose `source.sha256` matches the WAV, the run is
served from cache (`analyzers[].cached = true`) unless `--force` is given. This is what makes the
cheap replay path (`sai recover` over cached evidence) possible without re-decoding audio.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
from datetime import datetime, timezone

_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

from baseline_dsp import ANALYZER_ID, ANALYZER_VERSION, BaselineDSP, DspConfig  # noqa: E402
from canonicalize import CanonicalizeError, canonicalize, sha256_bytes, sha256_file  # noqa: E402
import sai_evidence as ev  # noqa: E402


def _git_commit(repo: str) -> str:
    try:
        import subprocess

        out = subprocess.run(
            ["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True, timeout=20
        )
        if out.returncode == 0:
            return out.stdout.strip()
    except Exception:
        pass
    return "unknown"


def _artifact_hash(obj: dict) -> str:
    # Canonical hash: stable key order, no whitespace dependence.
    blob = json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(blob)


def analyze_one(wav_path: str, out_dir: str, force: bool, git_commit: str) -> dict:
    item_id = os.path.splitext(os.path.basename(wav_path))[0]
    evidence_path = os.path.join(out_dir, "evidence", f"{item_id}.evidence.json")
    receipt_path = os.path.join(out_dir, "receipts", f"{item_id}.receipt.json")
    src_hash = sha256_file(wav_path)

    if not force and os.path.isfile(evidence_path):
        try:
            with open(evidence_path, "r", encoding="utf-8") as f:
                cached = json.load(f)
            if cached.get("source", {}).get("sha256") == src_hash:
                cached["analyzers"] = [
                    {**a, "cached": True} for a in cached.get("analyzers", [])
                ]
                with open(evidence_path, "w", encoding="utf-8") as f:
                    json.dump(cached, f, indent=2)
                    f.write("\n")
                return {"id": item_id, "cached": True, "evidence": evidence_path}
        except Exception:
            pass  # fall through to a fresh, authoritative run

    t0 = time.time()
    audio = canonicalize(wav_path)
    dsp = BaselineDSP(audio.samples, audio.sample_rate, DspConfig())
    body = dsp.analyze()
    wall = time.time() - t0

    body["analyzers"] = [
        {**a, "wall_seconds": wall, "cached": False} for a in body.get("analyzers", [])
    ]
    artifact = ev.artifact(
        source=audio.receipt_fields(path_hint=os.path.basename(wav_path)),
        analyzers=body["analyzers"],
        timing_ev=body["timing"],
        notes=body["notes"],
        tonal_ev=body["tonal"],
        onsets=body["onsets"],
        recurrence_ev=body["recurrence"],
        sections_ev=body["sections"],
        refusals=[],  # analyzers made no refusals; the core records orchestration's
        unknowns=body["unknowns"],
    )
    os.makedirs(os.path.dirname(evidence_path), exist_ok=True)
    os.makedirs(os.path.dirname(receipt_path), exist_ok=True)
    ev.write_json(evidence_path, artifact)

    art_hash = _artifact_hash(artifact)
    rec = ev.receipt(
        source=artifact["source"],
        analyzers=artifact["analyzers"],
        artifact_sha256=art_hash,
        git_commit=git_commit,
        created_utc=datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        warnings=[],
    )
    ev.write_json(receipt_path, rec)
    return {
        "id": item_id,
        "cached": False,
        "evidence": evidence_path,
        "receipt": receipt_path,
        "wall_seconds": wall,
        "artifact_sha256": art_hash,
        "meta": body.get("_meta", {}),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", required=True, help="dataset dir with manifest.json and wav/")
    ap.add_argument("--split", default="dev", choices=["dev", "holdout", "all"])
    ap.add_argument("--out", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--repo", default=os.path.abspath(os.path.join(_HERE, "..", "..", "..")))
    args = ap.parse_args()

    manifest_path = os.path.join(args.dataset, "manifest.json")
    with open(manifest_path, "r", encoding="utf-8") as f:
        manifest = json.load(f)
    items = manifest.get("items", [])
    if args.split != "all":
        items = [it for it in items if it.get("split") == args.split]
    if args.limit:
        items = items[: args.limit]
    if not items:
        print(f"no items for split={args.split} in {manifest_path}", file=sys.stderr)
        return 2

    commit = _git_commit(args.repo)
    index = {"dataset": os.path.abspath(args.dataset), "split": args.split, "git_commit": commit, "items": []}
    for it in items:
        wav = os.path.join(args.dataset, "wav", os.path.basename(it["id"]) + ".wav")
        if not os.path.isfile(wav):
            # manifest may already carry the full filename
            alt = os.path.join(args.dataset, "wav", it["id"])
            wav = alt if os.path.isfile(alt) else wav
        try:
            res = analyze_one(wav, args.out, args.force, commit)
        except CanonicalizeError as e:
            print(f"REFUSED {it.get('id')}: {e}", file=sys.stderr)
            return 3
        index["items"].append(res)
        print(f"{'cached ' if res['cached'] else 'analyzed'} {res['id']}")
    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, "index.json"), "w", encoding="utf-8") as f:
        json.dump(index, f, indent=2)
        f.write("\n")
    print(f"wrote {len(index['items'])} evidence artifacts to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

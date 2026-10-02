"""Seal a generated holdout manifest without analyzing it.

The holdout is generated under its own seed base and is committed only as a manifest + hash
commitment. Analysis is deferred to a later round so the development loop cannot see it.

    python3 scripts/seal_holdout.py --dataset /tmp/opencode/sai/holdout --out holdout
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
from datetime import datetime, timezone


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    manifest_path = os.path.join(args.dataset, "manifest.json")
    with open(manifest_path, "r", encoding="utf-8") as f:
        manifest = json.load(f)
    items = manifest.get("items", [])
    commitment = [
        {"id": it["id"], "wav_sha256": it["wav_sha256"], "truth_sha256": it["truth_sha256"]}
        for it in sorted(items, key=lambda x: x["id"])
    ]
    commitment_bytes = json.dumps(commitment, sort_keys=True, separators=(",", ":")).encode()
    seal = {
        "schema": "sai.holdout.seal/v1",
        "anchor_rev": manifest.get("anchor_rev"),
        "split": "holdout",
        "n": len(items),
        "seed_base": min(i["seed"] for i in items) if items else None,
        "manifest_sha256": sha256_file(manifest_path),
        "commitment_sha256": hashlib.sha256(commitment_bytes).hexdigest(),
        "sealed_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "note": (
            "Holdout generated under a distinct seed base and committed as a manifest + hash "
            "commitment only. The development analyzer has not seen these WAVs; analysis is "
            "deferred until schemas, relations and thresholds are frozen."
        ),
    }
    shutil.copyfile(manifest_path, os.path.join(args.out, "manifest.json"))
    with open(os.path.join(args.out, "SEAL.json"), "w", encoding="utf-8") as f:
        json.dump(seal, f, indent=2)
        f.write("\n")
    print(json.dumps(seal, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

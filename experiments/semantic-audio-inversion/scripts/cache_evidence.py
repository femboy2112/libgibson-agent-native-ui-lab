"""Emit cached evidence + receipts for the committed calibration fixtures.

This produces the small, in-repo replay corpus under `cached/`:

    cached/evidence/<name>.evidence.json   (sai.evidence/v1)
    cached/receipts/<name>.receipt.json    (sai.receipt/v1)
    cached/index.json

Ordinary CI can run `sai recover` over these artifacts without decoding any audio and without
running any inference. Regenerate only when the fixtures or the analyzer change:

    python3 scripts/cache_evidence.py --fixtures fixtures --out cached
"""

from __future__ import annotations

import argparse
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.abspath(os.path.join(_HERE, ".."))
sys.path.insert(0, os.path.join(_ROOT, "adapters"))

import run_analysis  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fixtures", default=os.path.join(_ROOT, "fixtures"))
    ap.add_argument("--out", default=os.path.join(_ROOT, "cached"))
    ap.add_argument("--repo", default=_ROOT)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    names = sorted(n for n in os.listdir(args.fixtures) if n.endswith(".wav"))
    for name in names:
        res = run_analysis.analyze_one(os.path.join(args.fixtures, name), args.out, True, "fixtures")
        print(f"cached {name} -> {res['evidence']}")
    print(f"wrote index to {os.path.join(args.out, 'index.json')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

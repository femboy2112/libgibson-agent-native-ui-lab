"""Bounded complete-corpus runner. Explicit render reuse; no holdout contact."""
import argparse
from concurrent.futures import ProcessPoolExecutor, as_completed
import json
from pathlib import Path
import subprocess

from .observe import write_json
from .run import evaluate_track


def campaign(root, renderer=None, workers=2, reuse_renders=False):
    root = Path(root)
    tracks = sorted(p.parent for p in root.glob("track-*/compiled_source_model.json"))
    if not tracks:
        raise ValueError("no imported real-source models; run corpus audit first")
    results, ready = {}, []
    for track in tracks:
        if not reuse_renders:
            if renderer is None:
                raise ValueError("renderer required unless --reuse-renders was explicitly selected")
            completed = subprocess.run([str(Path(renderer).resolve()), "cover",
                                        str(track/"compiled_source_model.json"), str(track),
                                        "220901,220902", "22050"], check=False)
            if completed.returncode:
                results[track.name] = {"status": "compilation_refused", "exit_code": completed.returncode,
                                       "receipt": str(track/"compilation_refusal.json")}
                continue
        if (track/"candidate_manifest.json").exists():
            ready.append(track)
        else:
            results[track.name] = {"status": "missing_candidate_manifest"}
    with ProcessPoolExecutor(max_workers=workers) as pool:
        jobs = {pool.submit(evaluate_track, track): track for track in ready}
        for future in as_completed(jobs):
            track = jobs[future]
            try:
                result = future.result()
                results[track.name] = {"status": "evaluated", "winner": result["ranking"]["winner"],
                                       "receipt": str(track/"ROUND2_RESULT.json")}
            except Exception as exc:
                results[track.name] = {"status": "evaluation_failed", "error": str(exc)}
            write_json(root/"campaign_receipt.json", {"schema": "sai.campaign/v2", "tracks": results,
                                                       "reuse_renders": reuse_renders})
    return results


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("root", type=Path)
    p.add_argument("--renderer", type=Path)
    p.add_argument("--workers", type=int, choices=range(1, 5), default=2)
    p.add_argument("--reuse-renders", action="store_true")
    args = p.parse_args()
    result = campaign(args.root, args.renderer, args.workers, args.reuse_renders)
    raise SystemExit(1 if any(r["status"] == "evaluation_failed" for r in result.values()) else 0)

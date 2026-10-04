"""Fresh six-item development isolation experiment. Never accepts holdout paths."""
import argparse
import json
from pathlib import Path
import statistics

from .observe import cached_observe, write_json
from .evaluate import compare


def observe_set(directory, *, frozen=True):
    d = Path(directory)
    result = {}
    for role in ("full", "lead", "bass", "drums", "keys", "pad", "support"):
        wav = d/"raw_mix.wav" if role == "full" else d/"stems"/(role+".wav")
        print(f"observe {d.name}/{role}", flush=True)
        result[role] = cached_observe(wav, d/"observations"/(role+".json"), role, frozen=frozen)
    return result


def run(directory):
    d = Path(directory)
    manifest = json.loads((d/"manifest.json").read_text())
    if manifest.get("schema") != "sai.round2_dev/v1" or manifest.get("holdout") is not False:
        raise ValueError("only explicitly marked fresh Round2 development manifests are accepted")
    rows = []
    for item in manifest["items"]:
        path = Path(item["path"])
        observations = observe_set(path)
        truth = json.loads((path/"truth.json").read_text())  # only after audio analysis
        result = compare(truth, observations)
        write_json(path/"residuals.json", result)
        rows.append({"id": item["id"], "world": item["world"], "pipeline_passes": item["pipeline_passes"],
                     **{k: result[k]["f1"] for k in result if k.startswith("frozen_notes")},
                     "yin_lead_f1": result["lead"]["f1"], "yin_bass_f1": result["bass"]["f1"],
                     "motif": result["motif"].get("relation"), "bass": result["bass_figure"].get("relation"),
                     "harmony": result["harmony"].get("relation"), "harmony_roots": result["harmony"].get("root_agreement"),
                     "full_harmony_roots": result["full_harmony"].get("root_agreement"),
                     "groove": result["groove"].get("relation"), "groove_f1": result["groove"].get("full", {}).get("f1")})
        print(json.dumps(rows[-1]), flush=True)
    summary = {"schema": "sai.round2_calibration/v1", "items": rows,
               "mean": {key: statistics.mean(r[key] for r in rows if isinstance(r.get(key), (int, float)))
                        for key in rows[0] if key.startswith(("frozen_notes", "yin_"))},
               "claim_scope": "six fresh development items; timing-conditioned evaluation; matched frozen DSP isolation contrast; YIN is separate instrument"}
    write_json(d.parent/"STEM_CALIBRATION.json", summary)
    return summary


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("directory")
    args = p.parse_args()
    run(args.directory)

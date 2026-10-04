"""Export compact hash/summary receipts; full event content stays local."""
import argparse
import json
from pathlib import Path

from .observe import write_json


def make_plot(directory, model, row):
    import matplotlib
    matplotlib.use("Agg")
    from matplotlib import pyplot as plt
    candidate = Path(row["path"])
    config = json.loads((candidate/"config.json").read_text())
    preroll = config.get("preroll_beats", 0)
    fig, axes = plt.subplots(3, 1, figsize=(12, 8), constrained_layout=True)
    for ax, role in zip(axes[:2], ("lead", "bass")):
        raw = json.loads((candidate/"observations"/(role+".json")).read_text())
        source = [n for n in model["notes"] if n["role"] == role]
        observed = raw["notes"]
        ax.scatter([n["onset_beat"]+preroll for n in source], [n["midi"] for n in source],
                   s=10, marker="o", facecolors="none", edgecolors="#7040b0", label="source measurement")
        ax.scatter([n["onset_second"]*model["tempo_bpm"]/60 for n in observed],
                   [n["pitch_midi"] for n in observed], s=9, marker="x", c="#009080", label="fresh PCM observation")
        ax.set(xlabel="Declared common beat coordinate", ylabel=f"{role} MIDI", title=f"{role}: absolute register shown; global octave freedom is reported separately")
        ax.legend(loc="upper right")
    metrics = row["residual"]
    keys = ["lead_octave_f1", "bass_octave_f1", "harmony_root_agreement", "harmony_family_agreement"]
    axes[2].barh(keys, [metrics.get(k) or 0 for k in keys], color="#7040b0")
    axes[2].set(xlim=(0, 1), xlabel="Measured agreement, not perceptual quality", title="Diagnostic residual components; failed/unknown relation gates remain failed")
    fig.suptitle(f"{model['source_id']} / {candidate.name} / {row['world']} / admitted={row['admitted']}")
    out = candidate/"plots"
    out.mkdir(exist_ok=True)
    fig.savefig(out/"observational_residuals.png", dpi=140)
    plt.close(fig)


def summarize(root, export=None, plots=False):
    root = Path(root)
    rows = []
    for result_path in sorted(root.glob("track-*/ROUND2_RESULT.json")) + sorted(root.glob("track-*/treatments/*/ROUND2_RESULT.json")):
        result = json.loads(result_path.read_text())
        directory = result_path.parent
        model_path = directory/"compiled_source_model.json"
        if not model_path.exists():
            model_path = directory/"renderer_source_snapshot.json"
        model = json.loads(model_path.read_text())
        candidates = result["candidates"]
        diagnostic = result["listening_diagnostic"]
        selected = next((r for r in candidates if r["candidate_id"] == diagnostic), None)
        entry = {"source_id": model["source_id"], "treatment": "native-pocket" if "treatments" in str(directory) else "initial",
                 "source_sha256": model["source_sha256"], "blueprint_sha256": model["blueprint_sha256"],
                 "attempted": len(candidates), "admitted": sum(r["admitted"] for r in candidates),
                 "rendered": sum("residual" in r for r in candidates),
                 "refused": sum(r["generation_status"] == "refused" for r in candidates),
                 "rejected": sum(r["generation_status"] == "rejected" for r in candidates),
                 "winner": result["ranking"]["winner"], "diagnostic_candidate": diagnostic,
                 "world": selected["world"] if selected else None,
                 "residual": selected.get("residual") if selected else None,
                 "production_residual": selected.get("production_residual") if selected else None,
                 "raw_wav": str(directory/"best"/"raw_humanmusic.wav") if selected else None,
                 "listening_status": result.get("perceptual_quality", "UNVERIFIED"),
                 "maintainer_listening": result.get("maintainer_listening", [])}
        if selected:
            residual = json.loads((Path(selected["path"])/"residuals.json").read_text())
            entry["local_motif_metric_fraction"] = residual.get("motif_occurrences", {}).get("metric_fraction")
            entry["local_bass_metric_fraction"] = residual.get("bass_occurrences", {}).get("metric_fraction")
            if plots:
                make_plot(directory, model, selected)
        rows.append(entry)
    initial = [r for r in rows if r["treatment"] == "initial"]
    summary = {"schema": "sai.round2_summary/v1", "sources": len(initial), "tracks": rows,
               "totals": {key: sum(r[key] for r in rows) for key in ("attempted", "admitted", "rendered", "refused", "rejected")},
               "structural_winners": sum(r["winner"] is not None for r in rows),
               "boundary": "Private measurement corpus; observed line agreement is not independently validated source correctness or human recognition; partial diagnostic lifts retain full-profile refusals"}
    summary["dynamic_ensemble_controls"] = [json.loads(p.read_text()) for p in
        sorted(root.glob("track-*/dynamic_ensemble_control/*/OBSERVATIONAL_RESULT.json"))]
    feedback = root/"LISTENING_FEEDBACK.json"
    if feedback.exists():
        summary["maintainer_listening"] = json.loads(feedback.read_text())
    calibration = root/"calibration"/"STEM_CALIBRATION.json"
    if calibration.exists():
        summary["synthetic_calibration"] = json.loads(calibration.read_text())
    write_json(root/"ROUND2_SUMMARY.json", summary)
    if export:
        write_json(export, summary)
    lines = ["# Round-2 listening queue", "", "No perceptual quality claim. See each directory's refusal and residual receipts.", "",
             "| Source / treatment | Diagnostic world | Admitted candidates | WAV |", "| --- | --- | ---: | --- |"]
    lines += [f"| {r['source_id']} / {r['treatment']} | {r['world']} | {r['admitted']} | {r['raw_wav']} |" for r in rows]
    (root/"LISTENING_QUEUE.md").write_text("\n".join(lines)+"\n")
    return summary


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("root", type=Path)
    p.add_argument("--export", type=Path)
    p.add_argument("--plots", action="store_true")
    args = p.parse_args()
    summarize(args.root, args.export, args.plots)

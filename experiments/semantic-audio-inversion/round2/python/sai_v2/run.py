"""Run or replay real-song candidate observations and produce a local listening queue."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

from .calibrate import observe_set
from .evaluate import compare, gates
from .observe import production_features, write_json, cached_observe
from .production import distance, fit_stems
from .rank import rank_candidates, structurally_valid


def produce_if_eligible(source, candidate, observations, checks, admitted, preroll=0.):
    """Production never runs for a failed structural or generator admission gate."""
    if admitted is not True or not structurally_valid({"gates": checks}):
        return fit_stems({}, {}, candidate/"processed_mix.wav", structural_pass=False)
    track = Path(source["provenance"]["blueprint"]["artifact"]).parents[1]
    aliases = {"lead": "vocals", "bass": "bass", "drums": "drums", "support": "other"}
    targets = {}
    for role, label in aliases.items():
        matches = list((track/"separation").glob(f"htdemucs_ft/{label}.wav"))
        if len(matches) == 1:
            targets[role] = production_features(matches[0])
    stems = {role: candidate/"stems"/(role+".wav") for role in ("lead", "bass", "drums", "support", "sfx")}
    receipt = fit_stems(stems, targets, candidate/"processed_mix.wav", structural_pass=True)
    after = dict(observations)
    for role in ("full", "lead", "bass", "drums", "support"):
        wav = candidate/"processed_mix.wav" if role == "full" else candidate/"processed_stems"/(role+".wav")
        after[role] = cached_observe(wav, candidate/"processed_observations"/(role+".json"), role)
    residual = compare(source, after, preroll)
    receipt["structural_acceptance_after_processing"] = all(gates(residual).values())
    write_json(candidate/"processed_residuals.json", residual)
    write_json(candidate/"processed_mix.production.json", receipt)
    return receipt


def brief(residual):
    return {"timing_relative_error": residual["timing"]["relative_error"],
            "lead_note_f1": residual["lead"]["f1"], "bass_note_f1": residual["bass"]["f1"],
            "lead_octave_f1": residual["lead_octave_quotient"]["f1"],
            "bass_octave_f1": residual["bass_octave_quotient"]["f1"],
            "motif": residual["motif"].get("relation"),
            "motif_relative_f1": residual["motif"].get("relative_note_match", {}).get("f1"),
            "bass": residual["bass_figure"].get("relation"),
            "harmony": residual["harmony"].get("relation"),
            "harmony_root_agreement": residual["harmony"].get("root_agreement"),
            "harmony_family_agreement": residual["harmony"].get("family_agreement"),
            "groove": residual["groove"].get("relation"),
            "groove_f1": residual["groove"].get("full", {}).get("f1"),
            "form": residual["form"].get("relation")}


def evaluate_track(directory):
    d = Path(directory)
    source_file = d/"compiled_source_model.json"
    if not source_file.exists():
        source_file = d/"renderer_source_snapshot.json"
    source = json.loads(source_file.read_text())
    receipt_file = d/"source_receipt.json"
    if not receipt_file.exists():
        receipt_file = d.parent.parent/"source_receipt.json"
    receipt = json.loads(receipt_file.read_text())
    manifest = json.loads((d/"candidate_manifest.json").read_text())
    source_production = production_features(receipt["source_path"])
    write_json(d/"source_production_observation.json", source_production)
    rows = []
    for i, entry in enumerate(manifest["candidates"], 1):
        c = Path(entry.get("path", d/"candidates"/f"candidate-{i:03}"))
        conformance = json.loads((c/"conformance.json").read_text())
        config = json.loads((c/"config.json").read_text())
        row = {"candidate_id": c.name, "path": str(c), "admitted": conformance["admitted"],
               "world": config["world"], "seed": config["seed"], "gates": {},
               "generation_status": conformance["status"]}
        if not (c/"raw_mix.wav").exists():
            row.update(gates={"rendered": False}, refusal=conformance.get("error", conformance.get("failures")))
            write_json(c/"residuals.json", {"status": "not_rendered", "reason": row["refusal"]})
            rows.append(row)
            continue
        observations = observe_set(c, frozen=False)
        residual = compare(source, observations, config.get("preroll_beats", 0.))
        # The fullmix v1 transcription route is reserved for the matched dev contrast;
        # not executed on real candidates, so absence must not masquerade as zero F1.
        for key in list(residual):
            if key.startswith("frozen_notes"):
                residual.pop(key)
        checks = gates(residual)
        checks["all_source_axes_represented"] = not config.get("unrepresented_observed_axes")
        residual["gates"] = checks
        residual["internal_admission"] = conformance["admitted"]
        prod = distance(source_production, observations["full"]["production"])
        residual["production"] = prod
        write_json(c/"residuals.json", residual)
        row.update(gates=checks, residual=brief(residual), production_residual=prod,
                   musical_residual=[1-residual["lead_octave_quotient"]["f1"], 1-residual["bass_octave_quotient"]["f1"],
                                     1-residual["harmony"].get("root_agreement", 0)])
        # All actual fits are gated. Failure still emits the explicit production receipt.
        row["production_status"] = produce_if_eligible(source, c, observations, checks, conformance["admitted"], config.get("preroll_beats", 0.))["status"]
        rows.append(row)
        print(json.dumps({"source": d.name, **row}), flush=True)
    ranking = rank_candidates(rows)
    # A rejected render can be offered for diagnosis, but cannot become a winner.
    diagnostic = ranking["diagnostic_best"] or next((r["candidate_id"] for r in rows if "residual" in r), None)
    report = {"source_id": d.name, "source_receipt": receipt,
              "candidates": rows, "ranking": ranking, "listening_diagnostic": diagnostic,
              "structural_success": ranking["winner"] is not None,
              "perceptual_quality": "UNVERIFIED: maintainer listening required"}
    write_json(d/"ROUND2_RESULT.json", report)
    reports = d/"reports"
    reports.mkdir(exist_ok=True)
    stale_produced = d/"best"/"produced_cover.wav"
    if stale_produced.exists() or stale_produced.is_symlink():
        stale_produced.unlink()
    if diagnostic:
        chosen = next(r for r in rows if r["candidate_id"] == diagnostic)
        best = d/"best"
        best.mkdir(exist_ok=True)
        source_wav = d/"candidates"/diagnostic/"raw_mix.wav"
        destination = best/"raw_humanmusic.wav"
        if destination.exists() or destination.is_symlink():
            destination.unlink()
        destination.symlink_to(source_wav)
        for name in ("config.json", "residuals.json"):
            shutil.copyfile(source_wav.parent/name, best/name)
        status = ("Structurally eligible under the declared observational gates; listener recognition remains unverified."
                  if ranking["winner"] else "No structurally eligible winner. This is a diagnostic listening candidate.")
        produced = source_wav.parent/"processed_mix.wav"
        produced_receipt = source_wav.parent/"processed_mix.production.json"
        if ranking["winner"] and produced_receipt.exists() and json.loads(produced_receipt.read_text()).get("structural_acceptance_after_processing") is True:
            destination_produced = best/"produced_cover.wav"
            if destination_produced.exists() or destination_produced.is_symlink():
                destination_produced.unlink()
            destination_produced.symlink_to(produced)
        (best/"WHY_THIS_WON.md").write_text(status + "\n\n" +
            f"Selected {diagnostic}; internal admission={chosen['admitted']}. " +
            "Admission and structural failures outrank production. See residuals.json.\n")
        notes = [f"# Listening queue: {d.name}", "", status, "",
                 f"Raw HumanMusic: `{destination}`", "",
                 "Listen against the source's corresponding metric positions; measured beat warping makes wall-clock timestamps differ.",
                 "Check the first lead entry, each repeated phrase, bass continuity, and section changes. No listener has certified these renders.", "",
                 ("Produced cover passed its post-processing re-analysis; listening remains required."
                  if (best/"produced_cover.wav").exists() else "The produced_cover.wav is withheld because observational structure has not passed. Raw rejected audio is for diagnosis only."), "",
                 "| Candidate | World | Admitted | Audio |", "| --- | --- | --- | --- |"]
        lead_notes = [n for n in source["notes"] if n["role"] == "lead"]
        if lead_notes:
            entry = lead_notes[0]["onset_beat"]*60/source["tempo_bpm"]
            notes.insert(7, f"First derived lead entry in cover: {entry:.2f}s. Source entry: {lead_notes[0].get('onset_sec', entry):.2f}s.")
        notes += [f"| {r['candidate_id']} | {r['world']} | {r['admitted']} | {r['path']}/raw_mix.wav |"
                  for r in rows if "residual" in r]
        (reports/"LISTENING_NOTES.md").write_text("\n".join(notes)+"\n")
    (reports/"ROUND2_REPORT.md").write_text(
        f"# {d.name}: measured Round-2 result\n\n"+
        f"Structurally eligible winner: {ranking['winner']}. Perceptual quality UNVERIFIED.\n\n"+
        "Complete per-axis values, refusal receipts and candidate order: ../ROUND2_RESULT.json.\n"+
        "Production is deferred while musical gates fail; internal cover conformance is not observational identity.\n")
    return report


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("directory", type=Path)
    p.add_argument("--renderer", type=Path)
    args = p.parse_args()
    if args.renderer:
        subprocess.run([str(args.renderer.resolve()), "cover", str(args.directory/"compiled_source_model.json"),
                        str(args.directory), "220901,220902", "22050"], check=True)
    evaluate_track(args.directory)

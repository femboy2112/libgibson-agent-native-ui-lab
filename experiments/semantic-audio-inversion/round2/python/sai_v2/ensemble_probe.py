"""Observe an unfitted measurement-playback control, never a checked-cover winner."""
import argparse
import hashlib
import json
from pathlib import Path

from .calibrate import observe_set
from .evaluate import compare, gates, seconds_source
from .observe import write_json
from .run import brief


def run(directory):
    directory = Path(directory)
    config = json.loads((directory/"config.json").read_text())
    if (config.get("scope") != "measurement_playback_unfitted_control"
            or config.get("source_production_fit_applied") is not False
            or config.get("transport") != "source_seconds"):
        raise ValueError("only declared unfitted, source-seconds controls are accepted")
    observations = observe_set(directory, frozen=False)
    source_bytes = (directory.parent/"source_snapshot.json").read_bytes()
    if hashlib.sha256(source_bytes).hexdigest() != config["input_sha256"]:
        raise ValueError("measurement-playback source snapshot hash mismatch")
    source = json.loads(source_bytes)
    residual = compare(seconds_source(source), observations)
    for key in list(residual):
        if key.startswith("frozen_notes"):
            residual.pop(key)
    residual.update(gates=gates(residual), public_cover_admission="not_applicable",
                    transport="source seconds, scaled by source BPM only for comparison units",
                    boundary="Direct measurement playback diagnoses extraction/arrangement; not an admitted fresh cover")
    write_json(directory/"residuals.json", residual)
    summary = {"source_id": source["source_id"], "world": config["world"],
               "wav": str(directory/"raw_mix.wav"), "gates": residual["gates"],
               "residual": brief(residual), "public_cover_admission": "not_applicable"}
    write_json(directory/"OBSERVATIONAL_RESULT.json", summary)
    print(json.dumps(summary), flush=True)
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    run(parser.parse_args().directory)

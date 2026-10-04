"""Controlled role ablation: no pitch/timing edits or source-production fitting."""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import soundfile as sf

from .observe import write_json


def run(directory):
    directory = Path(directory)
    config = json.loads((directory/"config.json").read_text())
    if config.get("scope") != "measurement_playback_unfitted_control":
        raise ValueError("ablation requires the unfitted measurement-playback control")
    roles, rate, length = {}, None, None
    for role in ("lead", "bass", "drums", "support"):
        path = directory/"stems"/(role+".wav")
        y, sr = sf.read(path, dtype="float32", always_2d=True)
        if rate is not None and (sr != rate or len(y) != length):
            raise ValueError("stems must be exactly aligned")
        roles[role] = y
        rate, length = sr, len(y)
    out = directory/"ablations"
    out.mkdir(exist_ok=True)
    receipts = []
    for name, selected in (("lead_only", ("lead",)),
                           ("rhythm_lead", ("lead", "bass", "drums")),
                           ("support_only", ("support",))):
        y = sum((roles[role] for role in selected), np.zeros_like(roles["lead"]))
        peak = float(np.max(np.abs(y)))
        # FLOAT preserves the original shared gain without clipping or remastering.
        path = out/(name+".wav")
        sf.write(path, y, rate, subtype="FLOAT")
        receipts.append({"path": str(path), "roles": selected, "peak": peak,
                         "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    receipt = {"schema": "sai.role_ablation/v1", "source_id": config["source_id"],
               "world": config["world"], "config": config, "outputs": receipts,
               "boundary": "Same rendered stems, sample clock and gain; only role inclusion changes. No checked-cover admission or quality claim."}
    write_json(out/"receipt.json", receipt)
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    run(parser.parse_args().directory)

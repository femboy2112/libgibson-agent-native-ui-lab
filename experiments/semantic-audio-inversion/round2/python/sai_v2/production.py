"""Optional bounded production fit, gated by observational structural acceptance."""
from pathlib import Path
import math

import numpy as np
import soundfile as sf
from scipy import signal

from .observe import production_features, write_json


def distance(source, candidate):
    """Named residual vector; no waveform error or fake musical quality score."""
    return {
        "rms_db_abs": abs(source["rms_db"]-candidate["rms_db"]),
        "crest_db_abs": abs(source["crest_db"]-candidate["crest_db"]),
        "centroid_octaves_abs": abs(math.log2(max(source["centroid_hz"], 1)/max(candidate["centroid_hz"], 1))),
        "width_abs": abs(source["width"]-candidate["width"]),
        "pan_abs": abs(source["pan"]-candidate["pan"]),
        "bands_l1": sum(abs(a-b) for a, b in zip(source["bands_low_mid_high"], candidate["bands_low_mid_high"])),
    }


def fit_stems(stems, targets, out, *, structural_pass):
    """Keep sample alignment; measured gain/width and three-band power adjustment.

    Targets are measured with this same production_features implementation. No named
    gear, reverb estimate, or source score enters. Bounds are global, not per-song.
    Post-fit structure MUST be re-observed by the caller before acceptance.
    """
    out = Path(out)
    if not structural_pass:
        refusal = {"status": "refused", "reason": "observational structural gates failed; production cannot compensate"}
        write_json(out.with_suffix(".production.json"), refusal)
        return refusal
    rendered, roles, controls, reference_sr, reference_len = [], [], {}, None, None
    for role, path in sorted(stems.items()):
        roles.append(role)
        y, sr = sf.read(path, always_2d=True)
        if not np.isfinite(y).all():
            raise ValueError("non-finite PCM")
        if reference_sr is not None and (sr != reference_sr or len(y) != reference_len):
            raise ValueError("stems are not sample aligned")
        reference_sr, reference_len = sr, len(y)
        before = production_features(path)
        target = targets.get(role)
        if target is None:
            rendered.append(y)
            controls[role] = {"status": "unfitted", "reason": "no measured source target"}
            continue
        for key in ("rms_db", "width", "pan", "centroid_hz"):
            if not math.isfinite(target[key]):
                raise ValueError("non-finite target")
        gains = np.clip(np.sqrt((np.asarray(target["bands_low_mid_high"])+1e-8)/
                                (np.asarray(before["bands_low_mid_high"])+1e-8)), .5, 2)
        low = signal.sosfilt(signal.butter(2, 250, fs=sr, output="sos"), y, axis=0)
        below_high = signal.sosfilt(signal.butter(2, 2500, fs=sr, output="sos"), y, axis=0)
        y = gains[0]*low + gains[1]*(below_high-low) + gains[2]*(y-below_high)
        width_gain = float(np.clip(target["width"]/max(before["width"], .05), .5, 2))
        mid, side = (y[:, 0]+y[:, -1])/2, (y[:, 0]-y[:, -1])/2
        y = np.column_stack((mid+width_gain*side, mid-width_gain*side))
        pan_delta = float(np.clip(target["pan"]-before["pan"], -.5, .5))
        y *= np.array([math.sqrt(1-pan_delta), math.sqrt(1+pan_delta)])
        desired = 10**(target["rms_db"]/20)
        gain = float(np.clip(desired/max(float(np.sqrt(np.mean(y*y))), 1e-10), .25, 4))
        rendered.append(y*gain)
        controls[role] = {"band_amplitude_gains": gains.tolist(), "gain": gain,
                          "width_gain": width_gain, "pan_delta": pan_delta,
                          "before": before, "target": target}
    if not rendered:
        raise ValueError("no stems")
    mix = np.sum(rendered, axis=0)
    headroom_gain = min(1., .95/max(float(np.max(np.abs(mix))), 1e-10))
    mix *= headroom_gain
    out.parent.mkdir(parents=True, exist_ok=True)
    sf.write(out, mix, reference_sr, subtype="PCM_16")
    processed = out.parent/"processed_stems"
    processed.mkdir(exist_ok=True)
    for role, stem in zip(roles, rendered):
        sf.write(processed/(role+".wav"), stem*headroom_gain, reference_sr, subtype="PCM_16")
    receipt = {"status": "processed_pending_reanalysis", "controls": controls,
               "headroom_gain": headroom_gain, "after": production_features(out),
               "sample_aligned": True, "structural_acceptance_after_processing": "UNVERIFIED"}
    write_json(out.with_suffix(".production.json"), receipt)
    return receipt

"""External ML/audio adapters: measurement instruments behind a refusal gate.

An external model is never a semantic authority here. It may only *emit* a versioned evidence
artifact, and it must declare its weights/version. When a model is not installed or cannot run
within the resource envelope, the adapter **refuses explicitly** — the absence is recorded as an
access/compute gap in the artifact, never filled by a default and never silently skipped.

Availability is probed by import and by a trivial forward pass where cheap. Nothing here is
called by `cargo test`.
"""

from __future__ import annotations

import importlib
import importlib.metadata as md
from dataclasses import dataclass
from typing import List, Optional

import sai_evidence as ev


@dataclass
class ExternalProbe:
    name: str
    module: str
    available: bool
    version: Optional[str]
    reason: Optional[str]


# Candidate instruments, cheapest probe first. These are *optional*: the baseline DSP is the
# primary route this round, and each of these was expected to be a compute/access gap on this
# host (~7 GB RAM, no GPU, none preinstalled).
CANDIDATES = [
    ("librosa", "librosa"),
    ("soundfile", "soundfile"),
    ("essentia", "essentia"),
    ("basic_pitch", "basic_pitch"),
    ("beat_this", "beat_this"),
    ("torch", "torch"),
    ("madmom", "madmom"),
]


def probe(name: str, module: str) -> ExternalProbe:
    try:
        importlib.import_module(module)
    except Exception as e:  # ImportError or a native init failure
        return ExternalProbe(name, module, False, None, f"import failed: {type(e).__name__}: {e}")
    version = None
    try:
        version = md.version(module)
    except Exception:
        version = "unknown"
    return ExternalProbe(name, module, True, version, None)


def probe_all() -> List[ExternalProbe]:
    return [probe(n, m) for n, m in CANDIDATES]


def refusals_for_unavailable() -> List[dict]:
    """One explicit refusal per unavailable instrument, for the evidence artifact."""
    out = []
    for p in probe_all():
        if not p.available:
            out.append(
                ev.refusal(
                    what=f"external:{p.name}",
                    reason=(
                        f"instrument not available on this host ({p.reason}); "
                        "baseline DSP remains the primary route"
                    ),
                )
            )
    return out


def probe_report() -> dict:
    probes = probe_all()
    return {
        "probes": [
            {"name": p.name, "module": p.module, "available": p.available, "version": p.version, "reason": p.reason}
            for p in probes
        ],
        "any_available": any(p.available for p in probes),
        "note": "External adapters emit evidence only; they never arbitrate the quotient.",
    }


if __name__ == "__main__":
    import json

    print(json.dumps(probe_report(), indent=2))

"""Deterministic lexicographic selection. Ineligible items cannot win."""
import math

REQUIRED = {"timing", "motif", "bass", "harmony", "groove", "form"}


def structurally_valid(c):
    return REQUIRED <= c["gates"].keys() and all(v is True for v in c["gates"].values())


def rank_candidates(candidates):
    def key(c):
        gates = c["gates"]
        prod = c.get("production_residual", {})
        musical = c.get("musical_residual", [])
        values = list(musical) + list(prod.values())
        if any(not isinstance(v, (int, float)) or not math.isfinite(v) for v in values):
            raise ValueError("invalid ranking residual")
        return (c["admitted"] is not True, not structurally_valid(c),
                sum(not v for v in gates.values()), tuple(musical),
                tuple(prod[k] for k in sorted(prod)), c["candidate_id"])
    ordered = sorted(candidates, key=key)
    eligible = [c for c in ordered if c["admitted"] is True and structurally_valid(c)]
    return {"order": [c["candidate_id"] for c in ordered],
            "winner": eligible[0]["candidate_id"] if eligible else None,
            "diagnostic_best": next((c["candidate_id"] for c in ordered if c["admitted"]), None),
            "policy": "admission, all structural gates, failures, musical residual vector, production residual vector, stable id"}

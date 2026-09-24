#!/usr/bin/env python3
"""Fail closed on changes to the substrate or pre-holdout helper/IR files."""
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
record = json.loads((ROOT / "docs/FREEZE.json").read_text())
failures = []
for name, digest in record["files"].items():
    actual = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    if actual != digest:
        failures.append(name)
    # The receipt must describe the actual committed helper version as well.
    original = subprocess.check_output(
        ["git", "show", record["lab_helper_ir_sha"] + ":" + name], cwd=ROOT
    )
    if hashlib.sha256(original).hexdigest() != digest:
        failures.append(name + " (receipt does not match frozen commit)")
assert not failures, "Frozen API/helper drift: " + ", ".join(failures)
assert record["libgibson_sha"] in (ROOT / "Cargo.toml").read_text()
assert record["libgibson_sha"] in (ROOT / "Cargo.lock").read_text()
print("PASS: pinned substrate and", len(record["files"]), "frozen files unchanged")
